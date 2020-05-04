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
