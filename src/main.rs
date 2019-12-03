#![allow(clippy::cast_lossless)]

pub mod devices;
pub mod gdbstub;
pub mod macros;
pub mod memory;
pub mod ts7200;

use ts7200::Ts7200;

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
        Some(port) => Some(gdbstub::GdbStub::new(format!("localhost:{}", port))?),
        None => None,
    };

    let system_result = match debugger {
        // hand off control to the debugger
        // TODO: if the debugging session is closed, the system should keep running.
        Some(mut debugger) => debugger.run(&mut system).map(|_| ()),
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
