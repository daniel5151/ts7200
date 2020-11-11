#[macro_use]
extern crate log;

use std::error::Error as StdError;
use std::fs;
use std::net::{TcpListener, TcpStream};

use gdbstub::{DisconnectReason, GdbStub, GdbStubError};
use log::LevelFilter;
use structopt::StructOpt;

pub mod devices;
pub mod memory;
pub mod sys;
pub mod util;

use crate::devices::uart;
use crate::sys::ts7200::Ts7200;

const SYSDUMP_FILENAME: &str = "sysdump.log";

#[derive(StructOpt)]
#[structopt(name = "ts7200")]
#[structopt(about = r#"
An emulator for the TS-7200 Single Board Computer, as used in CS 452 at the
University of Waterloo. Written by Daniel Prilik <danielprilik@gmail.com>
and Sean Purcell <me@seanp.xyz>.

                ********************************************
                * IF LOGS ARE SMEARING ACROSS THE TERMINAL *
                *             READ THE README!             *
                ********************************************

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
        - "host" defaults to 127.0.0.1 (localhost)

    e.g: `--uart1=file:/dev/null,in=/tmp/trainin.pipe`, `--uart1=tcp::3018`

HACKS:
    These hacks should be used with extreme caution, as they greatly compromise
    the emulator's accuracy.

    --hack-inf-uart-rx=[1|2]
        Gives the specified UART an infinite rx FIFO. This hack allows the
        MarklinSim to work properly ts7200's "always-on" CTS implementation.

    --hack-nodelay-uart-tx=[1|2]
        Disables all tx output delay on the specified UART. This hack is useful
        for `bwprintf` debugging time sensitive code (as at the time of writing,
        the GDB server doesn't actually "stop the clock" when paused).
"#)]
struct Args {
    /// kernel ELF file to load
    kernel_elf: String,

    /// Spawn a gdb server listening on the specified port
    #[structopt(short, long, value_name = "port")]
    gdbport: Option<u16>,

    /// UART1 configuration.
    #[structopt(long, value_name = "cfg", default_value = "none")]
    uart1: uart::UartCfg,

    /// UART2 configuration.
    #[structopt(long, value_name = "cfg", default_value = "stdio")]
    uart2: uart::UartCfg,

    /// HACK: Give UARTs infinite rx FIFOs.
    #[structopt(long, value_name = "uart")]
    hack_inf_uart_rx: Vec<usize>,

    /// HACK: Disables all tx output delay on the UARTs.
    #[structopt(long, value_name = "uart")]
    hack_nodelay_uart_tx: Vec<usize>,

    /// Disable all Address Sanitizer warnings from the RAM.
    #[structopt(long)]
    no_asan_ram: bool,
}

fn wait_for_tcp(port: u16) -> Result<TcpStream, Box<dyn StdError>> {
    let sockaddr = format!("127.0.0.1:{}", port);
    eprintln!("Waiting for a GDB connection on {:?}...", sockaddr);

    let sock = TcpListener::bind(sockaddr)?;
    let (stream, addr) = sock.accept()?;
    eprintln!("Debugger connected from {}", addr);

    Ok(stream)
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
    {
        let devices = system.devices_mut();
        let uart1 = &mut devices.uart1;
        let uart2 = &mut devices.uart2;

        for uart in args.hack_inf_uart_rx {
            match uart {
                1 => uart1.hack_inf_uart_rx(true),
                2 => uart2.hack_inf_uart_rx(true),
                _ => return Err("invalid uart".into()),
            }
        }

        for uart in args.hack_nodelay_uart_tx {
            match uart {
                1 => uart1.hack_nodelay_uart_tx(true),
                2 => uart2.hack_nodelay_uart_tx(true),
                _ => return Err("invalid uart".into()),
            }
        }
    }

    // asan ram
    system.devices_mut().sdram.set_asan(!args.no_asan_ram);

    // (potentially) spin up the debugger
    let mut debugger = match args.gdbport {
        Some(port) => Some(GdbStub::new(wait_for_tcp(port)?)),
        None => None,
    };

    let system_result = match debugger {
        // hand off control to the debugger
        Some(ref mut debugger) => match debugger.run(&mut system) {
            Ok(DisconnectReason::Disconnect) => {
                eprintln!("Disconnected from GDB. Shutting down.");
                system.run()
            }
            Ok(DisconnectReason::TargetHalted) => {
                eprintln!("Target halted!");
                Ok(())
            }
            Ok(DisconnectReason::Kill) => {
                eprintln!("GDB sent a kill command!");
                return Ok(());
            }
            Err(GdbStubError::TargetError(e)) => Err(e),
            Err(e) => return Err(e.into()),
        },
        // just run the system until it finishes, or an error occurs
        None => system.run(),
    };

    if let Err(fatal_error) = system_result {
        error!("Fatal Error! Caused by: {:#010x?}", fatal_error);
        error!("Dumping system state to {}", SYSDUMP_FILENAME);

        std::fs::write(
            SYSDUMP_FILENAME,
            format!(
                "Fatal Error! Caused by: {:#010x?}\n\n{:#x?}",
                fatal_error, system
            ),
        )?;

        if let Some(mut debugger) = debugger {
            info!("Resuming the debugging session in \"post-mortem\" mode.");
            warn!("Step/Continue will not work.");
            system.freeze();
            match debugger.run(&mut system) {
                Ok(_) => info!("Disconnected from post-mortem GDB session."),
                Err(e) => return Err(e.into()),
            }
        } else {
            return Err("Fatal Error!".into());
        }
    }

    Ok(())
}
