//! Adapted from https://stackoverflow.com/a/55201400

use std::ffi::OsString;
use std::fs;
use std::io;
use std::io::Read;
use std::io::Write;
use std::sync::mpsc;
use std::sync::mpsc::Receiver;
use std::sync::mpsc::TryRecvError;
use std::thread;

use super::NonBlockingByteIO;

enum ReadSource {
    Stdin,
    File(OsString),
}

fn spawn_in_channel(source: ReadSource) -> Receiver<u8> {
    let (tx, rx) = mpsc::channel::<u8>();
    thread::spawn(move || {
        let maybe_raw_terminal = match source {
            ReadSource::Stdin => {
                use termion::raw::IntoRawMode;
                Some(
                    io::stdout()
                        .into_raw_mode()
                        .expect("failed to enter raw mode"),
                )
            }
            ReadSource::File(_) => None,
        };

        match &source {
            ReadSource::Stdin => {
                for b in io::stdin().bytes() {
                    let b = b.unwrap();
                    if b == 3 {
                        maybe_raw_terminal.unwrap().suspend_raw_mode().unwrap();
                        eprintln!("Ctrl-C sent!");
                        std::process::exit(1);
                    }
                    tx.send(b).unwrap();
                }
            }
            ReadSource::File(path) => {
                // fast path for files that skips special key handling
                for b in fs::File::open(path)
                    .expect("failed to open file for reading")
                    .bytes()
                {
                    let b = b.unwrap();
                    tx.send(b).unwrap();
                }
            }
        }
    });
    rx
}

/// Read input from the file/stdin without blocking the main thread.
pub struct NonBlockingFileIO {
    next: Option<u8>,
    rx: mpsc::Receiver<u8>,
    write: Box<dyn io::Write>,
}

impl NonBlockingFileIO {
    pub fn new(in_path: OsString, out_path: OsString) -> Self {
        let write = Box::new(
            fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(out_path)
                .expect("failed to open file for writing"),
        );
        NonBlockingFileIO {
            next: None,
            rx: spawn_in_channel(ReadSource::File(in_path)),
            write,
        }
    }

    pub fn new_stdio() -> Self {
        use std::os::unix::io::FromRawFd;
        let stdout = unsafe { fs::File::from_raw_fd(1) };
        let write: Box<dyn Write> = Box::new(stdout);
        NonBlockingFileIO {
            next: None,
            rx: spawn_in_channel(ReadSource::Stdin),
            write,
        }
    }
}

impl NonBlockingByteIO for NonBlockingFileIO {
    fn can_read(&mut self) -> bool {
        match self.next {
            Some(_) => true,
            None => match self.rx.try_recv() {
                Ok(c) => {
                    self.next = Some(c);
                    true
                }
                Err(TryRecvError::Empty) => false,
                Err(TryRecvError::Disconnected) => panic!("Channel disconnected"),
            },
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
        self.write.write_all(&[val]).expect("io error");
    }
}
