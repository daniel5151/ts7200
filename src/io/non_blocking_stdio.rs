//! Adapted from https://stackoverflow.com/a/55201400

use std::io::{self, Read, Write};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::thread;

use termion::raw::IntoRawMode;

use super::NonBlockingByteIO;

fn spawn_reader_thread() -> Receiver<u8> {
    let (tx, rx) = mpsc::channel::<u8>();
    thread::spawn(move || {
        for b in io::stdin().bytes() {
            let b = b.unwrap();
            if b == 3 {
                // TODO: make ctrl-c handing more graceful
                eprintln!("Recieved Ctrl-C Signal - terminating now");
                std::process::exit(1);
            }
            tx.send(b).unwrap();
        }
    });
    rx
}

fn spawn_writer_thread() -> Sender<u8> {
    let (tx, rx) = mpsc::channel::<u8>();
    let (ready_tx, ready_rx) = mpsc::channel::<()>();
    thread::spawn(move || {
        println!("bruh");

        let mut stdout = io::stdout()
            .into_raw_mode()
            .expect("could not enter raw mode");

        ready_tx.send(()).unwrap();

        for b in rx {
            stdout.write_all(&[b]).expect("io error");
            stdout.flush().expect("io error");
        }
    });
    ready_rx.recv().unwrap();
    tx
}

/// Read input from the file/stdin without blocking the main thread.
pub struct NonBlockingStdio {
    next: Option<u8>,
    stdin_rx: mpsc::Receiver<u8>,
    stdout_tx: mpsc::Sender<u8>,
}

impl NonBlockingStdio {
    /// Return a new NonBlockingStdio instance backed by stdio
    /// (set to raw mode)
    pub fn new() -> Self {
        // the writer thread MUST be spawned first, as it sets the raw term mode
        let stdout_tx = spawn_writer_thread();
        let stdin_rx = spawn_reader_thread();

        NonBlockingStdio {
            next: None,
            stdin_rx,
            stdout_tx,
        }
    }
}

impl NonBlockingByteIO for NonBlockingStdio {
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
        self.stdout_tx.send(val).unwrap();
    }
}
