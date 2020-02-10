use crate::memory::{MemResult, MemResultExt, Memory};

/// System Controller module
///
/// As described in section 5 of the EP93xx User's Guide
#[derive(Debug)]
pub struct Syscon {
    device_cfg: u32,
}

impl Syscon {
    /// Create a new System Controller
    // XXX: hardware validate Syscon default values
    pub fn new_hle() -> Syscon {
        Syscon { device_cfg: 0 }
    }
}

#[rustfmt::skip]
const _: () = {
// Address     | Name         | SW Locked | Type | Size | Description
// ------------|--------------|-----------|------|------|-----------------------
// 0x8093_0000 | PwrSts       | No        | R    | 32   | Power/state control state
// 0x8093_0004 | PwrCnt       | No        | R/W  | 32   | Clock/Debug control status
// 0x8093_0008 | Halt         | No        | R    | 32   | Reading this location enters Halt mode.
// 0x8093_000C | Standby      | No        | R    | 32   | Reading this location enters Standby mode.
// 0x8093_0018 | TEOI         | No        | W    | 32   | Write to clear Tick interrupt
// 0x8093_001C | STFClr       | No        | W    | 32   | Write to clear CLDFLG, RSTFLG and WDTFLG.
// 0x8093_0020 | ClkSet1      | No        | R/W  | 32   | Clock speed control 1
// 0x8093_0024 | ClkSet2      | No        | R/W  | 32   | Clock speed control 2
// 0x8093_0040 | ScratchReg0  | No        | R/W  | 32   | Scratch register 0
// 0x8093_0044 | ScratchReg1  | No        | R/W  | 32   | Scratch register 1
// 0x8093_0050 | APBWait      | No        | R/W  | 32   | APB wait
// 0x8093_0054 | BusMstrArb   | No        | R/W  | 32   | Bus Master Arbitration
// 0x8093_0058 | BootModeClr  | No        | W    | 32   | Boot Mode Clear register
// 0x8093_0080 | DeviceCfg    | Yes       | R/W  | 32   | Device configuration
// 0x8093_0084 | VidClkDiv    | Yes       | R/W  | 32   | Video Clock Divider
// 0x8093_0088 | MIRClkDiv    | Yes       | R/W  | 32   | MIR Clock Divider, divides MIR clock for MIR IrDA
// 0x8093_008C | I2SClkDiv    | Yes       | R/W  | 32   | I2S Audio Clock Divider
// 0x8093_0090 | KeyTchClkDiv | Yes       | R/W  | 32   | Keyscan/Touch Clock Divider
// 0x8093_0094 | ChipID       | Yes       | R/W  | 32   | Chip ID Register
// 0x8093_009C | SysCfg       | Yes       | R/W  | 32   | System Configuration
// 0x8093_00A0 | -            | -         | -    | -    | Reserved
// 0x8093_00C0 | SysSWLock    | No        | R/W  | 1    | bit Software Lock Register
};

impl Memory for Syscon {
    fn device(&self) -> &'static str {
        "System Controller"
    }

    fn r32(&mut self, offset: u32) -> MemResult<u32> {
        match offset {
            0x00 => crate::mem_unimpl!("PwrSts"),
            0x04 => crate::mem_unimpl!("PwrCnt"),
            0x08 => crate::mem_unimpl!("Halt"),
            // TODO: actually enter "low power mode" somehow
            0x0C => crate::mem_stub!("Standby"),
            0x18 => crate::mem_unimpl!("TEOI"),
            0x1C => crate::mem_unimpl!("STFClr"),
            0x20 => crate::mem_unimpl!("ClkSet1"),
            0x24 => crate::mem_unimpl!("ClkSet2"),
            0x40 => crate::mem_unimpl!("ScratchReg0"),
            0x44 => crate::mem_unimpl!("ScratchReg1"),
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
            0xC0 => crate::mem_unimpl!("SysSWLock"),
            _ => crate::mem_unexpected!(),
        }
        .mem_ctx(offset, self)
    }

    fn w32(&mut self, offset: u32, val: u32) -> MemResult<()> {
        match offset {
            0x00 => crate::mem_unimpl!("PwrSts"),
            0x04 => crate::mem_unimpl!("PwrCnt"),
            0x08 => crate::mem_unimpl!("Halt"),
            0x0C => {
                // XXX: don't panic if writing to a read-only register
                panic!("tried to write value to read-only syscon register!");
            }
            0x18 => crate::mem_unimpl!("TEOI"),
            0x1C => crate::mem_unimpl!("STFClr"),
            0x20 => crate::mem_unimpl!("ClkSet1"),
            0x24 => crate::mem_unimpl!("ClkSet2"),
            0x40 => crate::mem_unimpl!("ScratchReg0"),
            0x44 => crate::mem_unimpl!("ScratchReg1"),
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
            0xC0 => crate::mem_unimpl!("SysSWLock"),
            _ => crate::mem_unexpected!(),
        }
        .mem_ctx(offset, self)
    }
}
