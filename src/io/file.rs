use std::fs;
use std::io::{Read, Write};
use std::path::Path;
use std::thread::{self, JoinHandle};

use crossbeam_channel as mpsc;

/// Spawns a thread that reads bytes received from a file at `path` to `tx`
pub fn spawn_reader_thread(
    path: impl AsRef<Path> + std::fmt::Debug,
    tx: mpsc::Sender<u8>,
) -> Result<JoinHandle<()>, std::io::Error> {
    let name = format!("File Reader | {:?}", path);

    let file = fs::File::open(path)?;
    let thread = move || {
        for b in file.bytes() {
            let b = b.expect("io error");
            tx.send(b).unwrap();
        }
    };

    let handle = thread::Builder::new().name(name).spawn(thread).unwrap();
    Ok(handle)
}

/// Spawns a thread that writes bytes received on `rx` to a file at `path`
pub fn spawn_writer_thread(
    path: impl AsRef<Path> + std::fmt::Debug,
    rx: mpsc::Receiver<u8>,
) -> Result<JoinHandle<()>, std::io::Error> {
    let name = format!("File Writer | {:?}", path);

    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    let thread = move || {
        for b in rx.iter() {
            file.write_all(&[b]).expect("io error");
        }
    };

    let handle = thread::Builder::new().name(name).spawn(thread).unwrap();
    Ok(handle)
}
