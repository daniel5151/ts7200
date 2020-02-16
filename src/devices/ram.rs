use std::vec::Vec;

use byteorder::{ByteOrder, LittleEndian};

use crate::memory::{MemException::*, MemResult, Memory};

/// Basic fixed-size RAM module.
pub struct Ram {
    mem: Vec<u8>,
    initialized: Vec<bool>,
}

impl std::fmt::Debug for Ram {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Ram").field("mem", &"<omitted>").finish()
    }
}

impl Ram {
    /// size in bytes
    pub fn new(size: usize) -> Ram {
        Ram {
            mem: vec![b'-'; size], // non-zero value to make it easier to spot bugs
            initialized: vec![false; size],
        }
    }

    pub fn new_with_data(size: usize, data: &[u8]) -> Ram {
        let mut ram = Ram::new(size);
        ram.bulk_write(0, data);
        ram
    }

    pub fn bulk_write(&mut self, offset: usize, data: &[u8]) {
        self.mem[offset..offset + data.len()].copy_from_slice(data);
        self.initialized[offset..offset + data.len()]
            .iter_mut()
            .for_each(|b| *b = true);
    }

    fn addr_as_str(&self, offset: usize, size: usize) -> String {
        let s = self.initialized[offset..offset + size]
            .iter()
            .zip(self.mem[offset..offset + size].iter())
            .map(|(init, val)| {
                if *init {
                    format!("{:02x?}", val)
                } else {
                    "??".to_string()
                }
            })
            .collect::<String>();
        format!("0x{}", s)
    }
}

impl Memory for Ram {
    fn device(&self) -> &'static str {
        "Ram"
    }

    fn id_of(&self, _offset: u32) -> Option<String> {
        None
    }

    fn r8(&mut self, offset: u32) -> MemResult<u8> {
        let offset = offset as usize;
        let val = self.mem[offset];

        if !self.initialized[offset] {
            return Err(ContractViolation {
                msg: format!(
                    "r8 from (partially) uninitialized RAM: {}",
                    self.addr_as_str(offset, 1)
                ),
                stub_val: Some(val as u32),
            });
        }
        Ok(val)
    }

    fn r16(&mut self, offset: u32) -> MemResult<u16> {
        let offset = offset as usize;
        let val = LittleEndian::read_u16(&self.mem[offset..offset + 2]);
        if self.initialized[offset..offset + 2] != [true; 2] {
            return Err(ContractViolation {
                msg: format!(
                    "r16 from (partially) uninitialized RAM: {}",
                    self.addr_as_str(offset, 2)
                ),
                stub_val: Some(val as u32),
            });
        }
        Ok(val)
    }

    fn r32(&mut self, offset: u32) -> MemResult<u32> {
        let offset = offset as usize;
        let val = LittleEndian::read_u32(&self.mem[offset..offset + 4]);
        if self.initialized[offset..offset + 4] != [true; 4] {
            return Err(ContractViolation {
                msg: format!(
                    "r32 from (partially) uninitialized RAM: {}",
                    self.addr_as_str(offset, 4)
                ),
                stub_val: Some(val as u32),
            });
        }
        Ok(val)
    }

    fn w8(&mut self, offset: u32, val: u8) -> MemResult<()> {
        let offset = offset as usize;
        self.initialized[offset] = true;
        self.mem[offset] = val;
        Ok(())
    }

    fn w16(&mut self, offset: u32, val: u16) -> MemResult<()> {
        let offset = offset as usize;
        self.initialized[offset..offset + 2].copy_from_slice(&[true; 2]);
        LittleEndian::write_u16(&mut self.mem[offset..offset + 2], val);
        Ok(())
    }

    fn w32(&mut self, offset: u32, val: u32) -> MemResult<()> {
        let offset = offset as usize;
        self.initialized[offset..offset + 4].copy_from_slice(&[true; 4]);
        LittleEndian::write_u32(&mut self.mem[offset..offset + 4], val);
        Ok(())
    }
}
