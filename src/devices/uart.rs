use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crossbeam_channel::{self as chan, select};
use log::*;

use crate::devices::vic::Interrupt;
use crate::memory::{MemException::*, MemResult, Memory};

/// Aggregate type to configure which Interrupts should be generated by the UART
#[derive(Debug)]
pub struct UartInterrupts {
    pub rx: Interrupt,
    pub tx: Interrupt,
    pub combo: Interrupt,
}

/// List of interrupts generated by each of the various UARTs on the TS-7200
pub mod interrupts {
    use super::UartInterrupts;
    use crate::devices::vic::Interrupt::*;

    /// Interrupts associated with UART1
    pub const UART1: UartInterrupts = UartInterrupts {
        rx: Uart1RxIntr1,
        tx: Uart1TxIntr1,
        combo: IntUart1,
    };

    /// Interrupts associated with UART2
    pub const UART2: UartInterrupts = UartInterrupts {
        rx: Uart2RxIntr2,
        tx: Uart2TxIntr2,
        combo: IntUart2,
    };

    /// Interrupts associated with UART3
    pub const UART3: UartInterrupts = UartInterrupts {
        rx: Uart3RxIntr3,
        tx: Uart3TxIntr3,
        combo: IntUart3,
    };
}

/// Derived from section 14 of the EP93xx User's Guide and the provided value
/// for bauddiv from CS452.
// TODO: A better source for UARTCLK_HZ would be appreciated.
const UARTCLK_HZ: u64 = 7_372_800;

/// UART internal register state.
///
/// Shared between the UART device and it's workers using a Mutex
#[derive(Debug)]
struct State {
    label: &'static str,
    interrupts: UartInterrupts,

    linctrl_latched: bool,
    linctrl_latch: [u32; 3],
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

    rx_int_asserted: bool,
    tx_int_asserted: bool,
    combo_int_asserted: bool,
}

impl State {
    fn new(label: &'static str, interrupts: UartInterrupts) -> State {
        let mut s = State {
            label,
            interrupts,

            linctrl_latched: false,
            linctrl_latch: [0, 0, 0],
            linctrl: [0, 0, 0],
            ctrl: 0,

            // set to proper defaults once update_linctrl is called below
            bittime: Duration::default(),
            word_len: 0,
            fifo_size: 0,

            overrun: false,
            busy: false,
            timeout: false,
            cts_change: false,

            rx_buf: VecDeque::new(),
            tx_buf_size: 0,

            rx_int_asserted: false,
            tx_int_asserted: false,
            combo_int_asserted: false,
        };
        s.update_linctrl();
        s
    }

