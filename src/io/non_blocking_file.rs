//! Adapted from https://stackoverflow.com/a/55201400

use std::fs;
use std::io::{self, Read, Write};
use std::path::Path;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread;

use super::NonBlockingByteIO;

/// Read input from the file/stdin without blocking the main thread.
pub struct NonBlockingFile {
    next: Option<u8>,
    in_rx: mpsc::Receiver<u8>,
    out_file: fs::File,
}

fn spawn_reader_thread(in_file: fs::File) -> Receiver<u8> {
    let (tx, rx) = mpsc::channel::<u8>();
    thread::spawn(move || {
        for b in in_file.bytes() {
            let b = b.unwrap();
            tx.send(b).unwrap();
        }
    });
    rx
}

impl NonBlockingFile {
    /// Return a new NonBlockingFile instance reading data from `in_path`, and
    /// pushing output to `out_path`. Returns an error if either of the files
    /// cannot be opened/created.
    pub fn new(
        in_path: impl AsRef<Path>,
        out_path: impl AsRef<Path>,
    ) -> io::Result<NonBlockingFile> {
        let in_file = fs::File::open(in_path)?;
        Ok(NonBlockingFile {
            next: None,
            in_rx: spawn_reader_thread(in_file),
            out_file: fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(out_path)?,
        })
    }
}

impl NonBlockingByteIO for NonBlockingFile {
    fn can_read(&mut self) -> bool {
        match self.next {
            Some(_) => true,
            None => match self.in_rx.try_recv() {
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
        self.out_file.write_all(&[val]).expect("io error");
    }
}
