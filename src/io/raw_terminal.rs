use std::io;
use std::io::Read;
use std::io::Write;

use termion::raw::IntoRawMode;
use termion::raw::RawTerminal as TermionRawTerminal;
use termion::{async_stdin, is_tty, AsyncReader};

use super::NonBlockingByteIO;

pub struct RawTerminal {
    stdin: AsyncReader,
    stdout: TermionRawTerminal<Box<dyn Write>>,
    next: Option<u8>,
}

impl RawTerminal {
    pub fn new() -> Self {
        if !(is_tty(&io::stdin()) && is_tty(&io::stdout())) {
            panic!("stdin/stdout must be tty's");
        }
        let stdin = async_stdin();
        let stdout: Box<dyn Write> = Box::new(io::stdout());
        let stdout = stdout.into_raw_mode().expect("failed to enter raw mode");
        RawTerminal {
            stdin,
            stdout,
            next: None,
        }
    }
}

impl NonBlockingByteIO for RawTerminal {
    fn can_read(&mut self) -> bool {
        match self.next {
            Some(_) => true,
            None => {
                let mut buf = [0];
                if self.stdin.read(&mut buf).unwrap() == 1 {
                    if (buf[0] == 3) {
                        self.stdout.suspend_raw_mode().unwrap();
                        panic!("Ctrl-C sent!");
                    }
                    self.next = Some(buf[0]);
                    true
                } else {
                    false
                }
            }
        }
    }

    fn read(&mut self) -> u8 {
        // call `can_read` first, just to fill the buffer if there is data available
        self.can_read();
        match self.next {
            Some(c) => {
                self.next = None;
                c
            }
            None => 0, // arbitrary value
        }
    }

    fn write(&mut self, val: u8) {
        self.stdout.write_all(&[val]).expect("io error");
    }
}
