//! A RISC-V simulator built progressively around the RVA22S64 profile.

pub mod bus;
pub mod cli;
pub mod cpu;
pub(crate) mod csr;
pub mod decode;
pub(crate) mod fp;
pub(crate) mod fp16;
pub(crate) mod fp64;
pub(crate) mod inst;
pub mod machine;
pub mod memory;
pub(crate) mod mmu;
pub(crate) mod opcodes;
pub mod platform;
pub mod trap;
