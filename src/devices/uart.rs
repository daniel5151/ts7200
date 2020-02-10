use crate::io::NonBlockingByteIO;
use crate::memory::{MemResult, MemResultExt, Memory};

/// UART module
///
/// As described in section 14 of the EP93xx User's Guide
pub struct Uart {
    label: &'static str,
    io: Option<Box<dyn NonBlockingByteIO>>,
}

impl std::fmt::Debug for Uart {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Uart").finish()
    }
}

impl Uart {
    /// Create a new uart
    pub fn new_hle(label: &'static str) -> Uart {
        Uart { label, io: None }
    }

    /// Set the UART's io handler
    pub fn set_io(&mut self, io: Option<Box<dyn NonBlockingByteIO>>) {
        self.io = io;
    }
}

impl Memory for Uart {
    fn label(&self) -> Option<&str> {
        Some(self.label)
    }

    fn device(&self) -> &'static str {
        "UART"
    }

    fn r32(&mut self, offset: u32) -> MemResult<u32> {
        match offset {
            // data (8-bit)
            0x00 => match self.io {
                // XXX: properly implement UART DATA read (i.e: respect flags)
                Some(ref mut io) => Ok(io.read() as u32),
                // just return a dummy value?
                None => Ok(0),
            },
            // read status
            0x04 => crate::mem_unimpl!("RSR_REG"),
            // line control high
            0x08 => crate::mem_stub!("LCRH_REG", 0),
            // line control mid
            0x0C => crate::mem_unimpl!("LCRM_REG"),
            // line control low
            0x10 => crate::mem_unimpl!("LCRL_REG"),
            // control
            0x14 => crate::mem_stub!("CTLR_REG", 0),
            // flag
            0x18 => {
                // XXX: properly implement UART DATA read (i.e: respect flags)
                match self.io {
                    Some(ref mut io) => {
                        if io.can_read() {
                            // 0x40 => something to receive
                            Ok(0x40)
                        } else {
                            // 0x10 => Receive fifo empty
                            Ok(0x10)
                        }
                    }
                    None => Ok(0),
                }
            }
            // interrupt identification and clear register
            0x1C => crate::mem_unimpl!("INTR_REG"),
            // dma control
            0x28 => crate::mem_unimpl!("DMAR_REG"),
            _ => crate::mem_unexpected!(),
        }
        .mem_ctx(offset, self)
    }

    fn w32(&mut self, offset: u32, val: u32) -> MemResult<()> {
        match offset {
            // data (8-bit)
            0x00 => match self.io {
                // XXX: properly implement UART DATA write (i.e: respect flags)
                Some(ref mut io) => {
                    io.write(val as u8);
                    Ok(())
                }
                None => Ok(()),
            },
            // read status
            0x04 => crate::mem_unimpl!("RSR_REG"),
            // line control high
            0x08 => crate::mem_stub!("LCRH_REG"),
            // line control mid
            0x0C => crate::mem_stub!("LCRM_REG"),
            // line control low
            0x10 => crate::mem_stub!("LCRL_REG"),
            // control
            0x14 => crate::mem_stub!("CTLR_REG"),
            // flag
            0x18 => crate::mem_unimpl!("FLAG_REG"),
            // interrupt identification and clear register
            0x1C => crate::mem_unimpl!("INTR_REG"),
            // dma control
            0x28 => crate::mem_unimpl!("DMAR_REG"),
            _ => crate::mem_unexpected!(),
        }
        .mem_ctx(offset, self)
    }
}
