use std::collections::VecDeque;
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crossbeam_channel::{self as mpsc, select};
use log::*;

use crate::devices::vic::Interrupt;
use crate::memory::{MemResult, MemResultExt, Memory};

// Derived from section 14 of the EP93xx User's Guide and the provided value for
// bauddiv from CS452.
// TODO: A better source for UARTCLK_HZ would be appreciated.
const UARTCLK_HZ: u64 = 7_372_800;

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

    fn new_hle(index: u8) -> Self {
        let mut s = Self::new(index);
        // 8 bit word, FIFO enable
        s.linctrl[0] = 0x70;
        // 115200 baud
        s.linctrl[1] = 0;
        s.linctrl[2] = 3;

        // UART enable
        s.ctrl = 1;

        s.update_linctrl();

        s
    }

    fn update_linctrl(&mut self) {
        let high = self.linctrl[0];
        let bauddiv = ((self.linctrl[1] & 0xff) as u64) << 32 | (self.linctrl[2] as u64);
        let baud = UARTCLK_HZ / 16 / (bauddiv + 1);
        self.bittime = Duration::from_nanos(1_000_000_000 / baud);
        self.word_len = 1 // start bit
            + 8 // word length TODO: Allow for other word lengths than 8
            + (if high & 0x8 != 0 { 2 } else { 1 }) // stop bits
            + (if high & 0x2 != 0 { 1 } else { 0 }); // parity bit
        self.fifo_size = if (high & 0x10) != 0 { 16 } else { 1 }
    }

    /// Returns the interrupt status in the format of the UARTxIntIDIntClr
    /// register
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

    fn update_interrupts(&mut self, interrupt_bus: &mpsc::Sender<(Interrupt, bool)>) {
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

        let int_id = self.get_int_id();

        for (int, mask) in [(UartInt::Tx, 4), (UartInt::Rx, 2), (UartInt::Combined, 15)].iter() {
            let assert = (int_id & mask) != 0;
            if assert != self.int_asserted[*int as usize] {
                self.int_asserted[*int as usize] = assert;
                let hw_int = int.hw_int(self.index);
                trace!(
                    "UART{} setting interrupt {:?} to {:?} from {}",
                    self.index,
                    hw_int,
                    assert,
                    int_id
                );
                interrupt_bus.send((hw_int, assert)).unwrap();
            }
        }
    }
}

struct Exit;

/// Structured return type for the various channels created as part of spawning
/// a UART input buffer thread
struct InputBufferThreadChans {
    pub exit: mpsc::Sender<Exit>,
    pub uart_input: mpsc::Sender<u8>,
}

