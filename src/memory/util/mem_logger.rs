use std::ops::{Deref, DerefMut};

use log::info;

use crate::memory::util::MemSniffer;
use crate::memory::{MemResult, Memory};

/// A transparent wrapper around memory objects that logs any reads / writes
#[derive(Debug)]
pub struct MemLogger<M: Memory>(M);

impl<M: Memory> MemLogger<M> {
    #[allow(dead_code)]
    pub fn new(memory: M) -> MemLogger<M> {
        MemLogger(memory)
    }
}

impl<T: Memory> Deref for MemLogger<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T: Memory> DerefMut for MemLogger<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

macro_rules! impl_memlogger_r {
    ($fn:ident, $ret:ty) => {
        fn $fn(&mut self, offset: u32) -> MemResult<$ret> {
            let ident = self.identifier();
            let mut snif = MemSniffer::new(&mut self.0);
            let res = snif.$fn(offset)?;
            info!("[{}] {}", ident, snif.take_last_access().unwrap());
            Ok(res)
        }
    };
}

macro_rules! impl_memlogger_w {
    ($fn:ident, $val:ty) => {
        fn $fn(&mut self, offset: u32, val: $val) -> MemResult<()> {
            let ident = self.identifier();
            let mut snif = MemSniffer::new(&mut self.0);
            snif.$fn(offset, val)?;
            info!("[{}] {}", ident, snif.take_last_access().unwrap());
            Ok(())
        }
    };
}

impl<M: Memory> Memory for MemLogger<M> {
    fn device(&self) -> &'static str {
        self.0.device()
    }

    fn label(&self) -> Option<&str> {
        self.0.label()
    }

    impl_memlogger_r!(r8, u8);
    impl_memlogger_r!(r16, u16);
    impl_memlogger_r!(r32, u32);
    impl_memlogger_w!(w8, u8);
    impl_memlogger_w!(w16, u16);
    impl_memlogger_w!(w32, u32);
}
