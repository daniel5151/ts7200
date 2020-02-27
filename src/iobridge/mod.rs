use std::io::{Read, Write};
use std::thread::{self, JoinHandle};

use crossbeam_channel as chan;

mod stdio;
pub use stdio::stdio_to_chans;

/// Spawn a thread that continuously writes data from `reader` to `tx`.
pub fn reader_to_chan(
    thread_label: String,
    reader: impl Read + Send + 'static,
    tx: chan::Sender<u8>,
) -> JoinHandle<()> {
    let thread = move || {
        for b in reader.bytes() {
            let b = b.expect("io error");
            tx.send(b).unwrap();
        }
    };

    thread::Builder::new()
        .name(format!("{} - Reader", thread_label))
        .spawn(thread)
        .expect("failed to spawn thread")
}

/// Spawn a thread that continuously writes data from `writer` to `rx`.
pub fn writer_to_chan(
    thread_label: String,
    mut writer: impl Write + Send + 'static,
    rx: chan::Receiver<u8>,
) -> JoinHandle<()> {
    let thread = move || {
        for b in rx.iter() {
            writer.write_all(&[b]).expect("io error");
        }
    };

    thread::Builder::new()
        .name(format!("{} - Writer", thread_label))
        .spawn(thread)
        .expect("failed to spawn thread")
}
