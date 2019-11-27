#![allow(clippy::cast_lossless)]

use std::io::Read;

use log::*;

pub mod debugger;
pub mod devices;
pub mod macros;
pub mod memory;
pub mod ts7200;
pub mod util;

use arm7tdmi_rs::reg;
use ts7200::{Ts7200, HLE_BOOTLOADER_LR};

fn main() -> std::io::Result<()> {
    pretty_env_logger::init();

    let args: Vec<String> = std::env::args().collect();

    let file = std::fs::File::open(args.get(1).expect("must provide .elf to load"))?;
    let mut system = Ts7200::new_hle(file)?;

    // hook up UARTs to I/O

    // uart1 is for trains
    // uart2 is for console communication
    system
        .devices_mut()
        .uart2
        .set_reader(Some(Box::new(std::io::stdin())));
    system
        .devices_mut()
        .uart2
        .set_writer(Some(Box::new(std::io::stdout())));

    // TODO: implement a gdb stub instead of the current terrible debugger
    let mut debugger = None;
    if let Some(objdump_fname) = args.get(2) {
        let mut dbg = debugger::asm2line::Asm2Line::new();
        dbg.load_objdump(objdump_fname)?;
        debugger = Some(dbg)
    }

    let mut step_through = true;
    loop {
        let pc = system.cpu().reg_get(0, reg::PC);

        if pc == HLE_BOOTLOADER_LR {
            eprintln!("Successfully returned to bootloader");
            return Ok(());
        }

        #[allow(clippy::single_match)]
        match pc {
            // 0x0021_9064 => step_through = true,
            _ => {}
        }

        // quick-and-dirty step through
        if step_through {
            if let Some(ref mut debugger) = debugger {
                match debugger.lookup(pc) {
                    Some(info) => debug!("{}", info),
                    None => debug!("{:#010x?}: ???", pc),
                }
            }

            loop {
                let c = std::io::stdin().bytes().next().unwrap().unwrap();
                // consume the newline
                if c as char != '\n' {
                    std::io::stdin().bytes().next().unwrap().unwrap();
                }

                match c as char {
                    'r' => step_through = false,
                    'c' => {
                        eprintln!("{:#x?}", system);
                        continue;
                    }
                    _ => {}
                }

                break;
            }
        }

        if let Err(fatal_error) = system.cycle() {
            eprintln!("Fatal Error!");
            eprintln!("============");
            eprintln!("{:#010x?}", system);
            if let Some(ref mut debugger) = debugger {
                match debugger.lookup(pc) {
                    Some(info) => eprintln!("{}", info),
                    None => eprintln!("{:#010x?}: ???", pc),
                }
            }
            eprintln!("{:#010x?}", fatal_error);
            panic!();
        }
    }
}
