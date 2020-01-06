//! Adapted from https://stackoverflow.com/a/55201400

use std::collections::VecDeque;
use std::ffi::OsString;
use std::fs;
use std::io;
use std::io::BufRead;
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
        let reader: Box<dyn Read> = match source {
            ReadSource::Stdin => Box::new(io::stdin()),
            ReadSource::File(path) => {
                Box::new(fs::File::open(path).expect("failed to open file for reading"))
            }
        };
        let mut bufreader = io::BufReader::new(reader);
        loop {
            let mut buffer = String::new();
            bufreader.read_line(&mut buffer).unwrap();
            for &b in buffer.as_bytes() {
                tx.send(b).unwrap()
            }
        }
    });
    rx
}

/// Read input from the file/stdin without blocking the main thread.
// TODO: Implement the stdio version separately using ncurses?
pub struct NonBlockingFileIO {
    buf: VecDeque<u8>,
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
            buf: VecDeque::new(),
            rx: spawn_in_channel(ReadSource::File(in_path)),
            write: write,
        }
    }

    pub fn new_stdio() -> Self {
        let write = Box::new(io::stdout());
        NonBlockingFileIO {
            buf: VecDeque::new(),
            rx: spawn_in_channel(ReadSource::Stdin),
            write: write,
        }
    }
}

impl NonBlockingByteIO for NonBlockingFileIO {
    fn can_read(&mut self) -> bool {
        if !self.buf.is_empty() {
            return true;
        } else {
            match self.rx.try_recv() {
                Ok(c) => {
                    self.buf.push_back(c);
                    true
                }
                Err(TryRecvError::Empty) => false,
                Err(TryRecvError::Disconnected) => panic!("Channel disconnected"),
            }
        }
    }

    fn read(&mut self) -> u8 {
        // call `can_read` first, just to fill the buffer if there is data available
        self.can_read();
        match self.buf.pop_front() {
            Some(c) => c,
            None => 0, // arbitrary value
        }
    }

    fn write(&mut self, val: u8) {
        self.write.write(&[val]).expect("io error");
    }
}
