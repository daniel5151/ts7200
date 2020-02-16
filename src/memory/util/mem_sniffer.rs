use crate::memory::{MemAccess, MemResult, Memory};

/// [MemSniffer] wraps a [Memory] object, forwarding requests to the underlying
/// memory object, while also recording accesses into the provided buffer.
///
/// Panics if the provided buffer overflows.
#[derive(Debug)]
pub struct MemSniffer<'a, M: Memory, F: FnMut(MemAccess)> {
    mem: &'a mut M,
    on_access: F,
}

impl<'a, M: Memory, F: FnMut(MemAccess)> MemSniffer<'a, M, F> {
    pub fn new(mem: &'a mut M, on_access: F) -> MemSniffer<'a, M, F> {
        MemSniffer { mem, on_access }
    }
}

macro_rules! impl_memsniff_r {
    ($fn:ident, $ret:ty) => {
        fn $fn(&mut self, addr: u32) -> MemResult<$ret> {
            let ret = self.mem.$fn(addr)?;
            (self.on_access)(MemAccess::$fn(addr, ret));
            Ok(ret)
        }
    };
}

macro_rules! impl_memsniff_w {
    ($fn:ident, $val:ty) => {
        fn $fn(&mut self, addr: u32, val: $val) -> MemResult<()> {
            self.mem.$fn(addr, val)?;
            (self.on_access)(MemAccess::$fn(addr, val));
            Ok(())
        }
    };
}

impl<'a, M: Memory, F: FnMut(MemAccess)> Memory for MemSniffer<'a, M, F> {
    fn device(&self) -> &'static str {
        self.mem.device()
    }

    fn label(&self) -> Option<&str> {
        self.mem.label()
    }

    fn id_of(&self, offset: u32) -> Option<String> {
        self.mem.id_of(offset)
    }

    impl_memsniff_r!(r8, u8);
    impl_memsniff_r!(r16, u16);
    impl_memsniff_r!(r32, u32);
    impl_memsniff_w!(w8, u8);
    impl_memsniff_w!(w16, u16);
    impl_memsniff_w!(w32, u32);
}