fn spawn_input_buffer_thread(
    label: &'static str,
    status: Arc<Mutex<Status>>,
    interrupt_bus: mpsc::Sender<(Interrupt, bool)>,
) -> (JoinHandle<()>, InputBufferThreadChans) {
    let (uart_tx, uart_rx) = mpsc::unbounded();
    let (exit_tx, exit_rx) = mpsc::bounded(1);
    let thread = move || loop {
        let (can_timeout, bittime, word_len) = {
            let status = status.lock().unwrap();
            (
                !status.rx_buf.is_empty() && !status.timeout,
                status.bittime,
                status.word_len,
            )
        };
        let b = if can_timeout {
            select! {
                recv(uart_rx) -> b => match b {
                    Ok(b) => Some(b),
                    Err(mpsc::RecvError) => panic!("uart_rx closed unexpectedly"),
                },
                recv(exit_rx) -> _ => break,
                default(bittime * 32) => None,
            }
        } else {
            select! {
                recv(uart_rx) -> b => match b {
                    Ok(b) => Some(b),
                    Err(mpsc::RecvError) => panic!("uart_rx closed unexpectedly"),
                },
                recv(exit_rx) -> _ => break,
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
                        "UART{} dropping received byte due to full FIFO",
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
    };

    let handle = thread::Builder::new()
        .name(format!("{} | UART Internal Reader", label))
        .spawn(thread)
        .unwrap();

    (
        handle,
        InputBufferThreadChans {
            exit: exit_tx,
            uart_input: uart_tx,
        },
    )
}

/// Structured return type for the various channels created as part of spawning
/// a UART output buffer thread
struct OutputBufferThreadChans {
    pub exit: mpsc::Sender<Exit>,
    pub uart_output: mpsc::Receiver<u8>,
    pub device_output: mpsc::Sender<u8>,
}

fn spawn_output_buffer_thread(
    label: &'static str,
    status: Arc<Mutex<Status>>,
    interrupt_bus: mpsc::Sender<(Interrupt, bool)>,
) -> (JoinHandle<()>, OutputBufferThreadChans) {
    let (uart_tx, uart_rx) = mpsc::unbounded();
    let (device_tx, device_rx) = mpsc::unbounded();
    let (exit_tx, exit_rx) = mpsc::bounded(1);
    let thread = move || {
        loop {
            let b = select! {
                recv(device_rx) -> b => match b {
                    Ok(b) => b,
                    Err(mpsc::RecvError) => panic!("tx closed unexpectedly"),
                },
                recv(exit_rx) -> _ => break,
            };

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
            match uart_tx.send(b) {
                Ok(()) => (),
                Err(mpsc::SendError(_)) => {
                    // Receiving end closed
                    return;
                }
            }
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
        for b in device_rx.try_iter() {
            uart_tx.send(b).expect("io receiver closed unexpectedly")
        }
    };

    let handle = thread::Builder::new()
        .name(format!("{} | UART Internal Writer", label))
        .spawn(thread)
        .unwrap();

    (
        handle,
        OutputBufferThreadChans {
            exit: exit_tx,
            uart_output: uart_rx,
            device_output: device_tx,
        },
    )
}

/// Newtype around `JoinHandle<()>`. Constructed via `.into()`
#[derive(Debug)]
pub struct UserReader(JoinHandle<()>);

impl From<JoinHandle<()>> for UserReader {
    fn from(handle: JoinHandle<()>) -> UserReader {
        UserReader(handle)
    }
}

/// Newtype around `JoinHandle<()>`. Constructed via `.into()`
#[derive(Debug)]
pub struct UserWriter(JoinHandle<()>);

impl From<JoinHandle<()>> for UserWriter {
    fn from(handle: JoinHandle<()>) -> UserWriter {
        UserWriter(handle)
    }
}

/// Owner of the UART's internal Input buffer and Output buffer threads, their
/// associated channels, and any User provided Reader/Writer handlers.
///
/// When dropped, the UartWorker ensures that the UART's internal buffer threads
/// are terminated _before_ waiting for any user provided Reader/Writer threads
/// to terminate.
#[derive(Debug)]
struct UartWorker {
    input_buffer_thread_exit: mpsc::Sender<Exit>,
    output_buffer_thread_exit: mpsc::Sender<Exit>,
    // must be optional, as `.join()` can only be called on an owned JoinHandle
    input_buffer_thread: Option<JoinHandle<()>>,
    output_buffer_thread: Option<JoinHandle<()>>,

    uart_input_chan: mpsc::Sender<u8>,
    uart_output_chan: mpsc::Receiver<u8>,
    device_output_chan: mpsc::Sender<u8>,

    user_reader_handler: Option<UserReader>,
    user_writer_handler: Option<UserWriter>,
}

impl Drop for UartWorker {
    fn drop(&mut self) {
        self.input_buffer_thread_exit
            .send(Exit)
            .expect("uart worker reader thread was unexpectedly terminated");
        self.output_buffer_thread_exit
            .send(Exit)
            .expect("uart worker writer thread was unexpectedly terminated");

        self.input_buffer_thread.take().unwrap().join().unwrap();
        self.output_buffer_thread.take().unwrap().join().unwrap();

        // HACK: don't actually join on the user_reader_thread
        // reader threads are typically blocked on IO, and don't have an easy way to
        // check if the other end of their send channel has closed.
        // TODO: provide a mechanism to cleanly close UserReader tasks

        // if let Some(user_reader_handler) = self.user_reader_handler.take() {
        //     user_reader_handler.0.join().unwrap();
        // }

        if let Some(user_writer_handler) = self.user_writer_handler.take() {
            user_writer_handler.0.join().unwrap();
        };
    }
}

impl UartWorker {
    fn new(
        label: &'static str,
        status: Arc<Mutex<Status>>,
        interrupt_bus: mpsc::Sender<(Interrupt, bool)>,
    ) -> UartWorker {
        let (input_buffer_thread, input_chans) =
            spawn_input_buffer_thread(label, status.clone(), interrupt_bus.clone());
        let (output_buffer_thread, output_chans) =
            spawn_output_buffer_thread(label, status, interrupt_bus);

        UartWorker {
            input_buffer_thread_exit: input_chans.exit,
            output_buffer_thread_exit: output_chans.exit,
            input_buffer_thread: Some(input_buffer_thread),
            output_buffer_thread: Some(output_buffer_thread),
            uart_input_chan: input_chans.uart_input,
            uart_output_chan: output_chans.uart_output,
            device_output_chan: output_chans.device_output,
            user_reader_handler: None,
            user_writer_handler: None,
        }
    }
}

/// UART device implementing all behavior shared by UARTs 1, 2, and 3 on the
/// TS-7200. i.e: this device doesn't include any UART-specific functionality,
/// such as HDCL or Modem controls.
///
/// As described in sections 14, 15, and 16 of the EP93xx User's Guide.
#[derive(Debug)]
pub struct Uart {
    label: &'static str,

    status: Arc<Mutex<Status>>,
    interrupt_bus: mpsc::Sender<(Interrupt, bool)>,

    worker: UartWorker,
}

impl Drop for Uart {
    fn drop(&mut self) {}
}

impl Uart {
    /// Create a new uart
    pub fn new_hle(
        label: &'static str,
        interrupt_bus: mpsc::Sender<(Interrupt, bool)>,
        index: u8,
    ) -> Uart {
        let status = Arc::new(Mutex::new(Status::new_hle(index)));

        let worker = UartWorker::new(label, status.clone(), interrupt_bus.clone());

        Uart {
            label,
            status,
            interrupt_bus,
            worker,
        }
    }

    /// Register an input handler thread with the UART.
    ///
    /// The provided thread SHOULD send data to UART via the provided Sender
    /// channel, and MUST terminate if the Sender hangs up.
    ///
    /// Returns the UserReader of any previous thread that was registered with
    /// the UART.
    pub fn install_reader<E>(
        &mut self,
        install_reader: impl FnOnce(mpsc::Sender<u8>) -> Result<UserReader, E>,
    ) -> Result<Option<UserReader>, E> {
        let ret = self.worker.user_reader_handler.take();
        self.worker.user_reader_handler =
            Some(install_reader(self.worker.uart_input_chan.clone())?);
        Ok(ret)
    }

    /// Register an output handler thread with the UART.
    ///
    /// The provided thread SHOULD receive data to UART via the provided
    /// Receiver channel, and MUST terminate if the Receiver hangs up.
    ///
    /// Returns the UserWriter of any previous thread that was registered
    /// with the UART.
    pub fn install_writer<E>(
        &mut self,
        install_writer: impl FnOnce(mpsc::Receiver<u8>) -> Result<UserWriter, E>,
    ) -> Result<Option<UserWriter>, E> {
        let ret = self.worker.user_writer_handler.take();
        self.worker.user_writer_handler =
            Some(install_writer(self.worker.uart_output_chan.clone())?);
        Ok(ret)
    }

    /// Register a pair of Input and Output handler threads with the UART.
    ///
    /// The provided threads SHOULD send/receiver data to/from UART via the
    /// provided Sender/Receiver channels, and MUST terminate if the
    /// Sender/Receiver hang up.
    ///
    /// Returns the UserReader/UserWriter of any previous threads that were
    /// registered with the UART.
    pub fn install_io_handlers<E>(
        &mut self,
        install_io_handlers: impl FnOnce(
            mpsc::Sender<u8>,
            mpsc::Receiver<u8>,
        ) -> Result<(UserReader, UserWriter), E>,
    ) -> Result<(Option<UserReader>, Option<UserWriter>), E> {
        let ret = (
            self.worker.user_reader_handler.take(),
            self.worker.user_writer_handler.take(),
        );
        let (in_handle, out_handle) = install_io_handlers(
            self.worker.uart_input_chan.clone(),
            self.worker.uart_output_chan.clone(),
        )?;
        self.worker.user_reader_handler = Some(in_handle);
        self.worker.user_writer_handler = Some(out_handle);
        Ok(ret)
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
                // If the buffer is empty return a dummy value
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
                    // XXX: set cts when not sending data
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
                if status.tx_buf_size < status.fifo_size {
                    // A little awkward, but it is important that
                    // this send happens while under lock, as
                    // otherwise it could lead to a race condition
                    // where the sender thread locks status before
                    // this thread does.
                    self.worker.device_output_chan.send(val as u8).unwrap();
                    status.tx_buf_size += 1;
                    status.update_interrupts(&self.interrupt_bus);
                } else {
                    warn!("{} dropping sent byte due to full FIFO", self.label);
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
                if status.cts_change {
                    trace!("{} clearing cts interrupt", self.label);
                }
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
