use std::collections::VecDeque;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;
use std::time::Duration;

use log::*;

use crate::devices::vic::Interrupt;
use crate::memory::{MemResult, MemResultExt, Memory};

/// UART module
///
/// As described in section 14 of the EP93xx User's Guide

static UARTCLK_HZ: u64 = 7_372_800;

#[derive(Debug, Copy, Clone)]
enum UartInt {
    Tx = 0,
    Rx = 1,
    Combined = 2,
}

impl UartInt {
    fn hw_int(self, index: u8) -> Interrupt {
        use Interrupt::*;
        use UartInt::*;
        match (self, index) {
            (Rx, 1) => Uart1RxIntr1,
            (Tx, 1) => Uart1TxIntr1,
            (Combined, 1) => IntUart1,
            (Rx, 2) => Uart2RxIntr2,
            (Tx, 2) => Uart2TxIntr2,
            (Combined, 2) => IntUart2,
            (Rx, 3) => Uart3RxIntr3,
            (Tx, 3) => Uart3TxIntr3,
            (Combined, 3) => IntUart3,
            _ => panic!("Unexpected index/interrupt"),
        }
    }
}

static INT_MASKS: [(UartInt, u8); 3] =
    [(UartInt::Tx, 4), (UartInt::Rx, 2), (UartInt::Combined, 15)];

#[derive(Debug, Default)]
struct Status {
    index: u8,

    linctrl: [u32; 3],
    ctrl: u32,

    // FIXME: Need to separate out bit time for the timeout interrupt
    bittime: Duration,
    word_len: u32,
    fifo_size: usize,
    overrun: bool,
    busy: bool,

    timeout: bool,
    cts_change: bool,

    rx_buf: VecDeque<u8>,
    tx_buf_size: usize,

    int_asserted: [bool; 3],
}

impl Status {
    fn new(index: u8) -> Self {
        let mut s: Self = Default::default();
        s.index = index;
        s.update_linctrl();
        s
    }

    fn update_linctrl(&mut self) {
        let high = self.linctrl[0];
        let bauddiv = ((self.linctrl[1] & 0xff) as u64) << 32 | (self.linctrl[2] as u64);
        let baud = UARTCLK_HZ / 16 / (bauddiv + 1);
        self.bittime = Duration::from_nanos(1_000_000_000 / baud);
        self.word_len = 1 + // start bit
            8 + // word length TODO: Allow for other word lengths than 8
            (if high & 0x8 != 0 { 2 } else { 1 }) + // stop bits
            (if high & 0x2 != 0 { 1 } else { 0 }); // parity bit
        self.fifo_size = if (high & 0x10) != 0 { 16 } else { 1 }
    }

    /// Returns the interrupt status in the format of the UARTxIntIDIntClr register
    fn get_int_id(&self) -> u8 {
        let mut result = 0;
        if self.timeout {
            result |= 8;
        }
        if self.tx_buf_size * 2 <= self.fifo_size {
            result |= 4;
        }
        if self.rx_buf.len() * 2 >= self.fifo_size {
            result |= 2;
        }
        if self.cts_change {
            result |= 1;
        }

        // the control register has the int enable data 3 bits up in the right order
        (result & (self.ctrl >> 3)) as u8
    }

    fn update_interrupts(&mut self, interrupt_bus: &Sender<(Interrupt, bool)>) {
        let int_id = self.get_int_id();

        for (int, mask) in INT_MASKS.iter() {
            let assert = (int_id & mask) != 0;
            if assert != self.int_asserted[*int as usize] {
                self.int_asserted[*int as usize] = assert;
                interrupt_bus
                    .send((int.hw_int(self.index), assert))
                    .unwrap();
            }
        }
    }
}

fn spawn_reader_thread(
    status: Arc<Mutex<Status>>,
    interrupt_bus: Sender<(Interrupt, bool)>,
) -> Sender<u8> {
    let _ = interrupt_bus;
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || loop {
        let (can_timeout, bittime, word_len) = {
            let status = status.lock().unwrap();
            (
                !status.rx_buf.is_empty() && !status.timeout,
                status.bittime,
                status.word_len,
            )
        };
        let b = if can_timeout {
            use RecvTimeoutError::*;
            match rx.recv_timeout(bittime * 32) {
                Ok(b) => Some(b),
                Err(Timeout) => None,
                Err(Disconnected) => break,
            }
        } else {
            match rx.recv() {
                Ok(b) => Some(b),
                Err(_) => break,
            }
        };

        match b {
            Some(b) => {
                thread::sleep(bittime * word_len);

                let mut status = status.lock().unwrap();
                if status.rx_buf.len() < status.fifo_size {
                    status.rx_buf.push_back(b);
                    status.update_interrupts(&interrupt_bus);
                } else {
                    warn!(
                        "UART {} dropping received byte due to full FIFO",
                        status.index
                    );
                }
            }
            None => {
                let mut status = status.lock().unwrap();
                status.timeout = true;
                status.update_interrupts(&interrupt_bus);
            }
        }
    });
    tx
}

