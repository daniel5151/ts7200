use std::io::Read;

use armv4t_emu::{reg, Cpu, Exception, Mode as ArmMode};
use crossbeam_channel as chan;
use log::*;

use crate::devices;
use crate::devices::vic::Interrupt;
use crate::devices::{Device, Probe};
use crate::memory::{
    armv4t_adaptor::{MemoryAdapter, MemoryAdapterException},
    MemAccessKind, MemException, MemResult, Memory,
};
use crate::util::MemSniffer;

mod gdb;

// Values grafted from hardware. May vary a couple of bytes here and there, but
// they're close enough.
pub const HLE_BOOTLOADER_SP: u32 = 0x01fd_cf34;
pub const HLE_BOOTLOADER_LR: u32 = 0x0001_74c8;

#[derive(Debug)]
pub enum FatalError {
    FatalMemException {
        addr: u32,
        in_mem_space_of: String,
        reason: MemException,
    },
    ContractViolation {
        in_mem_space_of: String,
        msg: String,
    },
    UnimplementedPowerState(devices::syscon::PowerState),
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Event {
    Halted,
    Break,
    WatchWrite(u32),
    WatchRead(u32),
}

pub enum BlockMode {
    Blocking,
    NonBlocking,
}

/// A Ts7200 system
#[derive(Debug)]
pub struct Ts7200 {
    hle: bool,
    frozen: bool,

    cpu: Cpu,
    devices: Ts7200Bus,
    interrupt_bus: chan::Receiver<(Interrupt, bool)>,

    watchpoints: Vec<u32>,
    breakpoints: Vec<u32>,
}

impl Ts7200 {
    /// Returns a new Ts7200 using High Level Emulation (HLE) of the bootloader.
    /// Execution begins from OS code (as specified in the elf file), and the
    /// system's peripherals are pre-initialized.
    pub fn new_hle(mut fw_file: impl Read) -> std::io::Result<Ts7200> {
        // TODO?: use seek instead of reading entire elf file into memory.

        // load kernel ELF
        let mut elf_data = Vec::new();
        fw_file.read_to_end(&mut elf_data)?;
        let elf_header = goblin::elf::Elf::parse(&elf_data).map_err(|_e| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, "could not parse elf file")
        })?;

        // load directly into the kernel
        debug!("Setting PC to {:#010x?}", elf_header.entry);
        let mut cpu = Cpu::new();
        cpu.reg_set(ArmMode::User, reg::PC, elf_header.entry as u32);
        cpu.reg_set(ArmMode::User, reg::CPSR, 0xd3); // supervisor mode
        cpu.reg_set(ArmMode::Supervisor, reg::LR, HLE_BOOTLOADER_LR);
        cpu.reg_set(ArmMode::Supervisor, reg::SP, HLE_BOOTLOADER_SP);

        // create the interrupt bus
        let (interrupt_bus_tx, interrupt_bus_rx) = chan::unbounded();

        // initialize system devices (in HLE state)
        let mut bus = Ts7200Bus::new_hle(interrupt_bus_tx);

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

