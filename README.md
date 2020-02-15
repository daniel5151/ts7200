# ts7200

A high level emulator for the [TS-7200](https://www.embeddedarm.com/products/TS-7200) Single Board Computer, as used in CS 452 - Real Time Operating Systems at the University of Waterloo.

AKA: _choochoo_emu_ 🚂

## Disclaimer

The primary purpose of this emulator is to enable rapid prototyping and development of the CS 452 kernel without having to literally _live_ in the Trains lab. That said, at the end of the day, you won't be marked on how well your kernel runs in this emulator, you'll be marked on how well your kernel runs on the actual hardware in the trains lab!

**We make no guarantees about the accuracy and/or stability of this emulator! Use it at your own risk!**

Instruction timings and hardware access times are _waaay_ off, so any profiling/benchmarking performed in the emulator won't be representative of the real hardware whatsoever!

## Emulator Quirks

- Instead of zeroing-out RAM, uninitialized RAM is set to the ASCII value corresponding to '-' (i.e: 45). This should make it easier to catch uninitialized memory bugs.

## Status

**NOTE:** This is a non-exhaustive list of the project's status. There are also a plethora of TODOs, FIXMEs, XXXs, and stubs littered throughout the codebase, which provide an informal overview of subtle bits of missing and/or flat out wrong functionality.

- Core features
    - [x] HLE boot (emulating CS 452's Redboot configuration)
        - [x] ELF file parsing
        - [x] Initializes devices / key memory locations with hardware-validated values
    - [x] Debugging with GDB
- Devices
    - [x] UARTs - _Implemented, but Incomplete_
        - [ ] TX bit is always set (can always write data)
        - [ ] RX is non-blocking (i.e: cannot receive if UART has no data to send)
        - [ ] Interrupts
    - [x] Timers - _Totally Accurate!_
        - [x] All Documented Register Functionality
        - [x] Interrupts
    - [x] VIC - _Mostly Accurate_
        - [x] Asserts and Clears Interrupts
        - [x] Correct daisy-chaining behavior
        - [x] Vectored Interrupt Support (_caution: not very well tested_)
        - [ ] Reading from the VectAddr register doesn't actually mask out interrupts until you write to it
        - [ ] Protection bit can be accessed from _any_ mode (not just privileged modes)
    - [x] System Controller (Syscon) - _Only the Important Parts_
        - _Note:_ Lot of stuff in the Syscon isn't relevant to CS 452, and will be left unimplemented
        - [x] Correct handling of SW Locked Registers
        - [x] Low Power Halt
        - [ ] Low Power Standby
        - [x] The two 32bit scratch registers (lmao)
    - [ ] RTC
    - [ ] Coprocessors
        - _Note:_ `arm7tdmi-rs` doesn't currently expose a configurable coprocessor interface. Instead, any coprocessor operations are simply logged, and treated as no-ops. Until `arm7tdmi-rs` adds support for custom coprocessors, the following devices cannot be emulated correctly:
        - [ ] MMU
        - [ ] Caches
        - [ ] MaverickCrunch Co-Processor (i.e: math coprocessor)

## Non-Goals

- LLE emulation of all TS-7200 hardware (e.g: to support "cold boots" directly into Redboot)
    - Most of the devices on the board aren't used by the CS 452 kernel, so emulating / stubbing them out just to get Redboot working isn't a great use of our time.
- Accurate CPU performance
    - The emulated CPU runs as fast as it can, and it's performance will vary based on which machine you run the emulator on.
    - _Note:_ Timers are implemented using the system clock, and do the Right Thing no matter how fast the host system is.
- Train emulation
    - You mean you want me to write a physics simulator for virtual trains? Hahahaha, yeah... no.
    - _Update:_ Looks like someone else was crazy enough to actually attempt doing this! Check out the [MarklinSim](https://github.com/Martin1994/MarklinSim) project. With a bit of bash-glue, you should be able to hook it up to `ts7200`!
