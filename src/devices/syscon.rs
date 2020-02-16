use crate::memory::{MemResult, MemResultExt, Memory};

/// EP9302 Power States (see page 5-10)
#[derive(Debug, Clone, Copy)]
pub enum PowerState {
    Run,
    Halt,
    Standby,
}

/// System Controller module
///
/// As described in section 5 of the EP93xx User's Guide.
#[derive(Debug)]
pub struct Syscon {
    scratch_reg: [u32; 2],
    device_cfg: u32,
    is_locked: bool,
    power_state: PowerState,
}

impl Syscon {
    /// Create a new System Controller
    pub fn new_hle() -> Syscon {
        Syscon {
            scratch_reg: [0, 0],
            // Enabled Bits: GonK CPENA U2EN U1EN HonIDE GonIDE EonIDE
            device_cfg: 0x0894_0d00, // hardware validated
            is_locked: true,
            power_state: PowerState::Run,
        }
    }

    /// Query the current [`PowerState`] of the system.
    pub fn power_state(&self) -> PowerState {
        self.power_state
    }

    /// Set the [`PowerState`] of the system back to Run.
    pub fn set_run_mode(&mut self) {
        self.power_state = PowerState::Run
    }
}

impl Memory for Syscon {
    fn device(&self) -> &'static str {
        "System Controller"
    }

    fn id_of(&self, offset: u32) -> Option<String> {
        let reg = match offset {
            0x00 => "PwrSts",
            0x04 => "PwrCnt",
            0x08 => "Halt",
            0x0C => "Standby",
            0x18 => "TEOI",
            0x1C => "STFClr",
            0x20 => "ClkSet1",
            0x24 => "ClkSet2",
            0x40 => "ScratchReg0",
            0x44 => "ScratchReg1",
            0x50 => "APBWait",
            0x54 => "BusMstrArb",
            0x58 => "BootModeClr",
            0x80 => "DeviceCfg",
            0x84 => "VidClkDiv",
            0x88 => "MIRClkDiv",
            0x8C => "I2SClkDiv",
            0x90 => "KeyTchClkDiv",
            0x94 => "ChipID",
            0x9C => "SysCfg",
            0xC0 => "SysSWLock",
            _ => return None,
        };
        Some(reg.to_string())
    }

    fn r32(&mut self, offset: u32) -> MemResult<u32> {
        match offset {
            0x00 => crate::mem_unimpl!("PwrSts"),
            0x04 => crate::mem_unimpl!("PwrCnt"),
            0x08 => {
                if self.device_cfg & 1 == 1 {
                    self.power_state = PowerState::Halt;
                    Ok(0) // doesn't matter
                } else {
                    // FIXME: emit warning when device contract is violated (instead of panic)
                    panic!("Cannot enter Halt mode if SHena != 1 in syscon DeviceCfg");
                }
            }
            0x0C => {
                if self.device_cfg & 1 == 1 {
                    self.power_state = PowerState::Standby;
                    Ok(0) // doesn't matter
                } else {
                    // FIXME: emit warning when device contract is violated (instead of panic)
                    panic!("Cannot enter Standby mode if SHena != 1 in syscon DeviceCfg");
                }
            }
            0x18 => crate::mem_unimpl!("TEOI"),
            0x1C => crate::mem_unimpl!("STFClr"),
            0x20 => crate::mem_unimpl!("ClkSet1"),
            0x24 => crate::mem_unimpl!("ClkSet2"),
            0x40 => Ok(self.scratch_reg[0]),
            0x44 => Ok(self.scratch_reg[1]),
            0x50 => crate::mem_unimpl!("APBWait"),
            0x54 => crate::mem_unimpl!("BusMstrArb"),
            0x58 => crate::mem_unimpl!("BootModeClr"),
            0x80 => Ok(self.device_cfg),
            0x84 => crate::mem_unimpl!("VidClkDiv"),
            0x88 => crate::mem_unimpl!("MIRClkDiv"),
            0x8C => crate::mem_unimpl!("I2SClkDiv"),
            0x90 => crate::mem_unimpl!("KeyTchClkDiv"),
            0x94 => crate::mem_unimpl!("ChipID"),
            0x9C => crate::mem_unimpl!("SysCfg"),
            0xC0 => {
                if self.is_locked {
                    Ok(0x00)
                } else {
                    Ok(0x01)
                }
            }
            _ => crate::mem_unexpected!(),
        }
        .mem_ctx(offset, self)
    }

    fn w32(&mut self, offset: u32, val: u32) -> MemResult<()> {
        if (0x80..=0x9C).contains(&offset) {
            if self.is_locked {
                // FIXME: emit warning when device contract is violated (instead of panic)
                panic!(
                    "Attempted to writing to SW locked syscon register (offset {:#x?})!",
                    offset
                );
            } else {
                // syscon re-locks after a locked register has been written to
                self.is_locked = true;
            }
        }

        match offset {
            0x00 => crate::mem_unimpl!("PwrSts"),
            0x04 => crate::mem_unimpl!("PwrCnt"),
            0x08 => crate::mem_invalid_access!("Halt"),
            0x0C => crate::mem_invalid_access!("Standby"),
            0x18 => crate::mem_unimpl!("TEOI"),
            0x1C => crate::mem_unimpl!("STFClr"),
            0x20 => crate::mem_unimpl!("ClkSet1"),
            0x24 => crate::mem_unimpl!("ClkSet2"),
            0x40 => Ok(self.scratch_reg[0] = val),
            0x44 => Ok(self.scratch_reg[1] = val),
            0x50 => crate::mem_unimpl!("APBWait"),
            0x54 => crate::mem_unimpl!("BusMstrArb"),
            0x58 => crate::mem_unimpl!("BootModeClr"),
            0x80 => Ok(self.device_cfg = val),
            0x84 => crate::mem_unimpl!("VidClkDiv"),
            0x88 => crate::mem_unimpl!("MIRClkDiv"),
            0x8C => crate::mem_unimpl!("I2SClkDiv"),
            0x90 => crate::mem_unimpl!("KeyTchClkDiv"),
            0x94 => crate::mem_unimpl!("ChipID"),
            0x9C => crate::mem_unimpl!("SysCfg"),
            0xC0 => {
                if val == 0xAA {
                    self.is_locked = false;
                } else {
                    // FIXME: emit warning when device contract is violated (instead of panic)
                    panic!("wrote non-0xAA value to SysSWLock register!");
                }
                Ok(())
            }
            _ => crate::mem_unexpected!(),
        }
        .mem_ctx(offset, self)
    }
}
