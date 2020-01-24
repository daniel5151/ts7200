use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::memory::{MemResult, MemResultExt, Memory};

use super::{Interrupts, VicManager};

#[derive(Clone, Copy, Debug, PartialEq)]
enum Mode {
    FreeRunning = 0,
    Periodic = 1,
}

#[derive(Clone, Copy, Debug)]
enum Clock {
    Khz2 = 0,
    Khz508 = 1,
}

impl Clock {
    fn khz(&self) -> u64 {
        use Clock::*;
        match self {
            Khz2 => 2,
            Khz508 => 508,
        }
    }
}

enum InterrupterMsg {
    Enabled(Instant, Duration),
    Disabled,
}

fn spawn_interrupter_thread(
    assert_interrupt: Arc<AtomicBool>,
) -> (JoinHandle<()>, Sender<InterrupterMsg>) {
    let (tx, rx) = mpsc::channel::<InterrupterMsg>();
    let handle = thread::spawn(move || {
        let mut next = None;
        let mut period = Default::default();
        loop {
            let timeout = match next {
                Some(next) => next - Instant::now(),
                None => Duration::from_secs(std::u64::MAX),
            };

            match rx.recv_timeout(timeout) {
                Ok(InterrupterMsg::Enabled(new_next, new_period)) => {
                    next = Some(new_next);
                    period = new_period;
                }
                Ok(InterrupterMsg::Disabled) => next = None,
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    // Sender exited
                    return;
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    // Interrupt!
                    assert_interrupt.store(true, Ordering::Relaxed);
                    next = Some(next.unwrap() + period);
                    log::warn!("Period: {:?}", period);
                }
            }
        }
    });
    (handle, tx)
}

fn get_interrupt(index: i32) -> Interrupts {
    use Interrupts::*;
    match index {
        1 => Tc1Ui,
        2 => Tc2Ui,
        3 => Tc3Ui,
        _ => panic!("Invalid timer index {}", index),
    }
}

/// Timer module
///
/// As described in section 18
/// https://www.student.cs.uwaterloo.ca/~cs452/F19/docs/ep93xx-user-guide.pdf
pub struct Timer {
    label: &'static str,
    // registers
    loadval: Option<u32>,
    val: u32,
    enabled: bool,
    mode: Mode,
    clksel: Clock,
    // implementation details
    wrapmask: u32, // 0x0000FFFF for 16 bit timers, 0xFFFFFFFF for 32 bit timers
    last_time: Instant,
    microticks: u32,

    interrupt: Interrupts,
    interrupter_tx: mpsc::Sender<InterrupterMsg>,
    assert_interrupt: Arc<AtomicBool>,
    clear_interrupt: bool,
}

impl std::fmt::Debug for Timer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Timer").finish()
    }
}

impl Timer {
    /// Create a new Timer
    pub fn new(label: &'static str, index: i32, bits: usize) -> Timer {
        let assert_interrupt = Arc::new(AtomicBool::new(false));
        let (_, interrupter_tx) = spawn_interrupter_thread(assert_interrupt.clone());
        Timer {
            label,
            loadval: None,
            val: 0,
            enabled: false,
            mode: Mode::FreeRunning,
            clksel: Clock::Khz2,
            wrapmask: ((1u64 << bits) - 1) as u32,
            last_time: Instant::now(),
            microticks: 0,

            interrupt: get_interrupt(index),
            interrupter_tx,
            assert_interrupt: assert_interrupt,
            clear_interrupt: false,
        }
    }

