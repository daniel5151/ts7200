use std::io::Read;

use arm7tdmi_rs::{reg, Cpu, Exception, Memory as ArmMemory};
use log::*;

use crate::devices;
use crate::memory::{
    util::MemSniffer, MemAccessKind, MemAccessVal, MemException, MemExceptionKind, MemResult,
    MemResultExt, Memory,
};

// Values grafted from hardware. May vary a couple of bytes here and there, but
// they're close enough.
pub const HLE_BOOTLOADER_SP: u32 = 0x01fd_cf34;
pub const HLE_BOOTLOADER_LR: u32 = 0x0001_74c8;

#[derive(Debug)]
pub enum FatalError {
    FatalMemException(MemException),
}

/// A Ts7200 system
#[derive(Debug)]
pub struct Ts7200 {
    hle: bool,
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
        debug!("Setting PC to {:#010x?}", elf_header.entry);
        let cpu = Cpu::new(&[
            (0, reg::PC, elf_header.entry as u32),
            (0, reg::CPSR, 0xd3), // supervisor mode
            // SP and LR vary across between banks
            // set supervisor mode registers
            (3, reg::LR, HLE_BOOTLOADER_LR),
            (3, reg::SP, HLE_BOOTLOADER_SP),
        ]);

        // Init system devices
        let mut bus = Ts7200Bus::new_hle();

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
            bus.sdram.w32(addr, 0xe59f_f018).unwrap();
        }

        // TODO: instantiate various hardware devices to HLE state

        Ok(Ts7200 {
            hle: true,
            cpu,
            devices: bus,
        })
    }

    fn handle_mem_exception(cpu: &Cpu, e: MemException) -> Result<(), FatalError> {
        use MemExceptionKind::*;

        match e.kind() {
            Unimplemented | Unexpected => return Err(FatalError::FatalMemException(e)),
            Misaligned => {
                // FIXME: Misaligned access (i.e: Data Abort) should be a CPU exception.
                return Err(FatalError::FatalMemException(e));
            }
            // non-fatal exceptions
            StubRead(_) => warn!(
                "[pc {:#010x?}] stubbed read from {}",
                cpu.reg_get(0, reg::PC),
                e.identifier()
            ),
            StubWrite => warn!(
                "[pc {:#010x?}] stubbed write to  {}",
                cpu.reg_get(0, reg::PC),
                e.identifier()
            ),
        }

        Ok(())
    }

    fn check_exception(&mut self) {
        self.devices
            .timer1
            .check_interrupts(&mut self.devices.vicmgr);
        self.devices
            .timer2
            .check_interrupts(&mut self.devices.vicmgr);
        self.devices
            .timer3
            .check_interrupts(&mut self.devices.vicmgr);

        if self.devices.vicmgr.fiq() {
            self.cpu.exception(Exception::FastInterrupt);
        };
        if self.devices.vicmgr.irq() {
            self.cpu.exception(Exception::Interrupt);
        };
    }

    /// Run the system, returning successfully on" graceful exit".
    ///
    /// In HLE mode, a "graceful exit" is when the PC points into the
    /// bootloader's code.
    pub fn run(&mut self) -> Result<(), FatalError> {
        loop {
            if self.hle {
                let pc = self.cpu.reg_get(0, reg::PC);
                if pc == HLE_BOOTLOADER_LR {
                    info!("Successfully returned to bootloader");
                    info!("Return value: {}", self.cpu.reg_get(0, 0));
                    return Ok(());
                }
            }

            let mut mem = MemoryAdapter::new(&mut self.devices);

            self.cpu.cycle(&mut mem);

            if let Some(e) = mem.exception.take() {
                Ts7200::handle_mem_exception(&self.cpu, e)?;
            }

            self.check_exception();
        }
    }

    pub fn devices_mut(&mut self) -> &mut Ts7200Bus {
        &mut self.devices
    }
}

use gdbstub::{Access as GdbStubAccess, AccessKind as GdbStubAccessKind, Target, TargetState};

impl Target for Ts7200 {
    type Usize = u32;
    type Error = FatalError;

    fn target_description_xml() -> Option<&'static str> {
        Some(r#"<target version="1.0"><architecture>armv4t</architecture></target>"#)
    }

    fn step(
        &mut self,
        mut log_mem_access: impl FnMut(GdbStubAccess<u32>),
    ) -> Result<TargetState, Self::Error> {
        if self.hle {
            let pc = self.cpu.reg_get(0, reg::PC);
            if pc == HLE_BOOTLOADER_LR {
                info!("Successfully returned to bootloader");
                info!("Return value: {}", self.cpu.reg_get(0, 0));
                return Ok(TargetState::Halted);
            }
        }

        let mut sniffer = MemSniffer::new(&mut self.devices, |access| {
            // translate the resulting `MemAccess`s into gdbstub-compatible accesses
            let mut push = |offset, val| {
                log_mem_access(GdbStubAccess {
                    kind: match access.kind {
                        MemAccessKind::Read => GdbStubAccessKind::Read,
                        MemAccessKind::Write => GdbStubAccessKind::Write,
                    },
                    addr: offset,
                    val,
                })
            };

            // transform multi-byte accesses into their constituent single-byte accesses
            match access.val {
                MemAccessVal::U8(val) => push(access.offset, val),
                MemAccessVal::U16(val) => val
                    .to_le_bytes()
                    .iter()
                    .enumerate()
                    .for_each(|(i, b)| push(access.offset + i as u32, *b)),
                MemAccessVal::U32(val) => val
                    .to_le_bytes()
                    .iter()
                    .enumerate()
                    .for_each(|(i, b)| push(access.offset + i as u32, *b)),
            }
        });

        let mut adapter = MemoryAdapter::new(&mut sniffer);

        self.cpu.cycle(&mut adapter);

        if let Some(e) = adapter.take_exception() {
            Ts7200::handle_mem_exception(&self.cpu, e)?;
        }

        self.check_exception();

        Ok(TargetState::Running)
    }