        Ok(Ts7200 {
            hle: true,
            frozen: false,

            cpu,
            devices: bus,
            interrupt_bus: interrupt_bus_rx,

            watchpoints: Vec::new(),
            breakpoints: Vec::new(),
        })
    }

    fn handle_mem_exception(
        cpu: &Cpu,
        mem: &impl Device,
        exception: MemoryAdapterException,
    ) -> Result<(), FatalError> {
        let MemoryAdapterException {
            addr,
            kind,
            mem_except,
        } = exception;

        let pc = cpu.reg_get(ArmMode::User, reg::PC);
        let in_mem_space_of = format!("{}", mem.probe(addr));

        let ctx = format!(
            "[pc {:#010x?}][addr {:#010x?}][{}]",
            pc, addr, in_mem_space_of
        );

        use MemException::*;
        match mem_except {
            Unimplemented | Unexpected => {
                return Err(FatalError::FatalMemException {
                    addr,
                    in_mem_space_of,
                    reason: mem_except,
                })
            }
            StubRead(_) => warn!("{} stubbed read", ctx),
            StubWrite => warn!("{} stubbed write", ctx),
            Misaligned => {
                // FIXME: Misaligned access (i.e: Data Abort) should be a CPU exception.
                return Err(FatalError::FatalMemException {
                    addr,
                    in_mem_space_of,
                    reason: mem_except,
                });
            }
            InvalidAccess => match kind {
                MemAccessKind::Read => error!("{} read from write-only register", ctx),
                MemAccessKind::Write => error!("{} write to read-only register", ctx),
            },
            ContractViolation { msg, severity, .. } => {
                if severity == log::Level::Error {
                    return Err(FatalError::ContractViolation {
                        in_mem_space_of,
                        msg,
                    });
                } else {
                    log!(severity, "{} {}", ctx, msg)
                }
            }
        }

        Ok(())
    }

    fn check_device_interrupts(&mut self, blocking: BlockMode) {
        macro_rules! check_device_interrupts {
            ($iter:expr) => {{
                for (interrupt, state) in $iter {
                    if state {
                        self.devices.vicmgr.assert_interrupt(interrupt)
                    } else {
                        self.devices.vicmgr.clear_interrupt(interrupt)
                    }
                }
            }};
        }

        match blocking {
            BlockMode::NonBlocking => check_device_interrupts!(self.interrupt_bus.try_iter()),
            BlockMode::Blocking => {
                check_device_interrupts!(std::iter::once(self.interrupt_bus.recv().unwrap())
                    .chain(self.interrupt_bus.try_iter()))
            }
        };

        if self.devices.vicmgr.fiq() {
            self.cpu.exception(Exception::FastInterrupt);
        };
        if self.devices.vicmgr.irq() {
            self.cpu.exception(Exception::Interrupt);
        };
    }

    /// Run the system for a single CPU instruction, returning `true` if the
    /// system is still running, or `false` upon exiting to the bootloader.
    pub fn step(&mut self, halt_block_mode: BlockMode) -> Result<Option<Event>, FatalError> {
        if self.frozen {
            return Ok(None);
        }

        use crate::devices::syscon::PowerState;
        match self.devices.syscon.power_state() {
            PowerState::Run => {
                // set up memory sniffer to support watchpoints
                let mut hit_watchpoint = None;
                let mut sniffer = MemSniffer::new(&mut self.devices, &self.watchpoints, |access| {
                    hit_watchpoint = Some(access)
                });

                // step the system
                let mut mem = MemoryAdapter::new(&mut sniffer);
                self.cpu.step(&mut mem);
                if let Some(e) = mem.take_exception() {
                    Ts7200::handle_mem_exception(&self.cpu, &self.devices, e)?;
                }
                self.check_device_interrupts(BlockMode::NonBlocking);

                let pc = self.cpu.reg_get(ArmMode::User, reg::PC);

                // check for HLE exit
                if self.hle && pc == HLE_BOOTLOADER_LR {
                    info!("Successfully returned to bootloader");
                    info!("Return value: {}", self.cpu.reg_get(ArmMode::User, 0));
                    return Ok(Some(Event::Halted));
                }

                // check to see if a watchpoint was hit
                if let Some(access) = hit_watchpoint {
                    return Ok(Some(match access.kind {
                        MemAccessKind::Read => Event::WatchRead(access.offset),
                        MemAccessKind::Write => Event::WatchWrite(access.offset),
                    }));
                }

                // check to see if a breakpoint was hit
                if self.breakpoints.contains(&pc) {
                    return Ok(Some(Event::Break));
                }
            }
            PowerState::Halt => {
                self.check_device_interrupts(halt_block_mode);
                if self.devices.vicmgr.fiq() || self.devices.vicmgr.irq() {
                    self.devices.syscon.set_run_mode();
                };
            }
            PowerState::Standby => {
                return Err(FatalError::UnimplementedPowerState(PowerState::Standby))
            }
        };

        Ok(None)
    }

    /// Run the system, returning successfully on "graceful exit".
    ///
    /// In HLE mode, a "graceful exit" is when the PC points into the
    /// bootloader's code.
    pub fn run(&mut self) -> Result<(), FatalError> {
        while self.step(BlockMode::Blocking)? != Some(Event::Halted) {}
        Ok(())
    }

    pub fn devices_mut(&mut self) -> &mut Ts7200Bus {
        &mut self.devices
    }

    /// Freeze the system such that `step` becomes a noop. Called prior to
    /// spawning a "post-mortem" GDB session.
    ///
    /// WARNING - THERE IS NO WAY TO "THAW" A FROZEN SYSTEM!
    pub fn freeze(&mut self) {
        self.frozen = true;
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
    pub uart3_hack: Option<devices::Uart>,
    pub vicmgr: devices::vic::VicManager,

    interrupt_bus: chan::Sender<(Interrupt, bool)>,
}

impl Ts7200Bus {
    fn new_hle(interrupt_bus: chan::Sender<(Interrupt, bool)>) -> Ts7200Bus {
        use devices::*;
        Ts7200Bus {
            sdram: Ram::new(32 * 1024 * 1024), // 32 MB
            syscon: Syscon::new_hle(),
            timer1: Timer::new("timer1", interrupt_bus.clone(), Interrupt::Tc1Ui, 16),
            timer2: Timer::new("timer2", interrupt_bus.clone(), Interrupt::Tc2Ui, 16),
            timer3: Timer::new("timer3", interrupt_bus.clone(), Interrupt::Tc3Ui, 32),
            uart1: Uart::new_hle("uart1", interrupt_bus.clone(), uart::interrupts::UART1),
            uart2: Uart::new_hle("uart2", interrupt_bus.clone(), uart::interrupts::UART2),
            uart3_hack: None,
            vicmgr: vic::VicManager::new(),

            interrupt_bus,
        }
    }

    pub fn hack_uart3_enable(&mut self) {
        use devices::*;
        self.uart3_hack = Some(Uart::new_hle(
            "uart3_hack",
            self.interrupt_bus.clone(),
            uart::interrupts::UART3,
        ));
    }
}

macro_rules! ts7200_mmap {
    ($($start:literal ..= $end:literal => $device:ident,)*) => {
        macro_rules! impl_ts7200_memory_r {
            ($fn:ident, $ret:ty) => {
                fn $fn(&mut self, addr: u32) -> MemResult<$ret> {
                    match addr {
                        $($start..=$end => self.$device.$fn(addr - $start),)*
                        _ => Err(MemException::Unexpected),
                    }
                }
            };
        }

        macro_rules! impl_ts7200_memory_w {
            ($fn:ident, $val:ty) => {
                fn $fn(&mut self, addr: u32, val: $val) -> MemResult<()> {
                    match addr {
                        $($start..=$end => self.$device.$fn(addr - $start, val),)*
                        _ => Err(MemException::Unexpected),
                    }
                }
            };
        }

        impl Device for Ts7200Bus {
            fn kind(&self) -> &'static str {
                "Ts7200"
            }

            fn probe(&self, offset: u32) -> Probe {
                match offset {
                    $($start..=$end => {
                        Probe::Device {
                            device: &self.$device,
                            next: Box::new(self.$device.probe(offset - $start))
                        }
                    })*
                    _ => Probe::Unmapped,
                }
            }
        }

        impl Memory for Ts7200Bus {
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
    0x808e_0000..=0x808e_ffff => uart3_hack,
    0x8093_0000..=0x8093_ffff => syscon,
}
