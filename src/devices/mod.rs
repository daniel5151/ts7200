pub mod ram;
pub mod uart;

pub use ram::Ram;
pub use uart::Uart;

use crate::memory::{MemResult, Memory};

/// A device which returns an AccessViolation::Unimplemented when accessed
#[derive(Debug)]
pub struct Stub;

impl Memory for Stub {
    fn device(&self) -> &str {
        "<unmapped memory>"
    }

    fn r32(&mut self, offset: u32) -> MemResult<u32> {
        crate::unexpected_offset!(offset)
    }
    fn w32(&mut self, offset: u32, _: u32) -> MemResult<()> {
        crate::unexpected_offset!(offset)
    }
}
