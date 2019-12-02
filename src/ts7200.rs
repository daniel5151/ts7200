use std::io::Read;

use arm7tdmi_rs::{reg, Cpu, Memory as ArmMemory};
use log::*;

use crate::devices;
use crate::memory::{MemException, MemExceptionKind, MemResult, MemResultExt, Memory};

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
    devices: Ts7200Bus,
}

impl Ts7200 {
    /// Returns a new Ts7200 using High Level Emulation (HLE) of the bootloader.
    /// Execution begins from OS code (as specified in the elf file), and the
    /// system's peripherals are pre-initialized.
    pub fn new_hle(mut fw_file: impl Read) -> std::io::Result<Ts7200> {
        // TODO: avoid reading entire elf file into memory. Use Seek to only load
        // headers we care about.

        // load kernel ELF
        let mut elf_data = Vec::new();
        fw_file.read_to_end(&mut elf_data)?;
        let elf_header = goblin::elf::Elf::parse(&elf_data).map_err(|_e| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, "could not parse elf file")
        })?;

        // load directly into the kernel
        let cpu = Cpu::new(&[
            (0, reg::PC, elf_header.entry as u32),
            (0, reg::CPSR, 0xd3), // supervisor mode
            // SP and LR vary across between banks
            // set supervisor mode registers
            (3, reg::LR, HLE_BOOTLOADER_LR),
            (3, reg::SP, HLE_BOOTLOADER_SP),
        ]);

        // Init system devices
        let mut bus = Ts7200Bus::new();

        // copy all in-memory sections from the ELF file into system RAM
        let sections = elf_header
            .section_headers
            .iter()
            .filter(|h| h.is_alloc() && h.sh_type != goblin::elf::section_header::SHT_NOBITS);
        for h in sections {
            debug!(
                "loading section {:?} into memory from [{:#010x?}..{:#010x?}]",
                elf_header.shdr_strtab.get(h.sh_name).unwrap().unwrap(),
                h.sh_addr,
                h.sh_addr + h.sh_size,
            );

            bus.sdram
                .bulk_write(h.sh_addr as usize, &elf_data[h.file_range()]);
        }

        // Redboot pre-populates up the interrupt vector table with a bunch of
        // `ldr pc, [pc, #0x20]` instructions. This enables easy interrupt service
        // routine registration, simply by writing function pointers at an offset
        // of +0x20 from the corresponding IVT entry.
        // e.g: SWI correspond to IVT entry 0x08, so to register a SWI handler,
        // write a function pointer to 0x28
        for addr in (0..0x20).step_by(0x04) {
            bus.sdram.w32(addr, 0xe59f_f020).unwrap();
        }

        // TODO: instantiate various hardware devices to HLE state

        Ok(Ts7200 { cpu, devices: bus })
    }

    fn handle_mem_exception(&mut self, e: MemException) -> Result<(), FatalError> {
        use MemExceptionKind::*;

        match e.kind() {
            Unimplemented | Unexpected => return Err(FatalError::FatalMemException(e)),
            Misaligned => {
                // FIXME: Misaligned access (i.e: Data Abort) should be a CPU exception.
                return Err(FatalError::FatalMemException(e));
            }
            // non-fatal exceptions
            StubRead(_) => warn!(
                "[pc {:#010x?}] stubed read from {}",
                self.cpu.reg_get(0, reg::PC),
                e.identifier()
            ),
            StubWrite => warn!(
                "[pc {:#010x?}] stubed write to  {}",
                self.cpu.reg_get(0, reg::PC),
                e.identifier()
            ),
        }

        Ok(())
    }

    pub fn cycle(&mut self) -> Result<(), FatalError> {
        let mut adapter = MemoryAdapter::new(&mut self.devices);

        self.cpu.cycle(&mut adapter);

        if let Some(e) = adapter.exception.take() {
            self.handle_mem_exception(e)?;
        }

        Ok(())
    }

    pub fn debug_cycle(&mut self) -> Result<(), FatalError> {
        let mut sniffer = crate::memory::util::MemSniffer::new(&mut self.devices);
        let mut adapter = MemoryAdapter::new(&mut sniffer);

        self.cpu.cycle(&mut adapter);

        let exception = adapter.take_exception();
        let last_access = sniffer.take_last_access();

        if let Some(e) = exception {
            self.handle_mem_exception(e)?;
        }

        // TODO: move debugger code here.

        eprintln!("{}", last_access.unwrap());

        Ok(())
    }

    pub fn cpu(&self) -> &Cpu {
        &self.cpu
    }

    pub fn devices(&self) -> &Ts7200Bus {
        &self.devices
    }

    pub fn devices_mut(&mut self) -> &mut Ts7200Bus {
        &mut self.devices
    }
}

