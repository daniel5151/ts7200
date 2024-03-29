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

/// Common trait implemented by all emulated devices.
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

impl_devfwd!(Box<dyn Device>);
impl_devfwd!(&dyn Device);
impl_devfwd!(&mut dyn Device);

impl<D: Device> Device for Option<D> {
    fn kind(&self) -> &'static str {
        match self {
            Some(dev) => dev.kind(),
            None => "<unmapped>",
        }
    }

    fn label(&self) -> Option<&str> {
        match self {
            Some(dev) => dev.label(),
            None => None,
        }
    }

    fn probe(&self, offset: u32) -> Probe<'_> {
        match self {
            Some(dev) => dev.probe(offset),
            None => Probe::Unmapped,
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
