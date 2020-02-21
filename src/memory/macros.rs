/// Utility macro to easily constuct an `Option<String>` corresponding to the
/// id_of a subdevice
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
