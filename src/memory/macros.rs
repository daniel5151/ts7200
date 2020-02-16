/// Utility macro to construct a [MemException::Unexpected]
#[macro_export]
macro_rules! mem_unexpected {
    () => {
        Err(crate::memory::MemException::Unexpected)
    };
}

/// Utility macro to construct a [MemException::Unimplemented]
#[macro_export]
macro_rules! mem_unimpl {
    ($loc_name:expr) => {
        Err(crate::memory::MemException::Unimplemented)
    };
}

/// Utility macro to construct a [MemException::StubRead] or
/// [MemException::StubWrite]
#[macro_export]
macro_rules! mem_stub {
    ($loc_name:expr, $stubval:expr) => {
        Err(crate::memory::MemException::StubRead($stubval))
    };

    ($loc_name:expr) => {
        Err(crate::memory::MemException::StubWrite)
    };
}

/// Utility macro to construct a [MemException::InvalidAccess]
#[macro_export]
macro_rules! mem_invalid_access {
    ($loc_name:expr) => {
        Err(crate::memory::MemException::InvalidAccess)
    };
}

#[macro_export]
macro_rules! id_of_subdevice {
    ($device:expr, $offset:expr) => {{
        let device = &$device;
        let offset = $offset;
        Some(match device.id_of(offset) {
            Some(id) => format!("{} > {}", device.id(), id),
            None => device.id(),
        })
    }};
}
