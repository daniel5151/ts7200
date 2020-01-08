//! Adapted from https://stackoverflow.com/a/55201400

use std::fs;
use std::io::{self, Read, Write};
use std::path::Path;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread;

use termion::raw::IntoRawMode;

use super::NonBlockingByteIO;

enum ReadSource {
    Stdin,
    File(fs::File),
}

fn spawn_in_channel(source: ReadSource) -> Receiver<u8> {
    let (tx, rx) = mpsc::channel::<u8>();
    thread::spawn(move || {
        match &source {
            // Snoop on the input stream to catch "special" key sequences.
            ReadSource::Stdin => {
                for b in io::stdin().bytes() {
                    let b = b.unwrap();
                    if b == 3 {
                        // TODO: make ctrl-c handing more graceful
                        eprintln!("Recieved Ctrl-C Signal - terminating now");
                        std::process::exit(1);
                    }
                    tx.send(b).unwrap();
                }
            }
            // No stream-snooping for files; just read bytes as they come.
            ReadSource::File(file) => {
                for b in file.bytes() {
                    let b = b.unwrap();
                    tx.send(b).unwrap();
                }
            }
        }
    });
    rx
}

/// Read input from the file/stdin without blocking the main thread.
pub struct NonBlockingFileIO<T: Write> {
    next: Option<u8>,
    stdin_rx: mpsc::Receiver<u8>,
    write: T,
}

impl NonBlockingFileIO<fs::File> {
    /// Return a new NonBlockingFileIO instance reading data from `in_path`, and
    /// pushing output to `out_path`. Returns an error if either of the files
    /// cannot be opened/created.
    pub fn new(in_path: impl AsRef<Path>, out_path: impl AsRef<Path>) -> io::Result<Self> {
        let write = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(out_path)?;

        Ok(NonBlockingFileIO {
            next: None,
            stdin_rx: spawn_in_channel(ReadSource::File(fs::File::open(in_path)?)),
            write,
        })
    }
}

impl NonBlockingFileIO<termion::raw::RawTerminal<io::Stdout>> {
    /// Return a new NonBlockingFileIO instance backed by stdio (set to raw
    /// mode)
    pub fn new_stdio() -> io::Result<Self> {
        let write = io::stdout().into_raw_mode()?;

        Ok(NonBlockingFileIO {
            next: None,
            stdin_rx: spawn_in_channel(ReadSource::Stdin),
            write,
        })
    }
}

impl<T: Write> NonBlockingByteIO for NonBlockingFileIO<T> {
    fn can_read(&mut self) -> bool {
        match self.next {
            Some(_) => true,
            None => match self.stdin_rx.try_recv() {
                Ok(c) => {
                    self.next = Some(c);
                    true
                }
                Err(TryRecvError::Empty) => false,
                // TODO: make ctrl-c handing more graceful
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
        self.write.flush().expect("io error");
    }
}
