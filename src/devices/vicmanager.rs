use crate::memory::{MemResult, Memory};

use super::Vic;

/// VIC Manager module
///
/// Contains the two VIC units from the EP9302 and handles the daisy chaining
/// logic.
///
/// As described in section 6
/// https://www.student.cs.uwaterloo.ca/~cs452/F19/docs/ep93xx-user-guide.pdf

// FIXME for prilik: unclear if this is the best place for this
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Interrupts {
    Tc1Ui,
    Tc2Ui,
    Uart1RxIntr1,
    Uart1TxIntr1,
    Uart2RxIntr2,
    Uart2TxIntr2,
    Uart3RxIntr3,
    Uart3TxIntr3,
    Tc3Ui,
    IntUart1,
    IntUart2,
    IntUart3,
}

impl Interrupts {
    fn overall_index(&self) -> u8 {
        use Interrupts::*;
        match self {
            Tc1Ui => 4,
            Tc2Ui => 5,
            Uart1RxIntr1 => 23,
            Uart1TxIntr1 => 24,
            Uart2RxIntr2 => 25,
            Uart2TxIntr2 => 26,
            Uart3RxIntr3 => 27,
            Uart3TxIntr3 => 28,
            Tc3Ui => 51,
            IntUart1 => 52,
            IntUart2 => 54,
            IntUart3 => 55,
        }
    }
    fn bank(&self) -> u8 {
        if self.overall_index() < 32 {
            1
        } else {
            2
        }
    }

    fn index(&self) -> u8 {
        self.overall_index() & !0x20
    }
}

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
    pub fn assert_interrupt(&mut self, int: Interrupts) {
        self.bank(int.bank()).assert_interrupt(int.index())
    }

    /// Clear an interrupt from a hardware source
    pub fn clear_interrupt(&mut self, int: Interrupts) {
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
