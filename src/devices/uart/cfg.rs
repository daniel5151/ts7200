//! Encapsulates `--uartX` config string parsing and validation

use std::error::Error as StdError;
use std::fmt::{self, Display};
use std::fs;
use std::net::TcpStream;
use std::str::FromStr;

use super::{iothreads, ReaderTask, Uart, WriterTask};

pub enum UartCfg {
    /// none
    None,
    /// file:/path/
    File {
        out_path: String,
        in_path: Option<String>,
    },
    /// stdio
    Stdio,
    /// tcp:[host]:port
    Tcp { host: String, port: u16 },
}

#[derive(Debug)]
pub enum UartCfgError {
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
    /// Apply uart configuration to the specified uart device
    pub fn apply(&self, uart: &mut Uart) -> Result<(), UartCfgError> {
        uart.install_io_tasks(|tx, rx| match self {
            UartCfg::None => Ok((None, None)),
            UartCfg::File { in_path, out_path } => {
                let in_writer = match in_path {
                    Some(in_path) => {
                        let in_file = fs::File::open(&in_path).map_err(UartCfgError::BadFile)?;
                        let in_thread = iothreads::reader_to_chan(in_path.to_string(), in_file, tx);
                        Some(ReaderTask::new(in_thread))
                    }
                    None => None,
                };

                let out_writer = {
                    let out_file = fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(&out_path)
                        .map_err(UartCfgError::BadFile)?;
                    let out_thread = iothreads::writer_to_chan(out_path.to_string(), out_file, rx);
                    Some(WriterTask::new(out_thread))
                };

                Ok((in_writer, out_writer))
            }
            UartCfg::Stdio => {
                let (in_thread, out_thread) = iothreads::stdio_to_chans(tx, rx);
                Ok((
                    Some(ReaderTask::new(in_thread)),
                    Some(WriterTask::new(out_thread)),
                ))
            }
            UartCfg::Tcp { host, port } => {
                let addr = format!("{}:{}", host, port);
                let in_stream = TcpStream::connect(&addr).map_err(UartCfgError::BadTcp)?;
                let out_stream = in_stream.try_clone().expect("could not clone TcpStream");

                let in_thread = iothreads::reader_to_chan(addr.clone(), in_stream, tx);
                let out_thread = iothreads::writer_to_chan(addr, out_stream, rx);
                Ok((
                    Some(ReaderTask::new(in_thread)),
                    Some(WriterTask::new(out_thread)),
                ))
            }
        })
        .map(drop)
    }
}

impl FromStr for UartCfg {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<UartCfg, &'static str> {
        let mut s = s.split(':');
        let kind = s.next().unwrap();
        Ok(match kind {
            "none" => UartCfg::None,
            "file" => {
                let mut s = s.next().ok_or("no output path specified")?.split(',');

                let out_path = s.next().unwrap().to_string();
                let in_path = match s.next() {
                    Some(s) => {
                        let mut s = s.split('=');
                        if s.next().unwrap() != "in" {
                            return Err("expected to find `in=/path/to/file`");
                        }
                        let in_path = s.next().ok_or("invalid input path")?.to_string();
                        Some(in_path)
                    }
                    None => None,
                };
                UartCfg::File { in_path, out_path }
            }
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
