use std::io::{self, Read, Write};
use std::sync::mpsc;
use std::thread::{self, JoinHandle};

use termion::raw::IntoRawMode;

#[derive(Clone, Copy)]
enum WriterMsg {
    Data(u8),
    CtrlCExit,
    Exit,
}

fn spawn_reader_thread(tx: mpsc::Sender<u8>, stdout_tx: mpsc::Sender<WriterMsg>) {
    thread::spawn(move || {
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
}

fn spawn_writer_thread() -> (JoinHandle<()>, mpsc::Sender<WriterMsg>) {
    let (tx, rx) = mpsc::channel::<WriterMsg>();
    let (ready_tx, ready_rx) = mpsc::channel::<()>();
    let handle = thread::spawn(move || {
        let mut stdout = io::stdout();

        let raw_mode_handle = if termion::is_tty(&stdout) {
            Some(
                io::stdout()
                    .into_raw_mode()
                    .expect("could not enter raw mode"),
            )
        } else {
            None
        };

        ready_tx.send(()).unwrap();

        for b in rx {
            match b {
                WriterMsg::Data(b) => {
                    stdout.write_all(&[b]).expect("io error");
                    stdout.flush().expect("io error");
                }
                WriterMsg::CtrlCExit => {
                    if let Some(handle) = raw_mode_handle {
                        handle.suspend_raw_mode().unwrap();
                    }
                    std::process::exit(1);
                }
                WriterMsg::Exit => {
                    if let Some(handle) = raw_mode_handle {
                        handle.suspend_raw_mode().unwrap();
                    }
                    return;
                }
            }
        }
    });
    ready_rx.recv().unwrap();
    (handle, tx)
}

fn spawn_output_transfer_thread(rx: mpsc::Receiver<u8>, stdout_tx: mpsc::Sender<WriterMsg>) {
    thread::spawn(move || {
        for b in rx.iter() {
            match stdout_tx.send(WriterMsg::Data(b)) {
                Ok(()) => {}
                Err(mpsc::SendError(_)) => {
                    // Don't unwrap the result of send, the other end will
                    // close when we're shutting down.
                    return;
                }
            }
        }
    });
}

/// Read input from the file/stdin without blocking the main thread.
pub struct Stdio {
    stdout_tx: mpsc::Sender<WriterMsg>,
    writer_thread: Option<JoinHandle<()>>,
}

impl Drop for Stdio {
    fn drop(&mut self) {
        self.stdout_tx.send(WriterMsg::Exit).unwrap();
        self.writer_thread.take().unwrap().join().unwrap();
    }
}

impl Stdio {
    /// Return a new NonBlockingStdio instance backed by stdio
    /// (set to raw mode)
    pub fn new(tx: mpsc::Sender<u8>, rx: mpsc::Receiver<u8>) -> Self {
        // the writer thread MUST be spawned first, as it sets the raw term mode
        let (writer_handle, stdout_tx) = spawn_writer_thread();
        spawn_output_transfer_thread(rx, stdout_tx.clone());
        spawn_reader_thread(tx, stdout_tx.clone());

        Stdio {
            stdout_tx,
            writer_thread: Some(writer_handle),
        }
    }
}