    /// Lazily update the registers on read / write.
    fn update_regs(&mut self) {
        // calculate the time delta
        let now = Instant::now();
        let dt = now.duration_since(self.last_time).as_nanos() as u64;
        self.last_time = now;

        if !self.enabled {
            return;
        }

        let khz = self.clksel.khz();

        // calculate number of ticks the timer should decrement by
        let microticks = dt * khz + self.microticks as u64;
        let ticks = (microticks / 1_000_000) as u32;
        self.microticks = (microticks % 1_000_000) as u32;

        match self.mode {
            Mode::FreeRunning => {
                self.val = self.val.wrapping_sub(ticks) & self.wrapmask;
            }
            Mode::Periodic => {
                let loadval = match self.loadval {
                    Some(v) => v,
                    None => panic!("trying to use unset load value with {}", self.label),
                };
                self.val = if loadval == 0 {
                    0
                } else if self.val < ticks {
                    let remaining_ticks = ticks - self.val;
                    loadval - (remaining_ticks % loadval)
                } else {
                    self.val - ticks
                }
            }
        }
    }

    /// Check if interrupts should be asserted or cleared
    pub fn check_interrupts(&mut self, vicmgr: &mut VicManager) {
        if self.assert_interrupt.fetch_and(false, Ordering::Relaxed) {
            vicmgr.assert_interrupt(self.interrupt);
        } else if self.clear_interrupt {
            self.clear_interrupt = false;
            vicmgr.clear_interrupt(self.interrupt);
        }
    }
}

impl Memory for Timer {
    fn label(&self) -> Option<&str> {
        Some(self.label)
    }

    fn device(&self) -> &'static str {
        "Timer"
    }

    fn r32(&mut self, offset: u32) -> MemResult<u32> {
        self.update_regs();

        match offset {
            0x00 => Ok(match self.loadval {
                Some(v) => v,
                None => panic!("tried to read {} Load before it's been set it", self.label),
            }),
            0x04 => Ok(self.val),
            0x08 => {
                let val = ((self.clksel as u32) << 3)
                    | ((self.mode as u32) << 6)
                    | ((self.enabled as u32) << 7);
                Ok(val)
            }
            // TODO: implement timer interrupts
            0x0C => crate::mem_unimpl!("CLR_REG"),
            _ => crate::mem_unexpected!(),
        }
        .mem_ctx(offset, self)
    }

    fn w32(&mut self, offset: u32, val: u32) -> MemResult<()> {
        self.update_regs();

        match offset {
            0x00 => {
                // "The Load register should not be written after the Timer is enabled because
                // this causes the Timer Value register to be updated with an undetermined
                // value."
                if self.enabled {
                    panic!("tried to write to {} Load while the timer is enabled", val);
                }

                let val = val & self.wrapmask;
                self.loadval = Some(val);
                // "The Timer Value register is updated with the Timer Load value as soon as the
                // Timer Load register is written"
                self.val = val;
                Ok(())
            }
            0x04 => {
                // TODO: add warning about writing to registers that _shouldn't_ be written to,
                // instead of this hard panic
                panic!("tried to write value to Write-only Timer register");
            }
            0x08 => {
                self.clksel = match val & (1 << 3) != 0 {
                    true => Clock::Khz508,
                    false => Clock::Khz2,
                };
                self.mode = match val & (1 << 6) != 0 {
                    true => Mode::Periodic,
                    false => Mode::FreeRunning,
                };
                let previous_enabled = self.enabled;
                self.enabled = val & (1 << 7) != 0;

                if self.enabled && !previous_enabled {
                    self.microticks = 0;

                    if self.mode == Mode::Periodic {
                        log::warn!("loadval: {:?} clksel: {:?}", self.loadval, self.clksel);
                        let period = Duration::from_nanos(
                            (self.loadval.unwrap() as u64) * 1_000_000 / self.clksel.khz(),
                        );
                        self.interrupter_tx
                            .send(InterrupterMsg::Enabled(Instant::now() + period, period))
                            .unwrap();
                    }
                }
                if !self.enabled {
                    self.loadval = None;
                    self.interrupter_tx.send(InterrupterMsg::Disabled).unwrap();
                }

                Ok(())
            }
            0x0C => Ok(self.clear_interrupt = true),
            _ => crate::mem_unexpected!(),
        }
        .mem_ctx(offset, self)
    }
}
