pub mod armv4t_adaptor;
pub mod util;

mod access;
mod exception;

pub use access::{MemAccess, MemAccessKind, MemAccessVal};
pub use exception::{MemException, MemResult};

/// Implemented by all emulated devices.
/// Provides a way to traverse and query the device tree.
pub trait Device {
    /// The name of the emulated device.
    fn kind(&self) -> &'static str;

    /// A descriptive label for a particular instance of the device
    /// (if applicable).
    fn label(&self) -> Option<&str> {
        None
    }

    /// Query what devices exist at a particular memory offset.
    fn probe(&self, offset: u32) -> Probe<'_>;
}

/// Common memory trait used throughout the emulator.
///
/// Default implementations for 8-bit and 16-bit read/write return a
/// [MemException::Misaligned] if the address isn't aligned properly.
pub trait Memory {
    /// Read a 32 bit value at a given offset
    fn r32(&mut self, offset: u32) -> MemResult<u32>;
    /// Write a 32 bit value to the given offset
    fn w32(&mut self, offset: u32, val: u32) -> MemResult<()>;

    /// Read a 8 bit value at a given offset
    fn r8(&mut self, offset: u32) -> MemResult<u8> {
        if offset & 0x3 != 0 {
            Err(MemException::Misaligned)
        } else {
            self.r32(offset).map(|v| v as u8)
        }
    }

    /// Read a 16 bit value at a given offset
    fn r16(&mut self, offset: u32) -> MemResult<u16> {
        if offset & 0x3 != 0 {
            Err(MemException::Misaligned)
        } else {
            self.r32(offset).map(|v| v as u16)
        }
    }

    /// Write a 8 bit value to the given offset
    fn w8(&mut self, offset: u32, val: u8) -> MemResult<()> {
        if offset & 0x3 != 0 {
            Err(MemException::Misaligned)
        } else {
            self.w32(offset, val as u32)
        }
    }

    /// Write a 16 bit value to the given offset
    fn w16(&mut self, offset: u32, val: u16) -> MemResult<()> {
        if offset & 0x3 != 0 {
            Err(MemException::Misaligned)
        } else {
            self.w32(offset, val as u32)
        }
    }
}

/// A link in a chain of devices corresponding to a particular memory offset.
pub enum Probe<'a> {
    /// Branch node representing a device.
    Device {
        device: &'a dyn Device,
        next: Box<Probe<'a>>,
    },
    /// Leaf node representing a register.
    Register(&'a str),
    /// Unmapped memory.
    Unmapped,
}

impl<'a> std::fmt::Display for Probe<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        match self {
            Probe::Device { device, next } => {
                match device.label() {
                    Some(label) => write!(f, "{}:{}", device.kind(), label)?,
                    None => write!(f, "{}", device.kind())?,
                };

                match &**next {
                    Probe::Unmapped => {}
                    next => write!(f, " > {}", next)?,
                }
            }
            Probe::Register(name) => write!(f, "{}", name)?,
            Probe::Unmapped => write!(f, "<unmapped>")?,
        }

        Ok(())
    }
}

macro_rules! impl_memfwd {
    ($type:ty) => {
        impl Memory for $type {
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
    };
}

macro_rules! impl_devfwd {
    ($type:ty) => {
        impl Device for $type {
            fn kind(&self) -> &'static str {
                (**self).kind()
            }

            fn label(&self) -> Option<&str> {
                (**self).label()
            }

            fn probe(&self, offset: u32) -> Probe<'_> {
                (**self).probe(offset)
            }
        }
    };
}

impl_memfwd!(Box<dyn Memory>);
impl_memfwd!(&mut dyn Memory);

impl_devfwd!(Box<dyn Device>);
impl_devfwd!(&dyn Device);
impl_devfwd!(&mut dyn Device);
