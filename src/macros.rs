/// Utility macro to construct a [MemExceptionKind::Unexpected]
#[macro_export]
macro_rules! mem_unexpected {
    () => {
        Err(crate::memory::MemException::new(
            "<unexpected offset>".to_string(),
            0,
            crate::memory::MemExceptionKind::Unexpected,
        ))
    };
}

/// Utility macro to construct a [MemExceptionKind::Unimplemented]
#[macro_export]
macro_rules! mem_unimpl {
    ($friendlyname:expr) => {
        Err(crate::memory::MemException::new(
            $friendlyname.to_string(),
            0,
            crate::memory::MemExceptionKind::Unimplemented,
        ))
    };
}

/// Utility macro to construct a [MemExceptionKind::StubRead] or
/// [MemExceptionKind::StubWrite]
#[macro_export]
macro_rules! mem_stub {
    ($friendlyname:expr, $stubval:expr) => {
        Err(crate::memory::MemException::new(
            $friendlyname.to_string(),
            0,
            crate::memory::MemExceptionKind::StubRead($stubval),
        ))
    };

    ($friendlyname:expr) => {
        Err(crate::memory::MemException::new(
            $friendlyname.to_string(),
            0,
            crate::memory::MemExceptionKind::StubWrite,
        ))
    };
}