    fn new_hle(label: &'static str, interrupts: UartInterrupts) -> State {
        let mut s = State::new(label, interrupts);
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

    fn update_interrupts(&mut self, interrupt_bus: &chan::Sender<(Interrupt, bool)>) {
        let int_id = self.get_int_id();

        macro_rules! update_interrupt {
            ($hw_int:expr, $is_asserted:expr, $mask:expr) => {
                let assert = (int_id & $mask) != 0;
                if assert != $is_asserted {
                    $is_asserted = assert;
                    trace!(
                        "UART {} setting interrupt {:?} to {:?} from {}",
                        self.label,
                        $hw_int,
                        assert,
                        int_id
                    );
                    interrupt_bus.send(($hw_int, assert)).unwrap();
                }
            };
        }

        update_interrupt!(self.interrupts.tx, self.tx_int_asserted, 0b1000);
        update_interrupt!(self.interrupts.rx, self.rx_int_asserted, 0b0010);
        update_interrupt!(self.interrupts.combo, self.combo_int_asserted, 0b1111);
    }
}

struct Exit;

/// Structured return type for the various channels created as part of spawning
/// a UART input buffer thread
struct InputBufferThreadChans {
    pub exit: chan::Sender<Exit>,
    pub uart_input: chan::Sender<u8>,
}

fn spawn_input_buffer_thread(
    label: &'static str,
    state: Arc<Mutex<State>>,
    interrupt_bus: chan::Sender<(Interrupt, bool)>,
) -> (JoinHandle<()>, InputBufferThreadChans) {
    let (uart_tx, uart_rx) = chan::unbounded();
    let (exit_tx, exit_rx) = chan::bounded(1);
    let thread = move || loop {
        let (can_timeout, bittime, word_len) = {
            let state = state.lock().unwrap();
            (
                !state.rx_buf.is_empty() && !state.timeout,
                state.bittime,
                state.word_len,
            )
        };
        let b = if can_timeout {
            select! {
                recv(uart_rx) -> b => match b {
                    Ok(b) => Some(b),
                    Err(chan::RecvError) => panic!("uart_rx closed unexpectedly"),
                },
                recv(exit_rx) -> _ => break,
                default(bittime * 32) => None,
            }
        } else {
            select! {
                recv(uart_rx) -> b => match b {
                    Ok(b) => Some(b),
                    Err(chan::RecvError) => panic!("uart_rx closed unexpectedly"),
                },
                recv(exit_rx) -> _ => break,
            }
        };

        match b {
            Some(b) => {
                thread::sleep(bittime * word_len);

                let mut state = state.lock().unwrap();
                if state.rx_buf.len() < state.fifo_size {
                    state.rx_buf.push_back(b);
                    state.update_interrupts(&interrupt_bus);
                } else {
                    warn!("UART {} dropping received byte due to full FIFO", label);
                    state.overrun = true;
                }
            }
            None => {
                let mut state = state.lock().unwrap();
                if state.rx_buf.len() > 0 {
                    state.timeout = true;
                    state.update_interrupts(&interrupt_bus);
                }
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
    pub exit: chan::Sender<Exit>,
    pub uart_output: chan::Receiver<u8>,
    pub device_output: chan::Sender<u8>,
}

fn spawn_output_buffer_thread(
    label: &'static str,
    state: Arc<Mutex<State>>,
    interrupt_bus: chan::Sender<(Interrupt, bool)>,
) -> (JoinHandle<()>, OutputBufferThreadChans) {
    let (uart_tx, uart_rx) = chan::unbounded();
    let (device_tx, device_rx) = chan::unbounded();
    let (exit_tx, exit_rx) = chan::bounded(1);
    let thread = move || {
        loop {
            let b = select! {
                recv(device_rx) -> b => match b {
                    Ok(b) => b,
                    Err(chan::RecvError) => panic!("tx closed unexpectedly"),
                },
                recv(exit_rx) -> _ => break,
            };

            // Sleep for the appropriate time
            let (bittime, word_len) = {
                let mut state = state.lock().unwrap();
                if !state.busy {
                    state.busy = true;
                    state.cts_change = true;
                    state.update_interrupts(&interrupt_bus);
                }

                (state.bittime, state.word_len)
            };
            thread::sleep(bittime * word_len);
            match uart_tx.send(b) {
                Ok(()) => (),
                Err(chan::SendError(_)) => {
                    // Receiving end closed
                    return;
                }
            }
            {
                let mut state = state.lock().unwrap();

                state.tx_buf_size -= 1;
                if state.tx_buf_size == 0 {
                    state.busy = false;
                    state.cts_change = true;
                }
                state.update_interrupts(&interrupt_bus);
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

/// User-provided task for providing input into a UART
#[derive(Debug)]
pub struct ReaderTask {
    handle: JoinHandle<()>,
}

impl ReaderTask {
    /// Create a new ReaderTask
    pub fn new(handle: JoinHandle<()>) -> ReaderTask {
        ReaderTask { handle }
    }
}

/// User-provided task for providing input into a UART
#[derive(Debug)]
pub struct WriterTask {
    handle: JoinHandle<()>,
}

impl WriterTask {
    /// Create a new WriterTask
    pub fn new(handle: JoinHandle<()>) -> WriterTask {
        WriterTask { handle }
    }
}

/// Owner of the UART's internal Input buffer and Output buffer threads, their
/// associated channels, and any User provided Reader/Writer tasks.
///
/// When dropped, the UartWorker ensures that the UART's internal buffer threads
/// are terminated _before_ waiting for any user provided Reader/Writer threads
/// to terminate.
#[derive(Debug)]
struct UartWorker {
    input_buffer_thread_exit: chan::Sender<Exit>,
    output_buffer_thread_exit: chan::Sender<Exit>,
    // must be optional, as `.join()` can only be called on an owned JoinHandle
    input_buffer_thread: Option<JoinHandle<()>>,
    output_buffer_thread: Option<JoinHandle<()>>,

    uart_input_chan: chan::Sender<u8>,
    uart_output_chan: chan::Receiver<u8>,
    device_output_chan: chan::Sender<u8>,

    user_reader_task: Option<ReaderTask>,
    user_writer_task: Option<WriterTask>,
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
        // TODO: provide a mechanism to cleanly close ReaderTask tasks

        // if let Some(user_reader_task) = self.user_reader_task.take() {
        //     user_reader_task.0.join().unwrap();
        // }

        if let Some(user_writer_task) = self.user_writer_task.take() {
            user_writer_task.handle.join().unwrap();
        };
    }
}

impl UartWorker {
    fn new(
        label: &'static str,
        state: Arc<Mutex<State>>,
        interrupt_bus: chan::Sender<(Interrupt, bool)>,
    ) -> UartWorker {
        let (input_buffer_thread, input_chans) =
            spawn_input_buffer_thread(label, state.clone(), interrupt_bus.clone());
        let (output_buffer_thread, output_chans) =
            spawn_output_buffer_thread(label, state, interrupt_bus);

        UartWorker {
            input_buffer_thread_exit: input_chans.exit,
            output_buffer_thread_exit: output_chans.exit,
            input_buffer_thread: Some(input_buffer_thread),
            output_buffer_thread: Some(output_buffer_thread),
            uart_input_chan: input_chans.uart_input,
            uart_output_chan: output_chans.uart_output,
            device_output_chan: output_chans.device_output,
            user_reader_task: None,
            user_writer_task: None,
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
    state: Arc<Mutex<State>>,
    interrupt_bus: chan::Sender<(Interrupt, bool)>,
    worker: UartWorker,
}

impl Uart {
    /// Create a new uart
    pub fn new_hle(
        label: &'static str,
        interrupt_bus: chan::Sender<(Interrupt, bool)>,
        interrupts: UartInterrupts,
    ) -> Uart {
        let state = Arc::new(Mutex::new(State::new_hle(label, interrupts)));
        let worker = UartWorker::new(label, state.clone(), interrupt_bus.clone());
        Uart {
            label,
            state,
            interrupt_bus,
            worker,
        }
    }

    /// Register a pair of Input and Output tasks with the UART.
    ///
    /// The provided tasks SHOULD send/receive data to/from UART via the
    /// provided Sender/Receiver channels, and MUST terminate if the
    /// Sender/Receiver hang up.
    ///
    /// Returns the ReaderTask/WriterTask of any previous tasks that may have
    /// been registered with the UART.
    pub fn install_io_tasks<E>(
        &mut self,
        install_io_tasks: impl FnOnce(
            chan::Sender<u8>,
            chan::Receiver<u8>,
        ) -> Result<(ReaderTask, WriterTask), E>,
    ) -> Result<(Option<ReaderTask>, Option<WriterTask>), E> {
        let ret = (
            self.worker.user_reader_task.take(),
            self.worker.user_writer_task.take(),
        );
        let (in_handle, out_handle) = install_io_tasks(
            self.worker.uart_input_chan.clone(),
            self.worker.uart_output_chan.clone(),
        )?;
        self.worker.user_reader_task = Some(in_handle);
        self.worker.user_writer_task = Some(out_handle);
        Ok(ret)
    }
}

impl Memory for Uart {
    fn device(&self) -> &'static str {
        "UART"
    }

    fn label(&self) -> Option<&str> {
        Some(self.label)
    }

    fn id_of(&self, offset: u32) -> Option<String> {
        let reg = match offset {
            0x00 => "Data",
            0x04 => "RXSts",
            0x08 => "LinCtrlHigh",
            0x0C => "LinCtrlMid",
            0x10 => "LinCtrlLow",
            0x14 => "Ctrl",
            0x18 => "Flag",
            0x1C => "IntIDIntClr",
            0x20 => "IrLowPwrCntr",
            0x28 => "DMACtrl",
            _ => return None,
        };
        Some(reg.to_string())
    }

    fn r32(&mut self, offset: u32) -> MemResult<u32> {
        let mut state = self.state.lock().unwrap();
        match offset {
            // data (8-bit)
            0x00 => {
                // If the buffer is empty return a dummy value
                let val = state.rx_buf.pop_front().unwrap_or(0) as u32;
                if state.rx_buf.is_empty() {
                    state.timeout = false;
                }
                state.update_interrupts(&self.interrupt_bus);
                Ok(val)
            }
            // read status
            0x04 => Ok(if state.overrun { 8 } else { 0 }),
            // line control high
            0x08 => Ok(state.linctrl[0]),
            // line control mid
            0x0C => {
                if state.linctrl_latched {
                    Err(ContractViolation {
                        msg: "Tried to read stale data (did you forget to update LinCtrlHigh?)"
                            .to_string(),
                        severity: log::Level::Warn,
                        stub_val: None,
                    })
                } else {
                    Ok(state.linctrl[1])
                }
            }
            // line control low
            0x10 => {
                if state.linctrl_latched {
                    Err(ContractViolation {
                        msg: "Tried to read stale data (did you forget to update LinCtrlHigh?)"
                            .to_string(),
                        severity: log::Level::Warn,
                        stub_val: None,
                    })
                } else {
                    Ok(state.linctrl[2])
                }
            }
            // control
            0x14 => Ok(state.ctrl),
            // flag
            0x18 => {
                let mut result = 0;
                if state.tx_buf_size == 0 {
                    result |= 0x80;
                }
                if state.rx_buf.len() >= state.fifo_size {
                    result |= 0x40;
                }
                if state.tx_buf_size >= state.fifo_size {
                    result |= 0x20;
                }
                if state.rx_buf.is_empty() {
                    result |= 0x10;
                }
                if state.busy {
                    result |= 0x8;
                } else {
                    // XXX: set cts when not sending data
                    // TODO: determine a better way to do cts
                    result |= 0x1;
                }
                Ok(result)
            }
            // interrupt identification and clear register
            0x1C => Ok(state.get_int_id() as u32),
            // dma control
            0x28 => Err(Unimplemented),
            _ => Err(Unexpected),
        }
    }

    fn w32(&mut self, offset: u32, val: u32) -> MemResult<()> {
        let mut state = self.state.lock().unwrap();
        match offset {
            // data (8-bit)
            0x00 => {
                // Drop the byte if the fifo is full
                if state.tx_buf_size < state.fifo_size {
                    // A little awkward, but it is important that
                    // this send happens while under lock, as
                    // otherwise it could lead to a race condition
                    // where the sender thread locks state before
                    // this thread does.
                    self.worker.device_output_chan.send(val as u8).unwrap();
                    state.tx_buf_size += 1;
                    state.update_interrupts(&self.interrupt_bus);
                } else {
                    warn!("{} dropping sent byte due to full FIFO", self.label);
                }
                Ok(())
            }
            // write status
            0x04 => {
                state.overrun = false;
                state.update_interrupts(&self.interrupt_bus);
                Ok(())
            }
            // line control high
            0x08 => {
                state.linctrl_latched = false;
                state.linctrl_latch[0] = val;

                state.linctrl = state.linctrl_latch;
                state.update_linctrl();
                state.update_interrupts(&self.interrupt_bus);
                Ok(())
            }
            // line control mid
            0x0C => {
                state.linctrl_latched = true;
                state.linctrl_latch[1] = val;
                Ok(())
            }
            // line control low
            0x10 => {
                state.linctrl_latched = true;
                state.linctrl_latch[2] = val;
                Ok(())
            }
            // control
            0x14 => {
                state.ctrl = val;
                state.update_interrupts(&self.interrupt_bus);
                Ok(())
            }
            // flag
            0x18 => Err(InvalidAccess),
            // interrupt identification and clear register
            0x1C => {
                if state.cts_change {
                    trace!("{} clearing cts interrupt", self.label);
                }
                state.cts_change = false;
                state.update_interrupts(&self.interrupt_bus);
                Ok(())
            }
            // dma control
            0x28 => Err(Unimplemented),
            _ => Err(Unexpected),
        }
    }
}
