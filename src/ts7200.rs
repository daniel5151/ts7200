use std::io::Read;

use arm7tdmi_rs::{reg, Cpu, Memory as ArmMemory};
use log::*;

use crate::devices;
use crate::memory::{MemException, MemExceptionKind, MemResultExt, Memory};

// TODO: improve bootloader initial SP and LR values
pub const HLE_BOOTLOADER_SP: u32 = 0x0100_0000;
pub const HLE_BOOTLOADER_LR: u32 = 0x1234_5678;

#[derive(Debug)]
pub enum FatalError {
    FatalMemException(MemException),
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

        let sections = elf
            .section_headers
            .iter()
            .filter(|h| h.is_alloc() && h.sh_type != goblin::elf::section_header::SHT_NOBITS);
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

        // Redboot sets up the interrupt table such that you can specify the handlers by
        // writing to a function pointer at an offset of +0x20 from the IVT entry. e.g:
        // To handle an SWI (IVT entry 0x08), you write a function pointer to 0x28
        //
        // We emulate this by pre-populating the IVT with a bunch of
        // `ldr pc, [pc, #0x20]` instructions, which results in the expected behavior
        for addr in (0..0x20).step_by(0x04) {
            devices.sdram.w32(addr, 0xe59f_f020).unwrap();
        }

        // TODO: instantiate hardware devices

        // fake the bootloader, load directly at the image address
        let cpu = Cpu::new(&[
            // PC and CPSR are the same for all banks
            (0, reg::PC, elf.entry as u32),
            (0, reg::CPSR, 0xd3), // supervisor mode
            // SP and LR vary across between banks
            // set supervisor mode registers
            (3, reg::LR, HLE_BOOTLOADER_LR),
            (3, reg::SP, HLE_BOOTLOADER_SP),
        ]);

        Ok(Ts7200 { cpu, devices })
    }

    pub fn cycle(&mut self) -> Result<(), FatalError> {
        self.cpu.cycle(&mut self.devices);

        if let Some(ref e) = self.devices.mem_exception {
            use MemExceptionKind::*;
            match e.kind() {
                Unimplemented | Unexpected => return Err(FatalError::FatalMemException(e.clone())),
                Misaligned => {
                    // FIXME: Misaligned access (i.e: Data Abort) should be a CPU exception.
                    return Err(FatalError::FatalMemException(e.clone()));
                }
                // non-fatal exceptions
                StubRead(_) => warn!(
                    "[pc {:#010x?}] stub read from {}",
                    self.cpu.reg_get(0, reg::PC),
                    e.identifier()
                ),
                StubWrite => warn!(
                    "[pc {:#010x?}] stub write to  {}",
                    self.cpu.reg_get(0, reg::PC),
                    e.identifier()
                ),
            }
            self.devices.mem_exception = None;
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
    pub mem_exception: Option<MemException>,
    unmapped: devices::UnmappedMemory,

    pub sdram: devices::Ram, // 32 MB
    pub timer1: devices::Timer,
    pub timer2: devices::Timer,
    pub timer3: devices::Timer,
    pub uart1: devices::Uart,
    pub uart2: devices::Uart,
    pub vic1: devices::Vic,
    pub vic2: devices::Vic,
}

impl Ts7200Devices {
    // TODO: specify uart I/O streams
    fn new() -> Ts7200Devices {
        Ts7200Devices {
            mem_exception: None,
            unmapped: devices::UnmappedMemory,

            sdram: devices::Ram::new(32 * 1024 * 1024), // 32 MB
            timer1: devices::Timer::new("timer1"),
            timer2: devices::Timer::new("timer2"),
            timer3: devices::Timer::new("timer3"),
            uart1: devices::Uart::new("uart1"),
            uart2: devices::Uart::new("uart2"),
            vic1: devices::Vic::new("vic1"),
            vic2: devices::Vic::new("vic2"),
        }
    }

    // TODO: explore other ways of specifying memory map, preferably _without_
    // trait objects (or at the very least, without having to constantly remake
    // the exact same trait object on each call...)
    fn addr_to_mem_offset(&mut self, addr: u32) -> (&mut dyn Memory, u32) {
        match addr {
            0x0000_0000..=0x01ff_ffff => (&mut self.sdram, 0),
            0x800b_0000..=0x800b_ffff => (&mut self.vic1, 0x800b_0000),
            0x800c_0000..=0x800c_ffff => (&mut self.vic2, 0x800c_0000),
            0x8081_0000..=0x8081_001f => (&mut self.timer1, 0x8081_0000),
            0x8081_0020..=0x8081_003f => (&mut self.timer2, 0x8081_0020),
            0x8081_0080..=0x8081_009f => (&mut self.timer3, 0x8081_0080),
            0x808c_0000..=0x808c_ffff => (&mut self.uart1, 0x808c_0000),
            0x808d_0000..=0x808d_ffff => (&mut self.uart2, 0x808d_0000),
            // TODO: add more devices
            _ => (&mut self.unmapped, 0),
        }
    }
}

// Because the cpu expects all memory accesses to "succeed" (i.e: return _some_
// sort of value), there needs to be a shim between the emulator's fallible
// memory interface and the cpu's Memory interface.
//
// These macros implement the memory interface such that if an error occurs, the
// mem_exception Optional is set, which can be checked right after the CPU
// cycle is executed.

macro_rules! impl_arm7tdmi_r {
    ($fn:ident, $ret:ty) => {
        fn $fn(&mut self, addr: u32) -> $ret {
            use crate::memory::AccessKind;

            let (mem, offset) = self.addr_to_mem_offset(addr);
            mem.$fn(addr - offset)
                .map_memerr_offset(offset)
                .or_else(|e| {
                    // catch stub exceptions
                    let ret = match e.kind() {
                        MemExceptionKind::StubRead(v) => Ok(v as $ret),
                        _ => Err(())
                    };
                    self.mem_exception = Some(e.with_access_kind(AccessKind::Read));
                    ret
                })
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
                .or_else(|e| {
                    // catch stub exceptions
                    let ret = match e.kind() {
                        MemExceptionKind::StubWrite => Ok(()),
                        _ => Err(())
                    };
                    self.mem_exception = Some(e.with_access_kind(AccessKind::Write));
                    ret
                })
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
