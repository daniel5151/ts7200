#![allow(clippy::unit_arg)] // Substantially reduces boilerplate

pub mod ram;
pub mod syscon;
pub mod timer;
pub mod uart;
pub mod vic;

pub use ram::Ram;
pub use syscon::Syscon;
pub use timer::Timer;
pub use uart::Uart;

use crate::memory::{MemResult, Memory};

/// A "device" which returns an MemError::Unexpected when accessed
#[derive(Debug)]
pub struct UnmappedMemory;

impl Memory for UnmappedMemory {
    fn device(&self) -> &'static str {
        "<unmapped memory>"
    }

    fn r32(&mut self, offset: u32) -> MemResult<u32> {
        Err(crate::memory::MemException::new(
            self.identifier(),
            offset,
            crate::memory::MemExceptionKind::Unexpected,
        ))
    }
    fn w32(&mut self, offset: u32, _: u32) -> MemResult<()> {
        Err(crate::memory::MemException::new(
            self.identifier(),
            offset,
            crate::memory::MemExceptionKind::Unexpected,
        ))
    }
}
