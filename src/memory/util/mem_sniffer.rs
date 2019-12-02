use crate::memory::{MemAccess, MemResult, Memory};

/// [MemSniffer] wraps a [Memory] object, recording any memory accesses
#[derive(Debug)]
pub struct MemSniffer<'a, M: Memory> {
    mem: &'a mut M,
    last_access: Option<MemAccess>,
}

impl<'a, M: Memory> MemSniffer<'a, M> {
    pub fn new(mem: &'a mut M) -> MemSniffer<'a, M> {
        MemSniffer {
            mem,
            last_access: None,
        }
    }

    pub fn take_last_access(&mut self) -> Option<MemAccess> {
        self.last_access.take()
    }
}

macro_rules! impl_memsniff_r {
    ($fn:ident, $ret:ty) => {
        fn $fn(&mut self, addr: u32) -> MemResult<$ret> {
            let ret = self.mem.$fn(addr)?;
            self.last_access = Some(MemAccess::$fn(addr, ret));
            Ok(ret)
        }
    };
}

macro_rules! impl_memsniff_w {
    ($fn:ident, $val:ty) => {
        fn $fn(&mut self, addr: u32, val: $val) -> MemResult<()> {
            self.mem.$fn(addr, val)?;
            self.last_access = Some(MemAccess::$fn(addr, val));
            Ok(())
        }
    };
}

impl<'a, M: Memory> Memory for MemSniffer<'a, M> {
    fn device(&self) -> &'static str {
        self.mem.device()
    }

    fn label(&self) -> Option<&str> {
        self.mem.label()
    }

    impl_memsniff_r!(r8, u8);
    impl_memsniff_r!(r16, u16);
    impl_memsniff_r!(r32, u32);
    impl_memsniff_w!(w8, u8);
    impl_memsniff_w!(w16, u16);
    impl_memsniff_w!(w32, u32);
}
