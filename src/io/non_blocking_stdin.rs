//! Adapted from https://stackoverflow.com/a/55201400

use std::collections::VecDeque;
use std::io;
use std::io::Write;
use std::sync::mpsc;
use std::sync::mpsc::Receiver;
use std::sync::mpsc::TryRecvError;
use std::thread;

use super::NonBlockingByteIO;

fn spawn_stdin_channel() -> Receiver<u8> {
    let (tx, rx) = mpsc::channel::<u8>();
    thread::spawn(move || loop {
        let mut buffer = String::new();
        io::stdin().read_line(&mut buffer).unwrap();
        for &b in buffer.as_bytes() {
            tx.send(b).unwrap()
        }
    });
    rx
}

/// Read input from stdin without blocking the main thread.
// TODO: reimplement this using ncurses?
pub struct NonBlockingStdin {
    buf: VecDeque<u8>,
    rx: mpsc::Receiver<u8>,
}

impl NonBlockingStdin {
    pub fn new() -> NonBlockingStdin {
        NonBlockingStdin {
            buf: VecDeque::new(),
            rx: spawn_stdin_channel(),
        }
    }
}

impl NonBlockingByteIO for NonBlockingStdin {
    fn can_read(&mut self) -> bool {
        match self.rx.try_recv() {
            Ok(c) => {
                self.buf.push_back(c);
                true
            }
            Err(TryRecvError::Empty) => false,
            Err(TryRecvError::Disconnected) => panic!("Channel disconnected"),
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
        io::stdout().lock().write(&[val]).expect("io error");
    }
}
