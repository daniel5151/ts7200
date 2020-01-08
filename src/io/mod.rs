mod non_blocking_file;
mod non_blocking_stdio;

pub use non_blocking_file::NonBlockingFile;
pub use non_blocking_stdio::NonBlockingStdio;

/// Incredibly basic trait to read / write bytes
pub trait NonBlockingByteIO {
    /// Check if there is data to be read
    fn can_read(&mut self) -> bool;
    /// Non-blocking read. Return value is undefined if no data is available to
    /// be read.
    fn read(&mut self) -> u8;
    /// Non-blocking write
    fn write(&mut self, val: u8);
}

impl<T: NonBlockingByteIO> NonBlockingByteIO for Box<T> {
    fn can_read(&mut self) -> bool {
        (**self).can_read()
    }
    fn read(&mut self) -> u8 {
        (**self).read()
    }
    fn write(&mut self, val: u8) {
        (**self).write(val)
    }
}
