//! Architectural state of a single hart.

use crate::csr::Csrs;

/// Number of general-purpose (integer) registers in the base ISA.
pub const GPR_COUNT: usize = 32;

/// Index of the hardwired zero register, `x0`.
pub const X0: usize = 0;

/// Index of the return-address register, `x1`.
pub const RA: u8 = 1;

/// Index of the stack-pointer register, `x2`.
pub const SP: u8 = 2;

/// ABI names of the general-purpose registers, indexed by register number.
pub const ABI_NAMES: [&str; GPR_COUNT] = [
    "zero", "ra", "sp", "gp", "tp", "t0", "t1", "t2", "s0", "s1", "a0", "a1", "a2", "a3", "a4",
    "a5", "a6", "a7", "s2", "s3", "s4", "s5", "s6", "s7", "s8", "s9", "s10", "s11", "t3", "t4",
    "t5", "t6",
];

/// ABI name of general-purpose register `index`.
pub fn abi_name(index: u8) -> &'static str {
    ABI_NAMES[usize::from(index)]
}

/// General-purpose register file and program counter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cpu {
    gprs: [u64; GPR_COUNT],
    pc: u64,
    csrs: Csrs,
}

impl Cpu {
    /// Creates a hart with every register and the PC cleared.
    pub fn new() -> Self {
        Self {
            gprs: [0; GPR_COUNT],
            pc: 0,
            csrs: Csrs::default(),
        }
    }

    /// Returns the program counter.
    pub fn pc(&self) -> u64 {
        self.pc
    }

    /// Sets the program counter.
    pub fn set_pc(&mut self, value: u64) {
        self.pc = value;
    }

    /// Reads general-purpose register `index`. `x0` always reads as zero.
    pub fn read_gpr(&self, index: usize) -> u64 {
        self.gprs[index]
    }

    /// Writes general-purpose register `index`. Writes to `x0` are discarded.
    pub fn write_gpr(&mut self, index: usize, value: u64) {
        if index != X0 {
            self.gprs[index] = value;
        }
    }

    /// Reads an implemented CSR, or returns `None` for an unsupported address.
    pub fn read_csr(&self, address: u16) -> Option<u64> {
        self.csrs.read(address)
    }

    /// Writes an implemented CSR and reports whether the address was valid.
    pub(crate) fn write_csr(&mut self, address: u16, value: u64) -> bool {
        self.csrs.write(address, value)
    }

    /// Immutable CSR state for trap delivery.
    pub(crate) fn csrs(&self) -> &Csrs {
        &self.csrs
    }

    /// Mutable CSR state for trap delivery.
    pub(crate) fn csrs_mut(&mut self) -> &mut Csrs {
        &mut self.csrs
    }
}

impl Default for Cpu {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_zeroed() {
        let cpu = Cpu::new();

        assert_eq!(cpu.pc(), 0);
        for index in 0..GPR_COUNT {
            assert_eq!(cpu.read_gpr(index), 0, "x{index} should start at zero");
        }
    }

    #[test]
    fn write_then_read_round_trips() {
        let mut cpu = Cpu::new();

        cpu.write_gpr(5, 0xdead_beef_cafe_f00d);

        assert_eq!(cpu.read_gpr(5), 0xdead_beef_cafe_f00d);
    }

    #[test]
    fn x0_discards_writes() {
        let mut cpu = Cpu::new();

        cpu.write_gpr(X0, 42);

        assert_eq!(cpu.read_gpr(X0), 0);
    }

    #[test]
    fn registers_are_independent() {
        let mut cpu = Cpu::new();

        for index in 0..GPR_COUNT {
            cpu.write_gpr(index, index as u64 + 1);
        }

        for index in 1..GPR_COUNT {
            assert_eq!(cpu.read_gpr(index), index as u64 + 1);
        }
        assert_eq!(cpu.read_gpr(X0), 0);
    }

    #[test]
    fn pc_round_trips() {
        let mut cpu = Cpu::new();

        cpu.set_pc(0x8000_0000);

        assert_eq!(cpu.pc(), 0x8000_0000);
    }

    #[test]
    fn default_matches_new() {
        assert_eq!(Cpu::default(), Cpu::new());
    }
}
