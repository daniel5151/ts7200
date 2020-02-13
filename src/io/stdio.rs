use std::io::{self, Read, Write};
use std::thread::{self, JoinHandle};

use crossbeam_channel::{self as mpsc, select};
use termion::raw::IntoRawMode;

struct CtrlC;

fn spawn_reader_thread(tx: mpsc::Sender<u8>, writer_exit: mpsc::Sender<CtrlC>) -> JoinHandle<()> {
    thread::spawn(move || {
        for b in io::stdin().bytes() {
            let b = b.unwrap();
            if b == 3 {
                // ctrl-c
                eprintln!("Recieved Ctrl-c - terminating now...");
                writer_exit.send(CtrlC).unwrap();
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
    })
}

fn spawn_writer_thread(rx: mpsc::Receiver<u8>) -> (JoinHandle<()>, mpsc::Sender<CtrlC>) {
    let (exit_tx, exit_rx) = mpsc::bounded::<CtrlC>(1);
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
                        Err(mpsc::RecvError) => break,
                    }

                }
                recv(exit_rx) -> _ => {
                    if let Some(handle) = raw_mode_handle {
                        handle.suspend_raw_mode().unwrap();
                    }
                    std::process::exit(1);
                }
            }
        }
        if let Some(handle) = raw_mode_handle {
            handle.suspend_raw_mode().unwrap();
        }
    });

    ready_rx.recv().unwrap();

    (handle, exit_tx)
}

/// Spawn stdio reader and writer threads that puts stdio in raw mode
pub fn spawn_threads(
    tx: mpsc::Sender<u8>,
    rx: mpsc::Receiver<u8>,
) -> (JoinHandle<()>, JoinHandle<()>) {
    // the writer thread MUST be spawned first, as it sets the raw term mode
    let (writer_handle, writer_exit) = spawn_writer_thread(rx);
    let reader_handle = spawn_reader_thread(tx, writer_exit);
    (reader_handle, writer_handle)
}
