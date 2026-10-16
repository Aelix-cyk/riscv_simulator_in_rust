//! A RISC-V simulator built progressively around the RVA22S64 profile.

pub mod bus;
pub mod cli;
pub mod cpu;
pub mod decode;
pub(crate) mod inst;
pub mod machine;
pub mod memory;
pub(crate) mod opcodes;
pub mod trap;
