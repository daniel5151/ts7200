use std::error::Error as StdError;
use std::fmt::{self, Display};
use std::fs;
use std::net::{TcpListener, TcpStream};
use std::str::FromStr;

use log::*;
use structopt::StructOpt;

pub mod devices;
pub mod iobridge;
pub mod memory;
pub mod ts7200;

use crate::devices::uart;
use crate::ts7200::Ts7200;

#[derive(StructOpt)]
#[structopt(name = "ts7200")]
#[structopt(about = r#"
An emulator for the TS-7200 Single Board Computer, as used in CS 452 at the
University of Waterloo.

UART CONFIGURATION:
    The `--uartX` flags except a configuration string. The string's format is
    closely modeled after QEMU's `-serial` flag:

    * none               | No device is connected
    * file:/path/to/file | Write output to the specified file
    * stdio              | Use the process's stdin / stdout
    * tcp:[host]:port    | Connect to a tcp server. "host" defaults to localhost

    e.g: `--uart1=file:./trainout`, `--uart1=tcp::3018`
"#)]
struct Args {
    /// kernel ELF file to load
    kernel_elf: String,

    /// spawn a gdb server listening on the specified port
    #[structopt(short, long)]
    gdbport: Option<u16>,

    /// UART1 configuration.
    #[structopt(long, default_value = "none")]
    uart1: UartCfg,

    /// UART2 configuration.
    #[structopt(long, default_value = "stdio")]
    uart2: UartCfg,
}

enum UartCfg {
    /// none
    None,
    /// file:/path/
    File(String),
    /// stdio
    Stdio,
    /// tcp:[host]:port
    Tcp { host: String, port: u16 },
}

#[derive(Debug)]
enum UartCfgError {
    BadFile(std::io::Error),
    BadTcp(std::io::Error),
}

impl Display for UartCfgError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        match self {
            UartCfgError::BadFile(e) => write!(f, "Could not open file: {}", e),
            UartCfgError::BadTcp(e) => write!(f, "Could not open tcp: {}", e),
        }
    }
}
impl StdError for UartCfgError {}

impl UartCfg {
    fn apply(&self, uart: &mut uart::Uart) -> Result<(), UartCfgError> {
        uart.install_io_tasks(|tx, rx| match self {
            UartCfg::None => Ok((None, None)),
            UartCfg::File(path) => {
                let out_file = fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&path)
                    .map_err(UartCfgError::BadFile)?;

                let out_thread = iobridge::writer_to_chan(path.to_string(), out_file, rx);
                Ok((None, Some(uart::WriterTask::new(out_thread))))
            }
            UartCfg::Stdio => {
                let (in_thread, out_thread) = iobridge::stdio_to_chans(tx, rx);
                Ok((
                    Some(uart::ReaderTask::new(in_thread)),
                    Some(uart::WriterTask::new(out_thread)),
                ))
            }
            UartCfg::Tcp { host, port } => {
                let addr = format!("{}:{}", host, port);
                let in_stream = TcpStream::connect(&addr).map_err(UartCfgError::BadTcp)?;
                let out_stream = in_stream.try_clone().expect("could not clone TcpStream");

                let in_thread = iobridge::reader_to_chan(addr.clone(), in_stream, tx);
                let out_thread = iobridge::writer_to_chan(addr.clone(), out_stream, rx);
                Ok((
                    Some(uart::ReaderTask::new(in_thread)),
                    Some(uart::WriterTask::new(out_thread)),
                ))
            }
        })
        .map(drop)
    }
}

impl FromStr for UartCfg {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<UartCfg, &'static str> {
        let mut s = s.split(":");
        let kind = s.next().unwrap();
        Ok(match kind {
            "none" => UartCfg::None,
            "file" => UartCfg::File(s.next().ok_or("no file path specified")?.to_string()),
            "stdio" => UartCfg::Stdio,
            "tcp" => {
                let host = match s.next().ok_or("no host specified")? {
                    "" => "127.0.0.1",
                    other => other,
                };
                let port = s
                    .next()
                    .ok_or("no port specified")?
                    .parse()
                    .map_err(|_| "invalid port")?;

                UartCfg::Tcp {
                    host: host.to_string(),
                    port,
                }
            }
            _ => return Err("invalid io type"),
        })
    }
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
    pretty_env_logger::init();
    let args = Args::from_args();

    // create the base system
    let file = fs::File::open(args.kernel_elf)?;
    let mut system = Ts7200::new_hle(file)?;

    // hook up the uarts
    args.uart1.apply(&mut system.devices_mut().uart1)?;
    args.uart2.apply(&mut system.devices_mut().uart2)?;

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
