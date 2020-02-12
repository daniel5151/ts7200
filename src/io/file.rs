use std::fs;
use std::io::{self, Read, Write};
use std::path::Path;
use std::sync::mpsc::{Receiver, Sender};
use std::thread;

pub fn spawn_reader_thread(path: impl AsRef<Path>, tx: Sender<u8>) -> io::Result<()> {
    let file = fs::File::open(path)?;
    thread::spawn(move || {
        for b in file.bytes() {
            let b = b.expect("io error");
            tx.send(b).unwrap();
        }
    });
    Ok(())
}

pub fn spawn_writer_thread(path: impl AsRef<Path>, rx: Receiver<u8>) -> io::Result<()> {
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    thread::spawn(move || {
        for b in rx.iter() {
            file.write_all(&[b]).expect("io error");
        }
    });
    Ok(())
}
