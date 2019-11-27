use std::io::{Read, Write};

use crate::memory::{MemResult, MemResultExt, Memory};

/// UART module
///
/// As described in section 14
/// https://www.student.cs.uwaterloo.ca/~cs452/F19/docs/ep93xx-user-guide.pdf
pub struct Uart {
    label: &'static str,
    reader: Option<Box<dyn Read>>,
    writer: Option<Box<dyn Write>>,
}

impl std::fmt::Debug for Uart {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Uart").finish()
    }
}

impl Uart {
    /// Create a new uart
    pub fn new(label: &'static str) -> Uart {
        Uart {
            label,
            reader: None,
            writer: None,
        }
    }

    /// Set the UART's reader
    pub fn set_reader(&mut self, reader: Option<Box<dyn Read>>) {
        self.reader = reader;
    }

    /// Set the UART's writer
    pub fn set_writer(&mut self, writer: Option<Box<dyn Write>>) {
        self.writer = writer;
    }
}

impl Memory for Uart {
    fn label(&self) -> Option<&str> {
        Some(self.label)
    }

    fn device(&self) -> &str {
        "uart"
    }

    fn r32(&mut self, offset: u32) -> MemResult<u32> {
        match offset {
            // data (8-bit)
            0x00 => match self.reader {
                // XXX: properly implement UART DATA read (i.e: respect flags)
                Some(ref mut reader) => {
                    let mut c = [0; 1];
                    reader.read_exact(&mut c).expect("uart read error");
                    Ok(c[0] as u32)
                }
                // return a dummy value?
                None => Ok(0),
            },
            // read status
            0x04 => crate::unimplemented_offset!(offset, "RSR_REG"),
            // line control high
            0x08 => {
                // XXX: properly implement UART LCRH read
                Ok(0)
            }
            // line control mid
            0x0C => crate::unimplemented_offset!(offset, "LCRM_REG"),
            // line control low
            0x10 => crate::unimplemented_offset!(offset, "LCRL_REF"),
            // control
            0x14 => crate::unimplemented_offset!(offset, "CTLR_REG"),
            // flag
            0x18 => {
                // XXX: properly implement UART FLAG_REG read
                // set flag as through there's always something to receive
                Ok(0x40)
            }
            // interrupt identification and clear register
            0x1C => crate::unimplemented_offset!(offset, "INTR_REG"),
            // dma control
            0x28 => crate::unimplemented_offset!(offset, "DMAR_REG"),
            _ => crate::unexpected_offset!(offset),
        }
        .map_memerr_ctx(offset, self)
    }

    fn w32(&mut self, offset: u32, val: u32) -> MemResult<()> {
        match offset {
            // data (8-bit)
            0x00 => match self.writer {
                // XXX: properly implement UART DATA write (i.e: respect flags)
                Some(ref mut writer) => {
                    writer.write_all(&[val as u8]).expect("uart write error");
                    Ok(())
                }
                None => Ok(()),
            },
            // read status
            0x04 => crate::unimplemented_offset!(offset, "RSR_REG"),
            // line control high
            0x08 => {
                // XXX: properly implement UART LCRH_REG write
                Ok(())
            }
            // line control mid
            0x0C => crate::unimplemented_offset!(offset, "LCRM_REG"),
            // line control low
            0x10 => crate::unimplemented_offset!(offset, "LCRL_REF"),
            // control
            0x14 => crate::unimplemented_offset!(offset, "CTLR_REG"),
            // flag
            0x18 => crate::unimplemented_offset!(offset, "FLAG_REG"),
            // interrupt identification and clear register
            0x1C => crate::unimplemented_offset!(offset, "INTR_REG"),
            // dma control
            0x28 => crate::unimplemented_offset!(offset, "DMAR_REG"),
            _ => crate::unexpected_offset!(offset),
        }
        .map_memerr_ctx(offset, self)
    }
}
