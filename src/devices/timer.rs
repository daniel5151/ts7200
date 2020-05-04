use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crossbeam_channel as chan;

use crate::memory::{Device, MemException::*, MemResult, Memory, Probe};

use super::vic::Interrupt;

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
    fn khz(self) -> u64 {
        use Clock::*;
        match self {
            Khz2 => 2,
            Khz508 => 508,
        }
    }
}

enum InterrupterMsg {
    Enabled { next: Instant, period: Duration },
    Disabled,
}

fn spawn_interrupter_thread(
    label: &'static str,
    interrupt_bus: chan::Sender<(Interrupt, bool)>,
    interrupt: Interrupt,
) -> (JoinHandle<()>, chan::Sender<InterrupterMsg>) {
    let (tx, rx) = chan::unbounded::<InterrupterMsg>();
    let thread = move || {
        let mut next: Option<Instant> = None;
        let mut period = Default::default();
        loop {
            let timeout = match next {
                Some(next) => next.saturating_duration_since(Instant::now()),
                // XXX: Not technically correct, but this is a long enough time
                None => Duration::from_secs(std::u32::MAX as _),
            };

            match rx.recv_timeout(timeout) {
                Ok(InterrupterMsg::Enabled {
                    next: new_next,
                    period: new_period,
                }) => {
                    next = Some(new_next);
                    period = new_period;
                }
                Ok(InterrupterMsg::Disabled) => next = None,
                Err(chan::RecvTimeoutError::Disconnected) => {
                    // Sender exited
                    return;
                }
                Err(chan::RecvTimeoutError::Timeout) => {
                    // Interrupt!
                    interrupt_bus.send((interrupt, true)).unwrap();
                    next = Some(
                        next.expect("Impossible: We timed out with an infinite timeout") + period,
                    );
                }
            }
        }
    };

    let handle = thread::Builder::new()
        .name(format!("{} | Timer Interrupter", label))
        .spawn(thread)
        .unwrap();

    (handle, tx)
}

/// 32bit timer device with configurable emulated wrap value (for emulating 16
/// bit timers as well).
///
/// As described in section 18 of the EP93xx User's Guide
#[derive(Debug)]
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

    interrupt_bus: chan::Sender<(Interrupt, bool)>,
    interrupt: Interrupt,

    interrupter_tx: chan::Sender<InterrupterMsg>,
}

impl Timer {
    /// Create a new Timer
    pub fn new(
        label: &'static str,
        interrupt_bus: chan::Sender<(Interrupt, bool)>,
        interrupt: Interrupt,
        bits: usize,
    ) -> Timer {
        let (_, interrupter_tx) = spawn_interrupter_thread(label, interrupt_bus.clone(), interrupt);
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

            interrupt,
            interrupter_tx,
            interrupt_bus,
        }
    }

    /// Lazily update the registers on read / write.
    fn update_regs(&mut self) -> MemResult<()> {
        // calculate the time delta
        let now = Instant::now();
        let dt = now.duration_since(self.last_time).as_nanos() as u64;
        self.last_time = now;

        if !self.enabled {
            return Ok(());
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
                    None => {
                        return Err(ContractViolation {
                            msg: "Periodic mode enabled before setting a Load value".to_string(),
                            severity: log::Level::Error,
                            stub_val: None,
                        })
                    }
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

        Ok(())
    }
}

impl Device for Timer {
    fn kind(&self) -> &'static str {
        "Timer"
    }

    fn label(&self) -> Option<&str> {
        Some(self.label)
    }

    fn probe(&self, offset: u32) -> Probe<'_> {
        let reg = match offset {
            0x00 => "Load",
            0x04 => "Value",
            0x08 => "Control",
            0x0C => "Clear",
            _ => return Probe::Unmapped,
        };
        Probe::Register(reg)
    }
}

impl Memory for Timer {
    fn r32(&mut self, offset: u32) -> MemResult<u32> {
        self.update_regs()?;

        match offset {
            0x00 => match self.loadval {
                Some(v) => Ok(v),
                None => Err(ContractViolation {
                    msg: "Cannot read Load register before it's been set".to_string(),
                    severity: log::Level::Error,
                    stub_val: None,
                }),
            },
            0x04 => Ok(self.val),
            0x08 => {
                let val = ((self.clksel as u32) << 3)
                    | ((self.mode as u32) << 6)
                    | ((self.enabled as u32) << 7);
                Ok(val)
            }
            0x0C => Err(InvalidAccess),
            _ => Err(Unexpected),
        }
    }

    fn w32(&mut self, offset: u32, val: u32) -> MemResult<()> {
        self.update_regs()?;

        match offset {
            0x00 => {
                // "The Load register should not be written after the Timer is enabled because
                // this causes the Timer Value register to be updated with an undetermined
                // value."
                if self.enabled {
                    return Err(ContractViolation {
                        msg: "cannot write Load register while timer is enabled".to_string(),
                        severity: log::Level::Error,
                        stub_val: None,
                    });
                }

                let val = val & self.wrapmask;
                self.loadval = Some(val);
                // "The Timer Value register is updated with the Timer Load value as soon as the
                // Timer Load register is written"
                self.val = val;
                Ok(())
            }
            0x04 => Err(InvalidAccess),
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
                        let loadval = match self.loadval {
                            Some(v) => v,
                            None => {
                                return Err(ContractViolation {
                                    msg: "Periodic mode enabled before setting a Load value"
                                        .to_string(),
                                    severity: log::Level::Error,
                                    stub_val: None,
                                })
                            }
                        };

                        let period =
                            Duration::from_nanos((loadval as u64) * 1_000_000 / self.clksel.khz());
                        self.interrupter_tx
                            .send(InterrupterMsg::Enabled {
                                next: Instant::now() + period,
                                period,
                            })
                            .unwrap();
                    }
                }
                if !self.enabled {
                    self.loadval = None;
                    self.interrupter_tx.send(InterrupterMsg::Disabled).unwrap();
                }

                Ok(())
            }
            0x0C => Ok(self.interrupt_bus.send((self.interrupt, false)).unwrap()),
            _ => Err(Unexpected),
        }
    }
}