/// The main Ts7200 memory bus.
///
/// This struct is the "top-level" implementation of the [Memory] trait for the
/// Ts7200, and maps the entire 32 bit address space to the Ts7200's various
/// devices.
#[derive(Debug)]
pub struct Ts7200Bus {
    mem_exception: Option<MemException>,
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

impl Ts7200Bus {
    fn new() -> Ts7200Bus {
        Ts7200Bus {
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
}

macro_rules! ts7200_mmap {
    ($($start:literal ..= $end:literal => $device:ident,)*) => {
        macro_rules! impl_ts7200_memory_r {
            ($fn:ident, $ret:ty) => {
                fn $fn(&mut self, addr: u32) -> MemResult<$ret> {
                    match addr {
                        $($start..=$end => self.$device.$fn(addr - $start).mem_ctx($start, self),)*
                        _ => self.unmapped.$fn(addr - 0).mem_ctx(0, self),
                    }
                }
            };
        }

        macro_rules! impl_ts7200_memory_w {
            ($fn:ident, $val:ty) => {
                fn $fn(&mut self, addr: u32, val: $val) -> MemResult<()> {
                    match addr {
                        $($start..=$end => self.$device.$fn(addr - $start, val).mem_ctx($start, self),)*
                        _ => self.unmapped.$fn(addr - 0, val).mem_ctx(0, self),
                    }
                }
            };
        }

        impl Memory for Ts7200Bus {
            fn device(&self) -> &'static str {
                "Ts7200"
            }

            impl_ts7200_memory_r!(r8, u8);
            impl_ts7200_memory_r!(r16, u16);
            impl_ts7200_memory_r!(r32, u32);
            impl_ts7200_memory_w!(w8, u8);
            impl_ts7200_memory_w!(w16, u16);
            impl_ts7200_memory_w!(w32, u32);
        }
    };
}

ts7200_mmap! {
    // TODO: fill out more of the memory map
    0x0000_0000..=0x01ff_ffff => sdram,
    0x800b_0000..=0x800b_ffff => vic1,
    0x800c_0000..=0x800c_ffff => vic2,
    0x8081_0000..=0x8081_001f => timer1,
    0x8081_0020..=0x8081_003f => timer2,
    0x8081_0080..=0x8081_009f => timer3,
    0x808c_0000..=0x808c_ffff => uart1,
    0x808d_0000..=0x808d_ffff => uart2,
}

// The CPU's Memory interface expects all memory accesses to "succeed" (i.e:
// return _some_ sort of value). As such, there needs to be some sort fo shim
// between the emulator's fallible [Memory] interface and the CPU's infallible
// [ArmMemory] interface.

/// [MemoryAdapter] wraps a [Memory] object, implementing the [ArmMemory]
/// interface such that if an error occurs while accessing memory, the access
/// "succeeds," and the exception stored until after the CPU cycle is executed.
/// to trigger an exception accordingly.
struct MemoryAdapter<'a, M: Memory> {
    mem: &'a mut M,
    exception: Option<MemException>,
}

impl<'a, M: Memory> MemoryAdapter<'a, M> {
    pub fn new(mem: &'a mut M) -> Self {
        MemoryAdapter {
            mem,
            exception: None,
        }
    }

    pub fn take_exception(&mut self) -> Option<MemException> {
        self.exception.take()
    }
}

macro_rules! impl_memadapter_r {
    ($fn:ident, $ret:ty) => {
        fn $fn(&mut self, addr: u32) -> $ret {
            use crate::memory::MemAccessKind;
            match self.mem.$fn(addr) {
                Ok(val) => val,
                Err(e) => {
                    self.exception = Some(e.with_access_kind(MemAccessKind::Read));
                    // If it's a stubbed-read, pass through the stub
                    match self.exception.as_ref().unwrap().kind() {
                        MemExceptionKind::StubRead(v) => v as $ret,
                        _ => 0x00 // contents of register undefined
                    }
                }
            }
        }
    };
}

macro_rules! impl_memadapter_w {
    ($fn:ident, $val:ty) => {
        fn $fn(&mut self, addr: u32, val: $val) {
            use crate::memory::MemAccessKind;
            match self.mem.$fn(addr, val) {
                Ok(()) => {}
                Err(e) => {
                    self.exception = Some(e.with_access_kind(MemAccessKind::Write));
                }
            }
        }
    };
}

impl<'a, M: Memory> ArmMemory for MemoryAdapter<'a, M> {
    impl_memadapter_r!(r8, u8);
    impl_memadapter_r!(r16, u16);
    impl_memadapter_r!(r32, u32);
    impl_memadapter_w!(w8, u8);
    impl_memadapter_w!(w16, u16);
    impl_memadapter_w!(w32, u32);
}
