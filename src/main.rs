pub mod devices;
pub mod iobridge;
pub mod memory;
pub mod ts7200;

use std::fs;
use std::net::{TcpListener, TcpStream};

use devices::uart;
use ts7200::Ts7200;

use log::*;

fn new_tcp_gdbstub<T: gdbstub::Target>(
    port: u16,
) -> Result<gdbstub::GdbStub<T, TcpStream>, std::io::Error> {
    let sockaddr = format!("127.0.0.1:{}", port);
    info!("Waiting for a GDB connection on {:?}...", sockaddr);

    let sock = TcpListener::bind(sockaddr)?;
    let (stream, addr) = sock.accept()?;
    info!("Debugger connected from {}", addr);

    Ok(gdbstub::GdbStub::new(stream))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    pretty_env_logger::init();

    let args: Vec<String> = std::env::args().collect();
    if args.len() != 2 && args.len() != 4 && args.len() != 5 {
        panic!("Usage: ts7200 ELF_BINARY <TRAIN_STDIN> <TRAIN_STDOUT> <GDBPORT>");
    }

    let file = fs::File::open(args.get(1).expect("must provide .elf to load"))?;
    let mut system = Ts7200::new_hle(file)?;

    // uart1 is for trains
    match (args.get(2), args.get(3)) {
        (Some(in_path), Some(out_path)) => {
            let check_shortcut = |path| match path {
                "-" => "/dev/null",
                _ => path,
            };
            let in_path = check_shortcut(in_path);
            let out_path = check_shortcut(out_path);

            system
                .devices_mut()
                .uart1
                .install_io_tasks(|tx, rx| -> Result<_, std::io::Error> {
                    let in_file = fs::File::open(in_path)?;
                    let out_file = fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(out_path)?;

                    let in_thread = iobridge::reader_to_chan(in_path.to_string(), in_file, tx)?;
                    let out_thread = iobridge::writer_to_chan(out_path.to_string(), out_file, rx)?;
                    Ok((
                        uart::ReaderTask::new(in_thread),
                        uart::WriterTask::new(out_thread),
                    ))
                })?;
        }
        (_, _) => {}
    }

    // uart2 is for console communication
    system
        .devices_mut()
        .uart2
        .install_io_tasks(|tx, rx| {
            let (in_thread, out_thread) = iobridge::stdio_to_chans(tx, rx);
            Ok((
                uart::ReaderTask::new(in_thread),
                uart::WriterTask::new(out_thread),
            ))
        })
        .map_err(|_: ()| "could not connect stdio to UART2")?;

    let debugger = match args.get(4) {
        Some(port) => Some(new_tcp_gdbstub(
            port.parse().map_err(|_| "invalid gdb port")?,
        )?),
        None => None,
    };

    let system_result = match debugger {
        // hand off control to the debugger
        Some(mut debugger) => match debugger.run(&mut system) {
            Ok(state) => {
                eprintln!("Disconnected from GDB. Target state: {:?}", state);
                // TODO: if the debugging session is closed, but the system isn't halted,
                // execution should continue.
                Ok(())
            }
            Err(gdbstub::Error::TargetError(e)) => Err(e),
            Err(e) => return Err(e.into()),
        },
        // just run the system until it finishes, or an error occurs
        None => system.run(),
    };

    if let Err(fatal_error) = system_result {
        eprintln!("Fatal Error! Dumping system state...");
        eprintln!("============");
        eprintln!("{:#010x?}", system);
        eprintln!("Cause: {:#010x?}", fatal_error);
        eprintln!("============");
        return Err("Fatal Error!".into());
    }

    Ok(())
}
