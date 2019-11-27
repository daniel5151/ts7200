use std::ops::{Deref, DerefMut};

use log::info;

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

impl<M: Memory> Memory for MemLogger<M> {
    fn device(&self) -> &str {
        self.0.device()
    }

    fn label(&self) -> Option<&str> {
        self.0.label()
    }

    fn r8(&mut self, offset: u32) -> MemResult<u8> {
        let res = self.0.r8(offset)?;
        info!(
            "[{}] r8({:#010x?}) -> 0x{:02x}",
            self.identifier(),
            offset,
            res,
        );
        Ok(res)
    }

    fn r16(&mut self, offset: u32) -> MemResult<u16> {
        let res = self.0.r16(offset)?;
        info!(
            "[{}] r16({:#010x?}) -> 0x{:04x}",
            self.identifier(),
            offset,
            res,
        );
        Ok(res)
    }

    fn r32(&mut self, offset: u32) -> MemResult<u32> {
        let res = self.0.r32(offset)?;
        info!(
            "[{}] r32({:#010x?}) -> 0x{:08x}",
            self.identifier(),
            offset,
            res,
        );
        Ok(res)
    }

    fn w8(&mut self, offset: u32, val: u8) -> MemResult<()> {
        self.0.w8(offset, val)?;
        info!(
            "[{}] w8({:#010x?}, {:#04x?})",
            self.identifier(),
            offset,
            val
        );
        Ok(())
    }

    fn w16(&mut self, offset: u32, val: u16) -> MemResult<()> {
        self.0.w16(offset, val)?;
        info!(
            "[{}] w16({:#010x?}, {:#06x?})",
            self.identifier(),
            offset,
            val
        );
        Ok(())
    }

    fn w32(&mut self, offset: u32, val: u32) -> MemResult<()> {
        self.0.w32(offset, val)?;
        info!(
            "[{}] w32({:#010x?}, {:#010x?})",
            self.identifier(),
            offset,
            val
        );
        Ok(())
    }
}
