#![allow(
    clippy::cast_lossless,
    clippy::match_bool // Matching on bools makes things cleaner sometimes
)]

pub mod devices;
pub mod io;
pub mod macros;
pub mod memory;
pub mod ts7200;

use std::net::{TcpListener, TcpStream};

use ts7200::Ts7200;

use log::*;

fn new_tcp_gdbstub<T: gdbstub::Target>(
    port: u16,
) -> std::io::Result<gdbstub::GdbStub<T, TcpStream>> {
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

    let file = std::fs::File::open(args.get(1).expect("must provide .elf to load"))?;
    let mut system = Ts7200::new_hle(file)?;

    // hook up UARTs to I/O
    // TODO: add CLI params to hook UARTs up to arbitrary files (e.g: named pipe)

    // uart1 is for trains
    match (args.get(2), args.get(3)) {
        (Some(in_path), Some(out_path)) => {
            let check_shortcut = |path| match path {
                "-" => "/dev/null",
                _ => path,
            };
            let in_path = check_shortcut(in_path);
            let out_path = check_shortcut(out_path);
            let uart1 = &mut system.devices_mut().uart1;

            uart1
                .install_io(|tx, rx| {
                    let _ = crate::io::file::spawn_reader_thread(in_path, tx)?;
                    let writer = crate::io::file::spawn_writer_thread(out_path, rx)?;
                    Ok((None, Some(writer)))
                })
                .unwrap();
            // TODO PRILLIIIIK
        }
        (_, _) => {}
    }

    // uart2 is for console communication
    // Unused variable here to ensure this doesn't get dropped until
    // we exit.
    {
        let uart2 = &mut system.devices_mut().uart2;

        uart2
            .install_io(|rx, tx| {
                let (_, writer) = crate::io::stdio::spawn_threads(rx, tx);
                Ok((None, Some(writer)))
            })
            .unwrap();
        // TODO PRILLIIIIK
    };

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
        eprintln!("Fatal Error!");
        eprintln!("============");
        eprintln!("{:#010x?}", system);
        eprintln!("{:#010x?}", fatal_error);
        eprintln!("============");
        return Err("Fatal Error!".into());
    }

    Ok(())
}
