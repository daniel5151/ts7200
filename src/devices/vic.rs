use crate::memory::{MemResult, MemResultExt, Memory};

/// VIC module
///
/// As described in section 6
/// https://www.student.cs.uwaterloo.ca/~cs452/F19/docs/ep93xx-user-guide.pdf
pub struct Vic {
    label: &'static str,
}

impl std::fmt::Debug for Vic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Vic").finish()
    }
}

impl Vic {
    /// Create a new Vic
    pub fn new(label: &'static str) -> Vic {
        Vic { label }
    }
}

impl Memory for Vic {
    fn label(&self) -> Option<&str> {
        Some(self.label)
    }

    fn device(&self) -> &'static str {
        "VIC"
    }

    fn r32(&mut self, offset: u32) -> MemResult<u32> {
        match offset {
            0x00 => crate::mem_unimpl!("STATUS_REG"),
            0x10 => crate::mem_stub!("ENABLE_REG", 0),
            0x14 => crate::mem_unimpl!("CLEAR_REG"),
            _ => crate::mem_unexpected!(),
        }
        .mem_ctx(offset, self)
    }

    fn w32(&mut self, offset: u32, _val: u32) -> MemResult<()> {
        match offset {
            0x00 => crate::mem_unimpl!("STATUS_REG"),
            0x10 => crate::mem_stub!("ENABLE_REG"),
            0x14 => crate::mem_stub!("CLEAR_REG"),
            _ => crate::mem_unexpected!(),
        }
        .mem_ctx(offset, self)
    }
}
