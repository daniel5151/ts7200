use std::fmt::Debug;
use std::net::{TcpListener, TcpStream, ToSocketAddrs};

use log::*;

#[derive(Debug)]
pub enum AccessKind {
    Read,
    Write,
}

#[derive(PartialEq, Eq)]
pub enum TargetState {
    Running,
    Halted,
}

/// The set of operations that a GDB target needs to implement.
pub trait GdbStubTarget {
    /// The target architecture's pointer size
    type Usize: Debug;
    /// A target-specific unrecoverable error, which should be propogated
    /// through the GdbStub
    type TargetFatalError;

    // /// Read a byte from a memory address
    // fn read(&mut self, addr: Self::Usize) -> u8;
    // /// Write a byte to a memory address
    // fn write(&mut self, addr: Self::Usize, val: u8);

    /// Perform a single "step" of the target CPU, recording any memory accesses
    /// in the `mem_accesses` vector. The return value indicates
    fn step(
        &mut self,
        mem_accesses: &mut Vec<(AccessKind, Self::Usize, u8)>,
    ) -> Result<TargetState, Self::TargetFatalError>;
}

/// [`GdbStub`] maintains the state of a GDB remote debugging session (including
/// the underlying TCP connection), and
pub struct GdbStub<T: GdbStubTarget> {
    stream: TcpStream,
    mem_accesses: Vec<(AccessKind, T::Usize, u8)>,
}

impl<T: GdbStubTarget> GdbStub<T> {
    pub fn new(sockaddr: impl ToSocketAddrs + Debug) -> std::io::Result<GdbStub<T>> {
        info!("Waiting for a GDB connection on {:?}...", sockaddr);

        let sock = TcpListener::bind(sockaddr)?;
        let (stream, addr) = sock.accept()?;

        stream.set_nonblocking(true)?;

        info!("Debugger connected from {}", addr);
        Ok(GdbStub {
            stream,
            mem_accesses: Vec::new(),
        })
    }

    /// Executes the target,
    pub fn run(&mut self, target: &mut T) -> Result<TargetState, T::TargetFatalError> {
        loop {
            if target.step(&mut self.mem_accesses)? == TargetState::Halted {
                return Ok(TargetState::Halted);
            };

            // TODO: check for incoming packets, and actually do things lol
        }
    }
}
