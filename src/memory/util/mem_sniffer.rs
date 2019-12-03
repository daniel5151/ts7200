use crate::memory::{MemAccess, MemResult, Memory};

/// [MemSniffer] wraps a [Memory] object, forwarding requests to the underlying
/// memory object, while also recording accesses into the provided buffer.
///
/// Panics if the provided buffer overflows.
#[derive(Debug)]
pub struct MemSniffer<'a, M: Memory> {
    mem: &'a mut M,
    accesses: &'a mut [Option<MemAccess>],
    i: usize,
}

impl<'a, M: Memory> MemSniffer<'a, M> {
    pub fn new(mem: &'a mut M, accesses: &'a mut [Option<MemAccess>]) -> MemSniffer<'a, M> {
        MemSniffer {
            mem,
            accesses,
            i: 0,
        }
    }
}

macro_rules! impl_memsniff_r {
    ($fn:ident, $ret:ty) => {
        fn $fn(&mut self, addr: u32) -> MemResult<$ret> {
            let ret = self.mem.$fn(addr)?;
            self.accesses[self.i] = Some(MemAccess::$fn(addr, ret));
            self.i += 1;
            Ok(ret)
        }
    };
}

macro_rules! impl_memsniff_w {
    ($fn:ident, $val:ty) => {
        fn $fn(&mut self, addr: u32, val: $val) -> MemResult<()> {
            self.mem.$fn(addr, val)?;
            self.accesses[self.i] = Some(MemAccess::$fn(addr, val));
            self.i += 1;
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