    // order specified in binutils-gdb/blob/master/gdb/features/arm/arm-core.xml
    fn read_registers(&mut self, mut push_reg: impl FnMut(&[u8])) {
        let bank = self.cpu.get_mode().reg_bank();
        for i in 0..13 {
            push_reg(&self.cpu.reg_get(bank, i).to_le_bytes());
        }
        push_reg(&self.cpu.reg_get(bank, reg::SP).to_le_bytes()); // 13
        push_reg(&self.cpu.reg_get(bank, reg::LR).to_le_bytes()); // 14
        push_reg(&self.cpu.reg_get(bank, reg::PC).to_le_bytes()); // 15

        // Floating point registers, unused
        for _ in 0..25 {
            push_reg(&[0, 0, 0, 0]);
        }

        push_reg(&self.cpu.reg_get(bank, reg::CPSR).to_le_bytes());
    }

    fn read_pc(&mut self) -> u32 {
        self.cpu.reg_get(self.cpu.get_mode().reg_bank(), reg::PC)
    }

    fn read_addrs(&mut self, addr: std::ops::Range<u32>, mut push_byte: impl FnMut(u8)) {
        for addr in addr {
            // TODO: handle non-ram accesses bette
            if addr > 0x01ff_ffff {
                push_byte(0xFE);
                continue;
            }

            match self.devices.r8(addr) {
                Ok(val) => push_byte(val),
                Err(e) => {
                    warn!("gdbstub read_addrs memory exception: {:?}", e);
                    panic!("Memory accesses shouldn't throw any errors!")
                }
            };
        }
    }

    fn write_addrs(&mut self, mut get_addr_val: impl FnMut() -> Option<(u32, u8)>) {
        while let Some((addr, val)) = get_addr_val() {
            match self.devices.w8(addr, val) {
                Ok(_) => {}
                Err(e) => warn!("gdbstub write_addrs memory exception: {:?}", e),
            };
        }
    }
}

/// The main Ts7200 memory bus.
///
/// This struct is the "top-level" implementation of the [Memory] trait for the
/// Ts7200, and maps the entire 32 bit address space to the Ts7200's various
/// devices.
#[derive(Debug)]
pub struct Ts7200Bus {
    pub sdram: devices::Ram, // 32 MB
    pub syscon: devices::Syscon,
    pub timer1: devices::Timer,
    pub timer2: devices::Timer,
    pub timer3: devices::Timer,
    pub uart1: devices::Uart,
    pub uart2: devices::Uart,
    pub vicmgr: devices::vic::VicManager,
}

impl Ts7200Bus {
    fn new_hle() -> Ts7200Bus {
        use devices::{vic::Interrupt, *};
        Ts7200Bus {
            sdram: Ram::new(32 * 1024 * 1024), // 32 MB
            syscon: Syscon::new_hle(),
            timer1: Timer::new("timer1", Interrupt::Tc1Ui, 16),
            timer2: Timer::new("timer2", Interrupt::Tc2Ui, 16),
            timer3: Timer::new("timer3", Interrupt::Tc3Ui, 32),
            uart1: Uart::new_hle("uart1"),
            uart2: Uart::new_hle("uart2"),
            vicmgr: vic::VicManager::new(),
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
                        _ => devices::UnmappedMemory.$fn(addr - 0).mem_ctx(0, self),
                    }
                }
            };
        }

        macro_rules! impl_ts7200_memory_w {
            ($fn:ident, $val:ty) => {
                fn $fn(&mut self, addr: u32, val: $val) -> MemResult<()> {
                    match addr {
                        $($start..=$end => self.$device.$fn(addr - $start, val).mem_ctx($start, self),)*
                        _ => devices::UnmappedMemory.$fn(addr - 0, val).mem_ctx(0, self),
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
    0x800b_0000..=0x800c_ffff => vicmgr,
    0x8081_0000..=0x8081_001f => timer1,
    0x8081_0020..=0x8081_003f => timer2,
    0x8081_0080..=0x8081_009f => timer3,
    0x808c_0000..=0x808c_ffff => uart1,
    0x808d_0000..=0x808d_ffff => uart2,
    0x8093_0000..=0x8093_ffff => syscon,
}

// The CPU's Memory interface expects all memory accesses to succeed (i.e:
// return _some_ sort of value). As such, there needs to be some sort of shim
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
