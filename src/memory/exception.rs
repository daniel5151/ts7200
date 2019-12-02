pub use super::{MemAccess, MemAccessKind, MemAccessVal, Memory};

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

/// Denotes some sort of memory access exception. May be recoverable.
#[derive(Debug, Clone)]
pub struct MemException {
    // `access_kind` is an Option so leaf memory devices aren't required to explicity specify
    // specify the access kind when returning the exception. Instead, `access_kind` is set by the
    // root Memory function using the `with_access_kind` method.
    access_kind: Option<MemAccessKind>,
    identifier: String,
    offset: u32,
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
            offset,
            kind,
        }
    }

    /// The offset of the access violation
    pub fn offset(&self) -> u32 {
        self.offset
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
    pub fn access_kind(&self) -> Option<MemAccessKind> {
        self.access_kind
    }

    /// Consume self, returning a new version with an explicit read or write
    pub fn with_access_kind(mut self, access_kind: MemAccessKind) -> Self {
        self.access_kind = Some(access_kind);
        self
    }
}

pub type MemResult<T> = Result<T, MemException>;

/// Utility methods to make working with MemResults more ergonomic
pub trait MemResultExt {
    /// If the MemResult is an error, add `base_offset` to the underlying
    /// offset, and update the identifier string
    fn mem_ctx(self, base_offset: u32, obj: &impl Memory) -> Self;
}

impl<T> MemResultExt for MemResult<T> {
    fn mem_ctx(self, base_offset: u32, obj: &impl Memory) -> Self {
        self.map_err(|mut exception| {
            exception.identifier = format!("{} > {}", obj.identifier(), exception.identifier);
            exception.offset += base_offset;
            exception
        })
    }
}
