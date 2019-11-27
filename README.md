# ts7200

An emulator for the [TS-7200](https://www.embeddedarm.com/products/TS-7200) Single Board Computer, as used in CS 452 at the University of Waterloo.

AKA: _choo_choo_emu_ 🚂

## Disclaimer

The primary purpose of this emulator is to enable rapid prototyping and development of the CS 452 kernel without having to literally _live_ in the Trains lab. That said, at the end of the day, you won't be marked on how well your kernel runs in this emulator, you'll be marked on how well your kernel runs on the actual hardware in the trains lab!

Instruction timings and hardware access times are _waaay_ off, so any profiling/benchmarking performed in the emulator won't be representative of the real hardware whatsoever.

## Status

- Core features
    - [x] HLE boot (emulating CS 452's Redboot configuration)
        - [x] ELF file parsing
    - [ ] Debugging with GDB
    - [ ] Improve CPU emulation
        - Fix bugs in arm7tdmi-rs
        - _OR_: switch to the CPU emulator from [Pyrite](https://github.com/ExPixel/Pyrite), a GBA emulator written in Rust.
- Devices
    - [ ] UARTs
        - [x] Busy-Wait I/O
        - [ ] Interrupts
        - [ ] Accurate flag handling
    - [ ] RTC
    - [ ] Timers
    - [ ] LED

## Non-Goals

- LLE emulation of all TS-7200 hardware (to support a "cold boot" into Redboot)
    - Most of the devices on the board aren't used by the CS 452 kernel
- Train emulation
    - You mean you want me to write a physics simulator for virtual trains? Yeah... no.
