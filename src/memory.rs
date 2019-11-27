/// Kinds of Memory exceptions
#[derive(Debug, Copy, Clone)]
pub enum MemExceptionKind {
    /// Attempted to access a device at an invalid offset
    Misaligned,
    /// Memory location that shouldn't have been accessed
    Unexpected,
    /// Memory location hasn't been implemented
    Unimplemented,
    /// Memory location is using a stubbed read implementation
    StubRead(u32),
    /// Memory location is using a stubbed write implementation
    StubWrite,
}

/// Memory Access Kind (Read of Write)
#[derive(Debug, Copy, Clone)]
pub enum AccessKind {
    Read,
    Write,
}

/// Denotes some sort of memory access exception. May be recoverable.
#[derive(Debug, Clone)]
pub struct MemException {
    access_kind: Option<AccessKind>,
    identifier: String,
    addr: u32,
    kind: MemExceptionKind,
}

impl MemException {
    /// Create a new MemException error from a given identifier, offset, and
    /// kind.
    ///
    /// Use the methods in [MemResultExt] to update the error as it propogates
    /// up the device heirarchy.
    pub fn new(identifier: String, offset: u32, kind: MemExceptionKind) -> MemException {
        MemException {
            access_kind: None,
            identifier,
            addr: offset,
            kind,
        }
    }

    /// The address of the access violation
    pub fn addr(&self) -> u32 {
        self.addr
    }

    /// The kind of access violation
    pub fn kind(&self) -> MemExceptionKind {
        self.kind
    }

    /// An identifier designating the full path of the device which returned the
    /// access violation.
    pub fn identifier(&self) -> &str {
        &self.identifier
    }

    /// The access kind of access violation (Read or Write)
    pub fn access_kind(&self) -> Option<AccessKind> {
        self.access_kind
    }

    /// Specify if this was a read or write
    pub fn with_access_kind(mut self, access_kind: AccessKind) -> Self {
        self.access_kind = Some(access_kind);
        self
    }
}

pub type MemResult<T> = Result<T, MemException>;

/// Utility methods to make working with MemResults more ergonomic
pub trait MemResultExt {
    /// If the MemResult is an error, add `offset` to the underlying addr, and
    /// prefix `label` to the address
    fn map_memerr_ctx(self, offset: u32, obj: &impl Memory) -> Self;
    /// If the MemResult is an error, add `offset` to the underlying addr
    fn map_memerr_offset(self, offset: u32) -> Self;
}

impl<T> MemResultExt for MemResult<T> {
    fn map_memerr_offset(self, offset: u32) -> Self {
        self.map_err(|mut violation| {
            violation.addr += offset;
            violation
        })
    }

    fn map_memerr_ctx(self, offset: u32, obj: &impl Memory) -> Self {
        self.map_err(|mut violation| {
            violation.identifier = format!("{} > {}", obj.identifier(), violation.identifier);
            violation.addr += offset;
            violation
        })
    }
}

/// Common memory trait used throughout the emulator.
/// Default implementations for 8-bit and 16-bit read/write is to return a
/// [MemException::Misaligned]
pub trait Memory {
    /// The underlying device type
    fn device(&self) -> &str;

    /// An optional named identifier for the memory region
    fn label(&self) -> Option<&str> {
        None
    }

    /// Returns "<device>:<label>", omitting the ":<label>" if none was given
    fn identifier(&self) -> String {
        format!(
            "{}{}",
            self.device(),
            self.label().map(|s| format!(":{}", s)).unwrap_or_default()
        )
    }

    /// Read a 32 bit value at a given offset
    fn r32(&mut self, offset: u32) -> MemResult<u32>;
    /// Write a 32 bit value to the given offset
    fn w32(&mut self, offset: u32, val: u32) -> MemResult<()>;

    /// Read a 8 bit value at a given offset
    fn r8(&mut self, offset: u32) -> MemResult<u8> {
        if offset & 0x3 != 0 {
            Err(crate::memory::MemException::new(
                self.identifier(),
                offset,
                crate::memory::MemExceptionKind::Misaligned,
            ))
        } else {
            Memory::r32(self, offset).map(|v| v as u8)
        }
    }

    /// Read a 16 bit value at a given offset
    fn r16(&mut self, offset: u32) -> MemResult<u16> {
        if offset & 0x3 != 0 {
            Err(crate::memory::MemException::new(
                self.identifier(),
                offset,
                crate::memory::MemExceptionKind::Misaligned,
            ))
        } else {
            Memory::r32(self, offset).map(|v| v as u16)
        }
    }

    /// Write a 8 bit value to the given offset
    fn w8(&mut self, offset: u32, val: u8) -> MemResult<()> {
        if offset & 0x3 != 0 {
            Err(crate::memory::MemException::new(
                self.identifier(),
                offset,
                crate::memory::MemExceptionKind::Misaligned,
            ))
        } else {
            Memory::w32(self, offset, val as u32)
        }
    }

    /// Write a 16 bit value to the given offset
    fn w16(&mut self, offset: u32, val: u16) -> MemResult<()> {
        if offset & 0x3 != 0 {
            Err(crate::memory::MemException::new(
                self.identifier(),
                offset,
                crate::memory::MemExceptionKind::Misaligned,
            ))
        } else {
            Memory::w32(self, offset, val as u32)
        }
    }
}

impl Memory for Box<dyn Memory> {
    fn device(&self) -> &str {
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
