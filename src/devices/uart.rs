use std::collections::VecDeque;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;

use crate::memory::{MemResult, MemResultExt, Memory};

/// UART module
///
/// As described in section 14 of the EP93xx User's Guide

#[derive(Debug, Default)]
struct Status {
    rx_buf: VecDeque<u8>,
    tx_buf_size: usize,
}

impl Status {
    fn new() -> Self {
        Default::default()
    }
}

fn spawn_reader_thread(status: Arc<Mutex<Status>>) -> Sender<u8> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        for b in rx.iter() {
            status.lock().unwrap().rx_buf.push_back(b);
        }
    });
    tx
}

fn spawn_writer_thread(status: Arc<Mutex<Status>>) -> (Receiver<u8>, Sender<u8>) {
    let (outer_tx, outer_rx) = mpsc::channel();
    let (inner_tx, inner_rx) = mpsc::channel();
    thread::spawn(move || {
        for b in inner_rx.iter() {
            status.lock().unwrap().tx_buf_size -= 1;
            outer_tx.send(b).unwrap();
        }
    });
    (outer_rx, inner_tx)
}

pub struct Uart {
    label: &'static str,

    status: Arc<Mutex<Status>>,

    input: Sender<u8>,
    output: Option<Receiver<u8>>,

    sender_tx: Sender<u8>,
}

impl std::fmt::Debug for Uart {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Uart").finish()
    }
}

impl Uart {
    /// Create a new uart
    pub fn new_hle(label: &'static str) -> Uart {
        let status = Arc::new(Mutex::new(Status::new()));

        let input = spawn_reader_thread(status.clone());
        let (output, sender_tx) = spawn_writer_thread(status.clone());

        Uart {
            label,
            status,
            input,
            output: Some(output),
            sender_tx,
        }
    }

    pub fn get_input(&self) -> Sender<u8> {
        self.input.clone()
    }

    pub fn get_output(&mut self) -> Receiver<u8> {
        self.output.take().expect("Output already gotten")
    }
}

impl Memory for Uart {
    fn label(&self) -> Option<&str> {
        Some(self.label)
    }

    fn device(&self) -> &'static str {
        "UART"
    }

    fn r32(&mut self, offset: u32) -> MemResult<u32> {
        match offset {
            // data (8-bit)
            0x00 => {
                // XXX: properly implement UART DATA read (i.e: respect flags)
                let mut status = self.status.lock().unwrap();
                Ok(status.rx_buf.pop_front().unwrap_or(0) as u32)
            }
            // read status
            0x04 => crate::mem_unimpl!("RSR_REG"),
            // line control high
            0x08 => crate::mem_stub!("LCRH_REG", 0),
            // line control mid
            0x0C => crate::mem_unimpl!("LCRM_REG"),
            // line control low
            0x10 => crate::mem_unimpl!("LCRL_REG"),
            // control
            0x14 => crate::mem_stub!("CTLR_REG", 0),
            // flag
            0x18 => {
                // XXX: properly implement UART DATA read (i.e: respect flags)
                let status = self.status.lock().unwrap();
                if status.rx_buf.is_empty() {
                    // 0x10 => Receive fifo empty
                    Ok(0x10)
                } else {
                    // 0x40 => something to receive
                    Ok(0x40)
                }
            }
            // interrupt identification and clear register
            0x1C => crate::mem_unimpl!("INTR_REG"),
            // dma control
            0x28 => crate::mem_unimpl!("DMAR_REG"),
            _ => crate::mem_unexpected!(),
        }
        .mem_ctx(offset, self)
    }

    fn w32(&mut self, offset: u32, val: u32) -> MemResult<()> {
        match offset {
            // data (8-bit)
            0x00 => {
                self.sender_tx.send(val as u8).unwrap();
                self.status.lock().unwrap().tx_buf_size += 1;
                Ok(())
            }
            // read status
            0x04 => crate::mem_unimpl!("RSR_REG"),
            // line control high
            0x08 => crate::mem_stub!("LCRH_REG"),
            // line control mid
            0x0C => crate::mem_stub!("LCRM_REG"),
            // line control low
            0x10 => crate::mem_stub!("LCRL_REG"),
            // control
            0x14 => crate::mem_stub!("CTLR_REG"),
            // flag
            0x18 => crate::mem_unimpl!("FLAG_REG"),
            // interrupt identification and clear register
            0x1C => crate::mem_unimpl!("INTR_REG"),
            // dma control
            0x28 => crate::mem_unimpl!("DMAR_REG"),
            _ => crate::mem_unexpected!(),
        }
        .mem_ctx(offset, self)
    }
}
