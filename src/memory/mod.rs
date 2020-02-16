pub mod macros;
pub mod util;

mod access;
mod exception;

pub use access::{MemAccess, MemAccessKind, MemAccessVal};
pub use exception::{MemException, MemExceptionKind, MemResult, MemResultExt};

/// Common memory trait used throughout the emulator.
///
/// Default implementations for 8-bit and 16-bit read/write return a
/// [MemException::Misaligned] if the address isn't aligned properly.
pub trait Memory {
    /// The name of the emulated device.
    fn device(&self) -> &'static str;

    /// A descriptive string for a particular instance of the device (if
    /// applicable)
    fn label(&self) -> Option<&str> {
        None
    }

    /// Returns the string "<device>:<label>"
    fn identifier(&self) -> String {
        match self.label() {
            Some(label) => format!("{}:{}", self.device(), label),
            None => self.device().to_string(),
        }
    }

    /// Read a 32 bit value at a given offset
    fn r32(&mut self, offset: u32) -> MemResult<u32>;
    /// Write a 32 bit value to the given offset
    fn w32(&mut self, offset: u32, val: u32) -> MemResult<()>;

    /// Read a 8 bit value at a given offset
    fn r8(&mut self, offset: u32) -> MemResult<u8> {
        if offset & 0x3 != 0 {
            Err(MemException::new(
                self.identifier(),
                offset,
                MemExceptionKind::Misaligned,
            ))
        } else {
            self.r32(offset).map(|v| v as u8)
        }
    }

    /// Read a 16 bit value at a given offset
    fn r16(&mut self, offset: u32) -> MemResult<u16> {
        if offset & 0x3 != 0 {
            Err(MemException::new(
                self.identifier(),
                offset,
                MemExceptionKind::Misaligned,
            ))
        } else {
            self.r32(offset).map(|v| v as u16)
        }
    }

    /// Write a 8 bit value to the given offset
    fn w8(&mut self, offset: u32, val: u8) -> MemResult<()> {
        if offset & 0x3 != 0 {
            Err(MemException::new(
                self.identifier(),
                offset,
                MemExceptionKind::Misaligned,
            ))
        } else {
            self.w32(offset, val as u32)
        }
    }

    /// Write a 16 bit value to the given offset
    fn w16(&mut self, offset: u32, val: u16) -> MemResult<()> {
        if offset & 0x3 != 0 {
            Err(MemException::new(
                self.identifier(),
                offset,
                MemExceptionKind::Misaligned,
            ))
        } else {
            self.w32(offset, val as u32)
        }
    }
}

impl Memory for Box<dyn Memory> {
    fn device(&self) -> &'static str {
        (**self).device()
    }

    fn label(&self) -> Option<&str> {
        (**self).label()
    }

    fn r32(&mut self, offset: u32) -> MemResult<u32> {
        (**self).r32(offset)
    }

    fn w32(&mut self, offset: u32, val: u32) -> MemResult<()> {
        (**self).w32(offset, val)
    }

    fn r8(&mut self, offset: u32) -> MemResult<u8> {
        (**self).r8(offset)
    }

    fn r16(&mut self, offset: u32) -> MemResult<u16> {
        (**self).r16(offset)
    }

    fn w8(&mut self, offset: u32, val: u8) -> MemResult<()> {
        (**self).w8(offset, val)
    }

    fn w16(&mut self, offset: u32, val: u16) -> MemResult<()> {
        (**self).w16(offset, val)
    }
}
