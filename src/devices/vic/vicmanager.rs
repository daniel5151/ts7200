use crate::memory::{MemResult, Memory};

use super::{Interrupt, Vic};

/// VIC Manager module.
///
/// Contains the two VIC units from the EP9302, and handles the daisy chaining
/// logic (As described in section 6.1 of the EP93xx user's guide)
#[derive(Debug)]
pub struct VicManager {
    vic1: Vic,
    vic2: Vic,
}

impl VicManager {
    /// Create a new VicManager
    #[allow(clippy::new_without_default)] // Prefer to force explicit creation
    pub fn new() -> Self {
        VicManager {
            vic1: Vic::new("vic1"),
            vic2: Vic::new("vic2"),
        }
    }

    /// Check if an IRQ should be requested
    pub fn fiq(&self) -> bool {
        self.vic1.fiq() || self.vic2.fiq()
    }

    /// Check if an FIQ should be requested
    pub fn irq(&self) -> bool {
        self.vic1.irq() || self.vic2.irq()
    }

    fn bank(&mut self, bank: u8) -> &mut Vic {
        match bank {
            1 => &mut self.vic1,
            2 => &mut self.vic2,
            _ => panic!("Unexpected VIC bank {}", bank),
        }
    }

    /// Request an interrupt from a hardware source
    pub fn assert_interrupt(&mut self, int: Interrupt) {
        self.bank(int.bank()).assert_interrupt(int.index())
    }

    /// Clear an interrupt from a hardware source
    pub fn clear_interrupt(&mut self, int: Interrupt) {
        self.bank(int.bank()).clear_interrupt(int.index())
    }
}
impl Memory for VicManager {
    fn device(&self) -> &'static str {
        "VicManager"
    }

    fn r32(&mut self, offset: u32) -> MemResult<u32> {
        match offset {
            0x30 => {
                // Daisy chain the VICVectAddr register
                if self.vic1.irq() {
                    self.vic1.r32(0x30)
                } else if self.vic2.irq() {
                    self.vic2.r32(0x30)
                } else {
                    // TODO: Result in this case unclear, needs hardware checking
                    self.vic1.r32(0x34) // Read the VIC1DefVectAddr register
                }
            }
            _ => {
                if offset < 0x10000 {
                    self.vic1.r32(offset)
                } else {
                    self.vic2.r32(offset - 0x10000)
                }
            }
        }
    }

    fn w32(&mut self, offset: u32, val: u32) -> MemResult<()> {
        if offset < 0x10000 {
            self.vic1.w32(offset, val)
        } else {
            self.vic2.w32(offset - 0x10000, val)
        }
    }
}
