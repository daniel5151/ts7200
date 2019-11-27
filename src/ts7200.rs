use std::io::Read;

use arm7tdmi_rs::{reg, Cpu, Memory as ArmMemory};
use log::*;

use crate::devices;
use crate::memory::{AccessViolation, AccessViolationKind, MemResultExt, Memory};

pub const HLE_BOOTLOADER_SP: u32 = 0x0100_0000;
pub const HLE_BOOTLOADER_LR: u32 = 0x1234_5678;

#[derive(Debug)]
pub enum FatalError {
    FatalAccessViolation(AccessViolation),
}

/// A Ts7200 system
#[derive(Debug)]
pub struct Ts7200 {
    cpu: Cpu,
    devices: Ts7200Devices,
}

impl Ts7200 {
    /// Returns a new Ts7200 using High Level Emulation (HLE) of the bootloader.
    /// Execution begins from OS code (as specified in the elf file), and the
    /// system's peripherals are pre-initialized.
    pub fn new_hle(mut fw_file: impl Read) -> std::io::Result<Ts7200> {
        let mut data = Vec::new();
        fw_file.read_to_end(&mut data)?;

        let elf = goblin::elf::Elf::parse(&data).map_err(|_e| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, "could not parse elf file")
        })?;

        // copy all in-memory sections to system RAM
        let mut devices = Ts7200Devices::new();

        let sections = elf.section_headers.iter().filter(|h| h.is_alloc());
        for h in sections {
            debug!(
                "loading section {:?} into memory from [{:#010x?}..{:#010x?}]",
                elf.shdr_strtab.get(h.sh_name).unwrap().unwrap(),
                h.sh_addr,
                h.sh_addr + h.sh_size,
            );

            devices
                .sdram
                .bulk_write(h.sh_addr as usize, &data[h.file_range()]);
        }

        // TODO: initialize hardware devices

        // fake the bootloader, load directly at the image address
        let cpu = Cpu::new(&[
            (3, reg::PC, elf.entry as u32),
            (3, reg::CPSR, 0xd3),
            // TODO: improve bootloader initial SP and LR values
            (3, reg::SP, HLE_BOOTLOADER_SP),
            (3, reg::LR, HLE_BOOTLOADER_LR),
        ]);

        Ok(Ts7200 { cpu, devices })
    }

    pub fn cycle(&mut self) -> Result<(), FatalError> {
        self.cpu.cycle(&mut self.devices);

        if let Some(access_violation) = self.devices.access_violation.take() {
            use AccessViolationKind::*;
            match access_violation.kind() {
                Unimplemented | Unexpected => {
                    return Err(FatalError::FatalAccessViolation(access_violation))
                }
                Misaligned => {
                    log::warn!("CPU {:#010x?}", access_violation);
                    // FIXME: Misaligned access (i.e: Data Abort) _should_ be a CPU exception.
                    return Err(FatalError::FatalAccessViolation(access_violation));
                }
            }
        }

        Ok(())
    }

    pub fn cpu(&self) -> &Cpu {
        &self.cpu
    }

    pub fn devices(&self) -> &Ts7200Devices {
        &self.devices
    }

    pub fn devices_mut(&mut self) -> &mut Ts7200Devices {
        &mut self.devices
    }
}

/// The devices that make up a Ts7200 system. Implements the [ArmMemory] trait
/// with the system's memory map.
#[derive(Debug)]
pub struct Ts7200Devices {
    pub access_violation: Option<AccessViolation>,
    stub: devices::Stub,

    pub sdram: devices::Ram, // 32 MB
    pub uart1: devices::Uart,
    pub uart2: devices::Uart,
}

impl Ts7200Devices {
    // TODO: specify uart I/O streams
    fn new() -> Ts7200Devices {
        Ts7200Devices {
            access_violation: None,
            stub: devices::Stub,

            sdram: devices::Ram::new(32 * 1024 * 1024), // 32 MB
            uart1: devices::Uart::new("uart1"),
            uart2: devices::Uart::new("uart2"),
        }
    }

    // TODO: explore other ways of specifying memory map, preferably _without_
    // trait objects (or at the very least, without having to constantly remake
    // the exact same trait object on each call...)
    fn addr_to_mem_offset(&mut self, addr: u32) -> (&mut dyn Memory, u32) {
        match addr {
            0x0000_0000..=0x01ff_ffff => (&mut self.sdram, 0),
            0x808c_0000..=0x808c_ffff => (&mut self.uart1, 0x808c_0000),
            0x808d_0000..=0x808d_ffff => (&mut self.uart2, 0x808d_0000),
            // TODO: add more devices
            _ => (&mut self.stub, 0),
        }
    }
}

// Because the cpu expects all memory accesses to "succeed" (i.e: return _some_
// sort of value), there needs to be a shim between the emulator's fallible
// memory interface and the cpu's Memory interface.
//
// These macros implement the memory interface such that if an error occurs, the
// access_violation Optional is set, which can be checked right after the CPU
// cycle is executed.

macro_rules! impl_arm7tdmi_r {
    ($fn:ident, $ret:ty) => {
        fn $fn(&mut self, addr: u32) -> $ret {
            use crate::memory::AccessKind;

            let (mem, offset) = self.addr_to_mem_offset(addr);
            mem.$fn(addr - offset)
                .map_memerr_offset(offset)
                .map_err(|e| self.access_violation = Some(e.with_access_kind(AccessKind::Read)))
                .unwrap_or(0x00) // contents of register undefined
        }
    };
}

macro_rules! impl_arm7tdmi_w {
    ($fn:ident, $val:ty) => {
        fn $fn(&mut self, addr: u32, val: $val) {
            use crate::memory::AccessKind;

            let (mem, offset) = self.addr_to_mem_offset(addr);
            mem.$fn(addr - offset, val as $val)
                .map_memerr_offset(offset)
                .map_err(|e| self.access_violation = Some(e.with_access_kind(AccessKind::Write)))
                .unwrap_or(())
        }
    };
}

impl ArmMemory for Ts7200Devices {
    impl_arm7tdmi_r!(r8, u8);
    impl_arm7tdmi_r!(r16, u16);
    impl_arm7tdmi_r!(r32, u32);
    impl_arm7tdmi_w!(w8, u8);
    impl_arm7tdmi_w!(w16, u16);
    impl_arm7tdmi_w!(w32, u32);
}
