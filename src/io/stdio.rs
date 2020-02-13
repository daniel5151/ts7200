use std::io::{self, Read, Write};
use std::thread::{self, JoinHandle};

use crossbeam_channel::{self as mpsc, select};
use termion::raw::IntoRawMode;

#[derive(Clone, Copy)]
enum WriterExit {
    CtrlCExit,
    Exit,
}

fn spawn_reader_thread(tx: mpsc::Sender<u8>, writer_exit: mpsc::Sender<WriterExit>) {
    thread::spawn(move || {
        for b in io::stdin().bytes() {
            let b = b.unwrap();
            if b == 3 {
                // ctrl-c
                eprintln!("Recieved Ctrl-c - terminating now...");
                writer_exit.send(WriterExit::CtrlCExit).unwrap();
            }
            // Key code remapping to match gtkterm.
            let b = match b {
                127 => 8,
                _ => b,
            };

            match tx.send(b) {
                Ok(()) => {}
                Err(mpsc::SendError(_)) => return,
            }
        }
    });
}

fn spawn_writer_thread(rx: mpsc::Receiver<u8>) -> (JoinHandle<()>, mpsc::Sender<WriterExit>) {
    let (exit_tx, exit_rx) = mpsc::unbounded::<WriterExit>();
    let (ready_tx, ready_rx) = mpsc::unbounded::<()>();

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

        loop {
            select! {
                recv(rx) -> b => {
                    match b {
                        Ok(b) => {
                            stdout.write_all(&[b]).expect("io error");
                            stdout.flush().expect("io error");
                        }
                        Err(mpsc::RecvError) => return
                    }

                }
                recv(exit_rx) -> kind => {
                    if let Some(handle) = raw_mode_handle {
                        handle.suspend_raw_mode().unwrap();
                    }
                    match kind {
                        Ok(WriterExit::CtrlCExit) => std::process::exit(1),
                        Ok(WriterExit::Exit) => return,
                        Err(mpsc::RecvError) => panic!("exit sender closed unexpectedly"),
                    }
                }
            }
        }
    });

    ready_rx.recv().unwrap();

    (handle, exit_tx)
}

/// Read input from the file/stdin without blocking the main thread.
pub struct Stdio {
    writer_exit: mpsc::Sender<WriterExit>,
    writer_thread: Option<JoinHandle<()>>,
}

impl Drop for Stdio {
    fn drop(&mut self) {
        self.writer_exit.send(WriterExit::Exit).unwrap();
        self.writer_thread.take().unwrap().join().unwrap();
    }
}

impl Stdio {
    /// Return a new NonBlockingStdio instance backed by stdio
    /// (set to raw mode)
    pub fn new(tx: mpsc::Sender<u8>, rx: mpsc::Receiver<u8>) -> Self {
        // the writer thread MUST be spawned first, as it sets the raw term mode
        let (writer_handle, writer_exit) = spawn_writer_thread(rx);
        spawn_reader_thread(tx, writer_exit.clone());

        Stdio {
            writer_exit,
            writer_thread: Some(writer_handle),
        }
    }
}
