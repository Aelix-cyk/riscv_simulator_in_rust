//! Architectural state of a single hart.

use crate::csr::Csrs;

/// Number of general-purpose (integer) registers in the base ISA.
pub const GPR_COUNT: usize = 32;

/// Number of single-precision floating-point registers in the F extension.
pub const FPR_COUNT: usize = 32;

/// Index of the hardwired zero register, `x0`.
pub const X0: usize = 0;

/// Index of the return-address register, `x1`.
pub const RA: u8 = 1;

/// Index of the stack-pointer register, `x2`.
pub const SP: u8 = 2;

/// Index of the first integer argument register, `x10`.
pub const A0: usize = 10;

/// Index of the second integer argument register, `x11`.
pub const A1: usize = 11;

/// Privilege mode currently executing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrivilegeMode {
    User,
    Supervisor,
    Machine,
}

impl PrivilegeMode {
    /// Encoding used by `mstatus.MPP`.
    pub(crate) const fn mpp_bits(self) -> u64 {
        match self {
            Self::User => 0,
            Self::Supervisor => 0b01,
            Self::Machine => 0b11,
        }
    }

    /// Numeric privilege ordering used by CSR access checks.
    pub(crate) const fn rank(self) -> u8 {
        match self {
            Self::User => 0,
            Self::Supervisor => 1,
            Self::Machine => 2,
        }
    }
}

/// ABI names of the general-purpose registers, indexed by register number.
pub const ABI_NAMES: [&str; GPR_COUNT] = [
    "zero", "ra", "sp", "gp", "tp", "t0", "t1", "t2", "s0", "s1", "a0", "a1", "a2", "a3", "a4",
    "a5", "a6", "a7", "s2", "s3", "s4", "s5", "s6", "s7", "s8", "s9", "s10", "s11", "t3", "t4",
    "t5", "t6",
];

/// ABI names of the floating-point registers, indexed by register number.
pub const FPR_ABI_NAMES: [&str; FPR_COUNT] = [
    "ft0", "ft1", "ft2", "ft3", "ft4", "ft5", "ft6", "ft7", "fs0", "fs1", "fa0", "fa1", "fa2",
    "fa3", "fa4", "fa5", "fa6", "fa7", "fs2", "fs3", "fs4", "fs5", "fs6", "fs7", "fs8", "fs9",
    "fs10", "fs11", "ft8", "ft9", "ft10", "ft11",
];

/// ABI name of general-purpose register `index`.
pub fn abi_name(index: u8) -> &'static str {
    ABI_NAMES[usize::from(index)]
}

/// ABI name of floating-point register `index`.
pub fn fpr_abi_name(index: u8) -> &'static str {
    FPR_ABI_NAMES[usize::from(index)]
}

/// General-purpose register file and program counter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cpu {
    gprs: [u64; GPR_COUNT],
    fprs: [u64; FPR_COUNT],
    pc: u64,
    csrs: Csrs,
    privilege: PrivilegeMode,
}

impl Cpu {
    /// Creates a hart with every register and the PC cleared.
    pub fn new() -> Self {
        Self {
            gprs: [0; GPR_COUNT],
            fprs: [0; FPR_COUNT],
            pc: 0,
            csrs: Csrs::default(),
            privilege: PrivilegeMode::Machine,
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

    /// Returns the current privilege mode.
    pub fn privilege(&self) -> PrivilegeMode {
        self.privilege
    }

    /// Sets the current privilege mode.
    pub(crate) fn set_privilege(&mut self, privilege: PrivilegeMode) {
        self.privilege = privilege;
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

    /// Reads floating-point register `index` as architectural bits.
    pub fn read_fpr(&self, index: usize) -> u64 {
        self.fprs[index]
    }

    /// Writes floating-point register `index` as architectural bits.
    pub fn write_fpr(&mut self, index: usize, value: u64) {
        self.fprs[index] = value;
        self.csrs.mark_fp_dirty();
    }

    /// Whether the F extension's register state is enabled by `mstatus.FS`.
    pub fn fp_enabled(&self) -> bool {
        self.csrs.fp_enabled()
    }

    /// Reads an implemented CSR, or returns `None` for an unsupported address.
    pub fn read_csr(&self, address: u16) -> Option<u64> {
        self.csrs.read_with_privilege(address, self.privilege)
    }

    /// Reads the software-writable value used as a CSR read-modify-write base.
    pub(crate) fn read_csr_for_rmw(&self, address: u16) -> Option<u64> {
        self.csrs
            .read_for_rmw_with_privilege(address, self.privilege)
    }

    /// Reads the combined floating-point control and status register.
    pub fn read_fcsr(&self) -> Option<u64> {
        self.read_csr(crate::inst::CSR_FCSR)
    }

    /// Writes an implemented CSR and reports whether the address was valid.
    pub(crate) fn write_csr(&mut self, address: u16, value: u64) -> bool {
        self.csrs
            .write_with_privilege(address, value, self.privilege)
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
        for index in 0..FPR_COUNT {
            assert_eq!(cpu.read_fpr(index), 0, "f{index} should start at zero");
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
    fn floating_point_register_writes_mark_state_dirty() {
        let mut cpu = Cpu::new();
        let status = 0b01 << 13;
        assert!(cpu.write_csr(crate::inst::CSR_MSTATUS, status));

        cpu.write_fpr(3, 0xffff_ffff_3f80_0000);

        assert_eq!(cpu.read_fpr(3), 0xffff_ffff_3f80_0000);
        assert_eq!(
            cpu.read_csr(crate::inst::CSR_MSTATUS).unwrap() & (0b11 << 13),
            0b11 << 13
        );
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