fn spawn_writer_thread(
    status: Arc<Mutex<Status>>,
    interrupt_bus: Sender<(Interrupt, bool)>,
) -> (Receiver<u8>, Sender<u8>) {
    let _ = interrupt_bus;
    let (outer_tx, outer_rx) = mpsc::channel();
    let (inner_tx, inner_rx) = mpsc::channel();
    thread::spawn(move || {
        for b in inner_rx.iter() {
            // Sleep for the appropriate time
            let (bittime, word_len) = {
                let mut status = status.lock().unwrap();
                if !status.busy {
                    status.busy = true;
                    status.cts_change = true;
                    status.update_interrupts(&interrupt_bus);
                }

                (status.bittime, status.word_len)
            };
            thread::sleep(bittime * word_len);
            outer_tx.send(b).unwrap();
            {
                let mut status = status.lock().unwrap();

                status.tx_buf_size -= 1;
                if status.tx_buf_size == 0 {
                    status.busy = false;
                    status.cts_change = true;
                }
                status.update_interrupts(&interrupt_bus);
            }
        }
    });
    (outer_rx, inner_tx)
}

pub struct Uart {
    label: &'static str,

    status: Arc<Mutex<Status>>,

    interrupt_bus: Sender<(Interrupt, bool)>,

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
    pub fn new_hle(
        label: &'static str,
        interrupt_bus: Sender<(Interrupt, bool)>,
        index: u8,
    ) -> Uart {
        let status = Arc::new(Mutex::new(Status::new(index)));

        let input = spawn_reader_thread(status.clone(), interrupt_bus.clone());
        let (output, sender_tx) = spawn_writer_thread(status.clone(), interrupt_bus.clone());

        Uart {
            label,
            status,
            interrupt_bus,
            input,
            output: Some(output),
            sender_tx,
        }
    }

    /// Get the input channel for this UART
    pub fn get_input(&self) -> Sender<u8> {
        self.input.clone()
    }

    /// Get the output channel for this UART
    /// Panics if called more than once
    pub fn get_output(&mut self) -> Receiver<u8> {
        self.output.take().expect("Output already gotten")
    }

    fn lock_status(&mut self) -> MutexGuard<Status> {
        self.status.lock().unwrap()
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
                let mut status = self.status.lock().unwrap();
                let val = status.rx_buf.pop_front().unwrap_or(0) as u32;
                if status.rx_buf.is_empty() {
                    status.timeout = false;
                }
                status.update_interrupts(&self.interrupt_bus);
                Ok(val)
            }
            // read status
            0x04 => {
                let overrun = self.lock_status().overrun;
                Ok(if overrun { 8 } else { 0 })
            }
            // line control
            0x08 | 0x0C | 0x10 => {
                let idx = ((offset - 8) / 4) as usize;
                let val = self.lock_status().linctrl[idx];
                Ok(val)
            }
            // control
            0x14 => Ok(self.lock_status().ctrl),
            // flag
            0x18 => {
                let status = self.lock_status();
                let mut result = 0;
                if status.tx_buf_size == 0 {
                    result |= 0x80;
                }
                if status.rx_buf.len() >= status.fifo_size {
                    result |= 0x40;
                }
                if status.tx_buf_size >= status.fifo_size {
                    result |= 0x20;
                }
                if status.rx_buf.is_empty() {
                    result |= 0x10;
                }
                if status.busy {
                    result |= 0x8;
                } else {
                    // Hack: set cts when not sending data
                    // TODO: determine a better way to do cts
                    result |= 0x1;
                }
                Ok(result)
            }
            // interrupt identification and clear register
            0x1C => Ok(self.lock_status().get_int_id() as u32),
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
                let mut status = self.status.lock().unwrap();
                // Drop the byte if the fifo is full
                // NOTE: This isn't validated to be the same as
                // hardware behaviour, but it should produce
                // similar-looking errors to hardware in the long
                // run if there's a problem
                if status.tx_buf_size < status.fifo_size {
                    // A little awkward, but it is important that
                    // this send happens while under lock, as
                    // otherwise it could lead to a race condition
                    // where the sender thread locks status before
                    // this thread does.
                    self.sender_tx.send(val as u8).unwrap();
                    status.tx_buf_size += 1;
                    status.update_interrupts(&self.interrupt_bus);
                } else {
                    warn!("UART {} dropping sent byte due to full FIFO", status.index);
                }
                Ok(())
            }
            // read status
            0x04 => {
                let mut status = self.status.lock().unwrap();
                status.overrun = false;
                status.update_interrupts(&self.interrupt_bus);
                Ok(())
            }
            // line control
            0x08 | 0x0C | 0x10 => {
                let idx = ((offset - 8) / 4) as usize;
                let mut status = self.status.lock().unwrap();
                status.linctrl[idx] = val;
                status.update_linctrl();
                status.update_interrupts(&self.interrupt_bus);
                Ok(())
            }
            // control
            0x14 => {
                let mut status = self.status.lock().unwrap();
                status.ctrl = val;
                status.update_interrupts(&self.interrupt_bus);
                Ok(())
            }
            // flag
            0x18 => crate::mem_unimpl!("FLAG_REG"),
            // interrupt identification and clear register
            0x1C => {
                let mut status = self.status.lock().unwrap();
                status.cts_change = false;
                status.update_interrupts(&self.interrupt_bus);
                Ok(())
            }
            // dma control
            0x28 => crate::mem_unimpl!("DMAR_REG"),
            _ => crate::mem_unexpected!(),
        }
        .mem_ctx(offset, self)
    }
}
