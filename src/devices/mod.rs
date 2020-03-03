#![allow(
    clippy::unit_arg,  // Substantially reduces boilerplate
    clippy::match_bool // can make things more clear at times
)]

pub mod ram;
pub mod syscon;
pub mod timer;
pub mod uart;
pub mod vic;

pub use ram::Ram;
pub use syscon::Syscon;
pub use timer::Timer;
pub use uart::Uart;

use crate::memory::{MemException, MemResult, Memory};

/// A "device" which returns an MemError::Unexpected when accessed
#[derive(Debug)]
pub struct UnmappedMemory;

impl Memory for UnmappedMemory {
    fn device(&self) -> &'static str {
        "<unmapped memory>"
    }

    fn id_of(&self, _offset: u32) -> Option<String> {
        None
    }

    fn r32(&mut self, _offset: u32) -> MemResult<u32> {
        Err(MemException::Unexpected)
    }
    fn w32(&mut self, _offset: u32, _val: u32) -> MemResult<()> {
        Err(MemException::Unexpected)
    }
}
