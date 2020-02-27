use std::io::{self, Read, Write};
use std::thread::{self, JoinHandle};

use crossbeam_channel::{self as chan, select};
use termion::raw::IntoRawMode;

struct CtrlC;

fn spawn_reader_thread(tx: chan::Sender<u8>, ctrl_c_exit: chan::Sender<CtrlC>) -> JoinHandle<()> {
    let thread = move || {
        for b in io::stdin().bytes() {
            let b = b.unwrap();
            if b == 3 {
                // ctrl-c
                eprintln!("Recieved Ctrl-c - terminating now...");
                ctrl_c_exit.send(CtrlC).unwrap();
            }
            // Key code remapping to match gtkterm.
            let b = match b {
                127 => 8,
                _ => b,
            };

            match tx.send(b) {
                Ok(()) => {}
                Err(chan::SendError(_)) => return,
            }
        }
    };

    thread::Builder::new()
        .name("stdio reader".to_string())
        .spawn(thread)
        .expect("failed to spawn thread")
}

fn spawn_writer_thread(rx: chan::Receiver<u8>) -> (JoinHandle<()>, chan::Sender<CtrlC>) {
    let (ctrl_c_exit_tx, ctrl_c_exit_rx) = chan::bounded::<CtrlC>(1);
    let (ready_tx, ready_rx) = chan::unbounded::<()>();

    let thread = move || {
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
                        Err(chan::RecvError) => break,
                    }

                }
                recv(ctrl_c_exit_rx) -> exit => {
                    match exit {
                        Ok(CtrlC) => {
                            if let Some(handle) = raw_mode_handle {
                                handle.suspend_raw_mode().unwrap();
                            }
                            std::process::exit(1);
                        }
                        Err(chan::RecvError) => {
                            // The reader thread has shut down without sending
                            // a CtrlC message. This typically happens when
                            // piping data in via stdout, and the sender process
                            // shuts down.
                            // The writer thread should stay alive though, and
                            // keep processing outgoing data.
                            //
                            // NOTE: we clone this logic here as select! will
                            // continously hammer this branch, even though the
                            // channel has already closed
                            for b in rx {
                                stdout.write_all(&[b]).expect("io error");
                                stdout.flush().expect("io error");
                            }
                            break;
                        }
                    }

                }
            }
        }
        if let Some(handle) = raw_mode_handle {
            handle.suspend_raw_mode().unwrap();
        }
    };

    let handle = thread::Builder::new()
        .name("stdio writer".to_string())
        .spawn(thread)
        .expect("failed to spawn thread");

    ready_rx.recv().unwrap();

    (handle, ctrl_c_exit_tx)
}

/// Put Stdin into raw mode, and connect Stdin/Stdout to the tx/rx channels.
pub fn stdio_to_chans(
    tx: chan::Sender<u8>,
    rx: chan::Receiver<u8>,
) -> (JoinHandle<()>, JoinHandle<()>) {
    // the writer thread MUST be spawned first, as it sets the raw term mode
    let (writer_handle, ctrl_c_exit) = spawn_writer_thread(rx);
    let reader_handle = spawn_reader_thread(tx, ctrl_c_exit);
    (reader_handle, writer_handle)
}
