# ts7200

A high level emulator for the [TS-7200](https://www.embeddedarm.com/products/TS-7200) Single Board Computer, as used in CS 452 - Real Time Operating Systems at the University of Waterloo.

_Choo Choo!_ 🚂

## Disclaimer

The primary purpose of this emulator is to enable rapid prototyping and development of the CS 452 kernel without having to literally _live_ in the Trains lab. That said, at the end of the day, you won't be marked on how well your kernel runs in this emulator, you'll be marked on how well your kernel runs on the actual hardware in the trains lab!

**We make no guarantees about the accuracy and/or stability of this emulator! Use it at your own risk!**

- Instruction timings and hardware access times are _waaay_ off, so any profiling/benchmarking performed in the emulator won't be representative of the real hardware whatsoever!
- Emulated UARTs are can be quite forgiving when it comes to sending / receiving data. Namely, the CTS behavior is _not_ representative of actual hardware.

If you find any bugs, **please file an issue!**
Even better, if you happen to spot and fix a bug yourself, **please consider upstreaming any improvements!**

## Getting Started

`ts7200` is written in Rust, and uses the standard Rust package manager (`cargo`) for building / installation. If you don't have Rust installed, you can install it via [`rustup`](https://rustup.rs/).

You can install `ts7200` to your `$PATH` by running the following command:

```bash
cargo install --path .
```

Alternatively, if you simply want to try-out the emulator, you can build and run it without adding it to you `$PATH`:

```bash
cargo run --release -- [FLAGS] [OPTIONS] <kernel-elf>
```

Run `ts7200 --help` for information on how to use the emulator.

**Make sure to redirect the emulator's `stderr` to a file / tty!**

By default, the emulator will put the tty into "raw" mode, which doesn't play nicely with the the logging information output via `stderr`. When `stderr` outputs text to the tty, it will end up "smearing" it across the terminal. This can be fixed by redirecting `ts7200`'s `stderr` to a separate tty: `ts7200 ... 2> /dev/pts/X`.

## Emulator Enhancements

There are quite a few features present in the emulator which are _not_ available on actual hardware:

- GDB Debugging across context switches!
    - The GDB server is part of the emulator itself (as opposed to running within the emulated hardware), which means it can provide "true" instruction-level step-by-step debugging of your code.
    - To start a debugging session, pass the `-g` flag to `ts7200`, and use the GDB command `target remote localhost:<port>` to connect to the GDB server.
- Emulated devices emit `ContractViolation` errors if they are accessed "incorrectly", instead of silently-failing as they would on real hardware. Keep an eye on those `stderr` logs!
    - e.g: Accessing Uninitialized RAM logs a warning to stderr
    - e.g: Trying to enable a timer without giving it an initial value throws a fatal error

## Emulator Quirks

- Instead of zeroing-out RAM, uninitialized RAM is set to the ASCII value corresponding to '-' (i.e: decimal 45, hex 0x2d). This, along with the uninitialized RAM logs, makes it easier to spot any uninitialized memory issues in your code.
- While most keycodes are forwarded directly to the emulated code, `Ctrl-C` is "hijacked" within `ts7200` to early-terminate the emulator. If your Kernel uses `Ctrl-C` for any functionality, you may need to tweak either `ts7200` or your kernel's source.
    - This could be fixed by implemented a "leader-key" system, similar to tmux. PRs welcome!
- GDB Debugging "breaks" in the presence of Timer interrupts
    - Trying to step to the next instruction will most-likely result in GDB jumping to the IRQ handler instead
    - This can be worked-around by avoiding the `n` and `s` GDB commands when debugging code with IRQs, and instead setting specific breakpoints via `b` to "step" through the code. Yes, it's annoying. Sorry!

## Project Status

At the moment, the majority of devices critical for running basic CS 452 kernels have been implemented and tested. _That said,_ there are plenty of devices that could still be added and/or improved further. **PRs are more than welcome!**

**NOTE:** This is a non-exhaustive list of the project's status. There are also a plethora of TODOs, FIXMEs, XXXs, and stubs littered throughout the codebase, which provide an informal overview of subtle bits of missing and/or flat out wrong functionality.

- Core features
    - [x] HLE boot (emulating CS 452's Redboot configuration)
        - [x] ELF file parsing
        - [x] Initializes devices / key memory locations with hardware-validated values
    - [x] Debugging with GDB
- Devices
    - [x] UARTs - _Implemented, but too forgiving!_
        - Real UART hardware can be quite finicky, _especially_ when interacting with the Marklin train controller. Things that work fine on the emulator may _not_ work on actual hardware!
        - [x] RX/TX Flags
        - [ ] CTS flag (_inaccurate: always set to `1` whenever UART isn't transmitting_)
        - [x] All "important" registers (for CS 452)
        - [x] Interrupts
    - [x] Timers - _Totally Accurate!_
        - _Note:_ Timer 4, the 47 bit debug timer, is _not_ currently implemented.
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
        - [x] The two 32bit scratch registers (just for fun lol)
    - [ ] RTC
    - [ ] Co-Processor Functionality
        - _Note:_ `arm7tdmi-rs` doesn't currently expose a configurable coprocessor interface. Instead, any coprocessor operations are simply logged, and treated as no-ops. Until `arm7tdmi-rs` adds support for custom coprocessors, the following devices cannot be emulated correctly:
        - [ ] MMU
        - [ ] Caches
        - [ ] MaverickCrunch Co-Processor (i.e: math coprocessor)

## Non-Goals

- LLE emulation of all TS-7200 hardware (e.g: to support "cold boots" directly into Redboot)
    - Most of the devices on the board aren't used by the CS 452 kernel, so emulating / stubbing them out just to get Redboot working isn't a great use of our time.
- Totally Accurate CPU performance
    - The emulated CPU runs as fast as the host system lets it, so performance will vary based on which machine you run the emulator on.
    - _Note:_ Timers are implemented using the system clock, and will do the Right Thing no matter how fast the host system is.
- Train emulation
    - You mean you want me to write a physics simulator for virtual trains? Hahahaha, yeah... no.
    - _Update:_ Looks like someone else was crazy enough to actually attempt doing this! Check out the [MarklinSim](https://github.com/Martin1994/MarklinSim) project!
        - You may need to use the `--hack-inf-uart-rx` flag to get this working, depending on how you're querying CTS in your kernel.
