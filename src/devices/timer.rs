use crate::memory::{MemResult, MemResultExt, Memory};

/// Timer module
///
/// As described in section 18
/// https://www.student.cs.uwaterloo.ca/~cs452/F19/docs/ep93xx-user-guide.pdf
pub struct Timer {
    label: &'static str,
}

impl std::fmt::Debug for Timer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Timer").finish()
    }
}

impl Timer {
    /// Create a new Timer
    pub fn new(label: &'static str) -> Timer {
        Timer { label }
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
        match offset {
            0x00 => crate::mem_unimpl!("LDR_REG"),
            0x04 => crate::mem_stub!("VAL_REG", 0),
            0x08 => crate::mem_stub!("CTRL_REG", 0),
            0x0C => crate::mem_unimpl!("CLR_REG"),
            _ => crate::mem_unexpected!(),
        }
        .mem_ctx(offset, self)
    }

    fn w32(&mut self, offset: u32, _val: u32) -> MemResult<()> {
        match offset {
            0x00 => crate::mem_stub!("LDR_REG"),
            0x04 => crate::mem_unimpl!("VAL_REG"),
            0x08 => crate::mem_stub!("CRTL_REG"),
            0x0C => crate::mem_unimpl!("CLR_REG"),
            _ => crate::mem_unexpected!(),
        }
        .mem_ctx(offset, self)
    }
}
