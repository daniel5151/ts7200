use std::io::{self, Read, Write};
use std::thread::{self, JoinHandle};

use crossbeam_channel::{self as mpsc, select};
use termion::raw::IntoRawMode;

enum ExitKind {
    Normal,
    CtrlC,
}

fn spawn_reader_thread(
    tx: mpsc::Sender<u8>,
    writer_exit: mpsc::Sender<ExitKind>,
) -> (JoinHandle<()>, mpsc::Sender<ExitKind>) {
    let (exit_tx, exit_rx) = mpsc::bounded(1);

    let handle = thread::spawn(move || {
        let (stdin_tx, stdin_rx) = mpsc::unbounded::<u8>();

        // create a detached thread to provide a select-able stdin stream
        thread::spawn(move || {
            for b in io::stdin().bytes() {
                let b = b.unwrap();
                match stdin_tx.send(b) {
                    Ok(()) => {}
                    Err(mpsc::SendError(_)) => return,
                }
            }
        });

        loop {
            select! {
                recv(exit_rx) -> _ => break,
                recv(stdin_rx) -> b => {
                    let b = b.unwrap(); // stdin thread should never fail
                    if b == 3 {
                        // ctrl-c
                        eprintln!("Recieved Ctrl-c - terminating now...");
                        writer_exit.send(ExitKind::CtrlC).unwrap();
                        break;
                    }
                    // Key code remapping to match gtkterm.
                    let b = match b {
                        127 => 8,
                        _ => b,
                    };

                    match tx.send(b) {
                        Ok(()) => {}
                        Err(mpsc::SendError(_)) => break,
                    }
                }
            }
        }
    });

    (handle, exit_tx)
}

fn spawn_writer_thread(rx: mpsc::Receiver<u8>) -> (JoinHandle<()>, mpsc::Sender<ExitKind>) {
    let (exit_tx, exit_rx) = mpsc::bounded::<ExitKind>(1);
    let (ready_tx, ready_rx) = mpsc::bounded(0);

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
                recv(exit_rx) -> kind => {
                    let kind = kind.unwrap();
                    match kind {
                        ExitKind::Normal => break,
                        ExitKind::CtrlC => {
                            // leave raw mode!
                            if let Some(handle) = raw_mode_handle {
                                handle.suspend_raw_mode().unwrap();
                            }
                            std::process::exit(1);
                        }
                    }
                },
                recv(rx) -> b => {
                    match b {
                        Ok(b) => {
                            stdout.write_all(&[b]).expect("io error");
                            stdout.flush().expect("io error");
                        }
                        Err(mpsc::RecvError) => break,
                    }

                }
            }
        }

        // leave raw mode!
        if let Some(handle) = raw_mode_handle {
            handle.suspend_raw_mode().unwrap();
        }
    });

    ready_rx.recv().unwrap();

    (handle, exit_tx)
}

pub struct Stdio {
    reader_thread: Option<JoinHandle<()>>,
    writer_thread: Option<JoinHandle<()>>,
    reader_exit: mpsc::Sender<ExitKind>,
    writer_exit: mpsc::Sender<ExitKind>,
}

impl Drop for Stdio {
    fn drop(&mut self) {
        self.reader_exit.send(ExitKind::Normal).unwrap();
        self.writer_exit.send(ExitKind::Normal).unwrap();
    }
}

impl Stdio {
    /// Spawn stdio reader and writer threads that puts stdio in raw mode
    pub fn new(tx: mpsc::Sender<u8>, rx: mpsc::Receiver<u8>) -> Stdio {
        // the writer thread MUST be spawned first, as it sets the raw term mode
        let (writer_thread, writer_exit) = spawn_writer_thread(rx);
        let (reader_thread, reader_exit) = spawn_reader_thread(tx, writer_exit.clone());
        Stdio {
            reader_thread: Some(reader_thread),
            writer_thread: Some(writer_thread),
            reader_exit,
            writer_exit,
        }
    }

    pub fn take_reader_thread(&mut self) -> Option<JoinHandle<()>> {
        self.reader_thread.take()
    }

    pub fn take_writer_thread(&mut self) -> Option<JoinHandle<()>> {
        self.writer_thread.take()
    }
}
