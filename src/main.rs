#![allow(clippy::cast_lossless)]

pub mod devices;
pub mod macros;
pub mod memory;
pub mod ts7200;

use std::net::{TcpListener, TcpStream};

use ts7200::Ts7200;

use log::*;

fn new_tcp_gdbstub<T: gdbstub::Target>(
    port: u16,
) -> std::io::Result<gdbstub::GdbStub<T, TcpStream>> {
    let sockaddr = format!("localhost:{}", port);
    info!("Waiting for a GDB connection on {:?}...", sockaddr);

    let sock = TcpListener::bind(sockaddr)?;
    let (stream, addr) = sock.accept()?;
    info!("Debugger connected from {}", addr);

    Ok(gdbstub::GdbStub::new(stream))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    pretty_env_logger::init();

    let args: Vec<String> = std::env::args().collect();

    let file = std::fs::File::open(args.get(1).expect("must provide .elf to load"))?;
    let mut system = Ts7200::new_hle(file)?;

    // hook up UARTs to I/O
    // TODO: add CLI params to hook UARTs up to arbitrary files (e.g: named pipe)

    // uart1 is for trains
    // uart2 is for console communication
    system
        .devices_mut()
        .uart2
        .set_reader(Some(Box::new(std::io::stdin())));
    system
        .devices_mut()
        .uart2
        .set_writer(Some(Box::new(std::io::stdout())));

    let debugger = match args.get(2) {
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
        // TODO: spin up GDB session if a interrupt isn't handled
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
