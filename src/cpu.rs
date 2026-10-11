//! Architectural state of a single hart.

/// Number of general-purpose (integer) registers in the base ISA.
pub const GPR_COUNT: usize = 32;

/// Index of the hardwired zero register, `x0`.
pub const X0: usize = 0;

/// General-purpose register file and program counter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cpu {
    gprs: [u64; GPR_COUNT],
    pc: u64,
}

impl Cpu {
    /// Creates a hart with every register and the PC cleared.
    pub fn new() -> Self {
        Self {
            gprs: [0; GPR_COUNT],
            pc: 0,
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
}
