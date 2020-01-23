//! Adapted from https://stackoverflow.com/a/55201400

use std::io::{self, Read, Write};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::thread::{self, JoinHandle};

use termion::raw::IntoRawMode;

use super::NonBlockingByteIO;

#[derive(Clone, Copy)]
enum WriterMsg {
    Data(u8),
    CtrlCExit,
    Exit,
}

fn spawn_reader_thread(stdout_tx: Sender<WriterMsg>) -> (JoinHandle<()>, Receiver<u8>) {
    let (tx, rx) = mpsc::channel::<u8>();
    let handle = thread::spawn(move || {
        for b in io::stdin().bytes() {
            let b = b.unwrap();
            if b == 3 {
                // ctrl-c
                eprintln!("Recieved Ctrl-c - terminating now...");
                stdout_tx.send(WriterMsg::CtrlCExit).unwrap();
            }
            // Key code remapping to match gtkterm.
            let b = match b {
                127 => 8,
                _ => b,
            };
            tx.send(b).unwrap();
        }
    });
    (handle, rx)
}

fn spawn_writer_thread() -> (JoinHandle<()>, Sender<WriterMsg>) {
    let (tx, rx) = mpsc::channel::<WriterMsg>();
    let (ready_tx, ready_rx) = mpsc::channel::<()>();
    let handle = thread::spawn(move || {
        let mut stdout = io::stdout();

        ready_tx.send(()).unwrap();

        if termion::is_tty(&stdout) {
            let mut stdout = stdout
                .into_raw_mode()
                .expect("could not enter raw mode");

            for b in rx {
                match b {
                    WriterMsg::Data(b) => {
                        stdout.write_all(&[b]).expect("io error");
                        stdout.flush().expect("io error");
                    }
                    WriterMsg::CtrlCExit => {
                        stdout.suspend_raw_mode().unwrap();
                        std::process::exit(1);
                    }
                    WriterMsg::Exit => {
                        stdout.suspend_raw_mode().unwrap();
                        return;
                    }
                }
            }
        } else {
            for b in rx {
                match b {
                    WriterMsg::Data(b) => {
                        stdout.write_all(&[b]).expect("io error");
                        stdout.flush().expect("io error");
                    }
                    WriterMsg::CtrlCExit => {
                        std::process::exit(1);
                    }
                    WriterMsg::Exit => {
                        return;
                    }
                }
            }
        }
    });
    ready_rx.recv().unwrap();
    (handle, tx)
}

/// Read input from the file/stdin without blocking the main thread.
pub struct NonBlockingStdio {
    next: Option<u8>,
    stdin_rx: mpsc::Receiver<u8>,
    stdout_tx: mpsc::Sender<WriterMsg>,
    writer_thread: Option<JoinHandle<()>>,
}

impl Drop for NonBlockingStdio {
    fn drop(&mut self) {
        self.stdout_tx.send(WriterMsg::Exit).unwrap();
        self.writer_thread.take().unwrap().join().unwrap();
    }
}

impl Default for NonBlockingStdio {
    fn default() -> Self {
        Self::new()
    }
}

impl NonBlockingStdio {
    /// Return a new NonBlockingStdio instance backed by stdio
    /// (set to raw mode)
    pub fn new() -> Self {
        // the writer thread MUST be spawned first, as it sets the raw term mode
        let (writer_handle, stdout_tx) = spawn_writer_thread();
        let (_, stdin_rx) = spawn_reader_thread(stdout_tx.clone());

        NonBlockingStdio {
            next: None,
            stdin_rx,
            stdout_tx,
            writer_thread: Some(writer_handle),
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
        self.stdout_tx.send(WriterMsg::Data(val)).unwrap();
    }
}
