use std::error::Error as StdError;
use std::fs;
use std::net::{TcpListener, TcpStream};

use log::*;
use structopt::StructOpt;

pub mod devices;
pub mod memory;
pub mod sys;

use crate::devices::uart;
use crate::sys::ts7200::Ts7200;

#[derive(StructOpt)]
#[structopt(name = "ts7200")]
#[structopt(about = r#"
An emulator for the TS-7200 Single Board Computer, as used in CS 452 at the
University of Waterloo.

UART CONFIGURATION:
    The `--uartX` flags accept a configuration string. The format is closely
    modeled after QEMU's `-serial` flag:

    * none
        - No device is connected
    * file:/path/to/output[,in=/path/to/input]
        - Write output to the specified file
        - Read input from the specified file
    * stdio
        - Use the process's stdin / stdout
        - Sets the terminal to "raw" mode
    * tcp:[host]:port
        - Connect to a tcp server
        - "host" defaults to localhost

    e.g: `--uart1=file:/dev/null,in=/tmp/trainin.pipe`, `--uart1=tcp::3018`

HACKS:
    --hack-inf-uart-rx    Work around for using the MarklinSim with the current
                          "always-on" CTS implementation.
"#)]
struct Args {
    /// kernel ELF file to load
    kernel_elf: String,

    /// spawn a gdb server listening on the specified port
    #[structopt(short, long)]
    gdbport: Option<u16>,

    /// UART1 configuration.
    #[structopt(long, default_value = "none")]
    uart1: uart::UartCfg,

    /// UART2 configuration.
    #[structopt(long, default_value = "stdio")]
    uart2: uart::UartCfg,

    /// HACK: Give UARTs infinite rx FIFOs.
    #[structopt(long)]
    hack_inf_uart_rx: bool,
}

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

fn main() -> Result<(), Box<dyn StdError>> {
    pretty_env_logger::formatted_builder()
        .filter(None, LevelFilter::Debug)
        .filter(Some("armv4t_emu"), LevelFilter::Debug)
        .parse_filters(&std::env::var("RUST_LOG").unwrap_or_default())
        .init();

    let args = Args::from_args();

    // create the base system
    let file = fs::File::open(args.kernel_elf)?;
    let mut system = Ts7200::new_hle(file)?;

    // hook up the uarts
    args.uart1.apply(&mut system.devices_mut().uart1)?;
    args.uart2.apply(&mut system.devices_mut().uart2)?;

    // apply hax
    if args.hack_inf_uart_rx {
        system.devices_mut().uart1.hack_set_infinite_rx(true);
        system.devices_mut().uart2.hack_set_infinite_rx(true);
    }

    // (potentially) spin up the debugger
    let debugger = match args.gdbport {
        Some(port) => Some(new_tcp_gdbstub(port)?),
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
