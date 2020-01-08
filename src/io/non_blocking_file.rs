//! Adapted from https://stackoverflow.com/a/55201400

use std::fs;
use std::io::{self, Read, Write};
use std::path::Path;

use super::NonBlockingByteIO;

/// Read input from the file/stdin without blocking the main thread.
pub struct NonBlockingFile {
    next: Option<u8>,
    in_file: fs::File,
    out_file: fs::File,
}

impl NonBlockingFile {
    /// Return a new NonBlockingFile instance reading data from `in_path`, and
    /// pushing output to `out_path`. Returns an error if either of the files
    /// cannot be opened/created.
    pub fn new(
        in_path: impl AsRef<Path>,
        out_path: impl AsRef<Path>,
    ) -> io::Result<NonBlockingFile> {
        Ok(NonBlockingFile {
            next: None,
            in_file: fs::File::open(in_path)?,
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
            None => {
                let mut c = [0];
                match self.in_file.read(&mut c).expect("file io error") {
                    0 => false,
                    1 => {
                        self.next = Some(c[0]);
                        true
                    }
                    _ => unreachable!(),
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
        self.out_file.write_all(&[val]).expect("io error");
        self.out_file.flush().expect("io error");
    }
}
