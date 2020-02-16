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
    ($loc_name:expr) => {
        Err(crate::memory::MemException::new(
            $loc_name.to_string(),
            0,
            crate::memory::MemExceptionKind::Unimplemented,
        ))
    };
}

/// Utility macro to construct a [MemExceptionKind::StubRead] or
/// [MemExceptionKind::StubWrite]
#[macro_export]
macro_rules! mem_stub {
    ($loc_name:expr, $stubval:expr) => {
        Err(crate::memory::MemException::new(
            $loc_name.to_string(),
            0,
            crate::memory::MemExceptionKind::StubRead($stubval),
        ))
    };

    ($loc_name:expr) => {
        Err(crate::memory::MemException::new(
            $loc_name.to_string(),
            0,
            crate::memory::MemExceptionKind::StubWrite,
        ))
    };
}

/// Utility macro to construct a [MemExceptionKind::InvalidAccess]
#[macro_export]
macro_rules! mem_invalid_access {
    ($loc_name:expr) => {
        Err(crate::memory::MemException::new(
            $loc_name.to_string(),
            0,
            crate::memory::MemExceptionKind::InvalidAccess,
        ))
    };
}
