//! Machine-mode control and status registers.

use crate::inst;

const MEDIUM_INSTRUCTION_BYTES: u64 = 4;
const MTVEC_ALIGNMENT_MASK: u64 = !(MEDIUM_INSTRUCTION_BYTES - 1);
const MEPC_ALIGNMENT_MASK: u64 = !1;
const MSTATUS_MIE: u64 = 1 << 3;
const MSTATUS_MPIE: u64 = 1 << 7;
const MSTATUS_MPP_MASK: u64 = 0b11 << 11;
const MSTATUS_WRITABLE: u64 = MSTATUS_MIE | MSTATUS_MPIE | MSTATUS_MPP_MASK;
const MACHINE_MODE: u64 = 0b11;
const MXL_SHIFT: u32 = 62;
const MXL_64: u64 = 2;
const MISA_EXT_A_SHIFT: u32 = 0;
const MISA_EXT_C_SHIFT: u32 = 2;
const MISA_EXT_I_SHIFT: u32 = 8;
const MISA_EXT_M_SHIFT: u32 = 12;
const MISA_VALUE: u64 = (MXL_64 << MXL_SHIFT)
    | (1 << MISA_EXT_A_SHIFT)
    | (1 << MISA_EXT_C_SHIFT)
    | (1 << MISA_EXT_I_SHIFT)
    | (1 << MISA_EXT_M_SHIFT);

/// Architectural machine-mode CSR state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct Csrs {
    mstatus: u64,
    mtvec: u64,
    mscratch: u64,
    mepc: u64,
    mcause: u64,
    mtval: u64,
    mie: u64,
    medeleg: u64,
    mideleg: u64,
    pmpcfg0: u64,
    pmpaddr0: u64,
    mnstatus: u64,
}

impl Csrs {
    /// Reads an implemented CSR.
    pub(crate) fn read(self, address: u16) -> Option<u64> {
        match address {
            inst::CSR_MSTATUS => Some(self.mstatus),
            inst::CSR_MISA => Some(MISA_VALUE),
            inst::CSR_MHARTID => Some(0),
            inst::CSR_MTVEC => Some(self.mtvec),
            inst::CSR_MSCRATCH => Some(self.mscratch),
            inst::CSR_MEPC => Some(self.mepc),
            inst::CSR_MCAUSE => Some(self.mcause),
            inst::CSR_MTVAL => Some(self.mtval),
            inst::CSR_MIE => Some(self.mie),
            inst::CSR_MEDELEG => Some(self.medeleg),
            inst::CSR_MIDELEG => Some(self.mideleg),
            inst::CSR_PMPCFG0 => Some(self.pmpcfg0),
            inst::CSR_PMPADDR0 => Some(self.pmpaddr0),
            inst::CSR_MNSTATUS => Some(self.mnstatus),
            inst::CSR_SATP => Some(0),
            _ => None,
        }
    }

    /// Writes an implemented CSR, applying its writable-field rules.
    pub(crate) fn write(&mut self, address: u16, value: u64) -> bool {
        match address {
            inst::CSR_MSTATUS => self.mstatus = value & MSTATUS_WRITABLE,
            // MXL and the implemented extension bits are fixed, so WARL writes
            // are accepted and ignored.
            inst::CSR_MISA => {}
            inst::CSR_MTVEC => self.mtvec = value & MTVEC_ALIGNMENT_MASK,
            inst::CSR_MSCRATCH => self.mscratch = value,
            inst::CSR_MEPC => self.mepc = value & MEPC_ALIGNMENT_MASK,
            inst::CSR_MCAUSE => self.mcause = value,
            inst::CSR_MTVAL => self.mtval = value,
            inst::CSR_MIE => self.mie = value,
            inst::CSR_MEDELEG => self.medeleg = value,
            inst::CSR_MIDELEG => self.mideleg = value,
            inst::CSR_PMPCFG0 => self.pmpcfg0 = value,
            inst::CSR_PMPADDR0 => self.pmpaddr0 = value,
            inst::CSR_MNSTATUS => self.mnstatus = value,
            // Only Bare translation is implemented.
            inst::CSR_SATP => {}
            _ => return false,
        }
        true
    }

    /// Records a synchronous machine-mode trap.
    pub(crate) fn enter_trap(&mut self, epc: u64, cause: u64, tval: u64) {
        if self.mstatus & MSTATUS_MIE != 0 {
            self.mstatus |= MSTATUS_MPIE;
        } else {
            self.mstatus &= !MSTATUS_MPIE;
        }
        self.mstatus &= !MSTATUS_MIE;
        self.mstatus = (self.mstatus & !MSTATUS_MPP_MASK) | (MACHINE_MODE << 11);

        self.mepc = epc & MEPC_ALIGNMENT_MASK;
        self.mcause = cause;
        self.mtval = tval;
    }

    /// Restores machine state for `mret` and returns the saved PC.
    pub(crate) fn return_from_m_trap(&mut self) -> u64 {
        if self.mstatus & MSTATUS_MPIE != 0 {
            self.mstatus |= MSTATUS_MIE;
        } else {
            self.mstatus &= !MSTATUS_MIE;
        }
        self.mstatus |= MSTATUS_MPIE;
        self.mstatus &= !MSTATUS_MPP_MASK;
        self.mepc
    }

    /// Address entered by a synchronous trap.
    pub(crate) fn trap_vector(self) -> u64 {
        self.mtvec & MTVEC_ALIGNMENT_MASK
    }

    /// PC saved by the most recent trap.
    pub(crate) fn exception_pc(self) -> u64 {
        self.mepc
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn implemented_csrs_round_trip_with_warl_masks() {
        let mut csrs = Csrs::default();

        assert!(csrs.write(inst::CSR_MSTATUS, u64::MAX));
        assert!(csrs.write(inst::CSR_MTVEC, u64::MAX));
        assert!(csrs.write(inst::CSR_MSCRATCH, u64::MAX));
        assert!(csrs.write(inst::CSR_MEPC, u64::MAX));
        assert!(csrs.write(inst::CSR_MCAUSE, u64::MAX));
        assert!(csrs.write(inst::CSR_MTVAL, u64::MAX));
        assert!(csrs.write(inst::CSR_MIE, u64::MAX));
        assert!(csrs.write(inst::CSR_MEDELEG, u64::MAX));
        assert!(csrs.write(inst::CSR_MIDELEG, u64::MAX));
        assert!(csrs.write(inst::CSR_PMPCFG0, u64::MAX));
        assert!(csrs.write(inst::CSR_PMPADDR0, u64::MAX));
        assert!(csrs.write(inst::CSR_MNSTATUS, u64::MAX));
        assert!(csrs.write(inst::CSR_SATP, u64::MAX));

        assert_eq!(csrs.read(inst::CSR_MSTATUS), Some(MSTATUS_WRITABLE));
        assert_eq!(csrs.read(inst::CSR_MTVEC), Some(MTVEC_ALIGNMENT_MASK));
        assert_eq!(csrs.read(inst::CSR_MEPC), Some(MEPC_ALIGNMENT_MASK));
        assert_eq!(csrs.read(inst::CSR_MSCRATCH), Some(u64::MAX));
        assert_eq!(csrs.read(inst::CSR_MCAUSE), Some(u64::MAX));
        assert_eq!(csrs.read(inst::CSR_MTVAL), Some(u64::MAX));
        assert_eq!(csrs.read(inst::CSR_MIE), Some(u64::MAX));
        assert_eq!(csrs.read(inst::CSR_MEDELEG), Some(u64::MAX));
        assert_eq!(csrs.read(inst::CSR_MIDELEG), Some(u64::MAX));
        assert_eq!(csrs.read(inst::CSR_PMPCFG0), Some(u64::MAX));
        assert_eq!(csrs.read(inst::CSR_PMPADDR0), Some(u64::MAX));
        assert_eq!(csrs.read(inst::CSR_MNSTATUS), Some(u64::MAX));
        assert_eq!(csrs.read(inst::CSR_SATP), Some(0));
    }

    #[test]
    fn unsupported_csrs_are_rejected() {
        let mut csrs = Csrs::default();

        assert_eq!(csrs.read(inst::CSR_MIP), None);
        assert!(!csrs.write(inst::CSR_MIP, 1));
    }

    #[test]
    fn misa_is_a_fixed_rv64imac_warl() {
        let mut csrs = Csrs::default();

        assert_eq!(csrs.read(inst::CSR_MISA), Some(MISA_VALUE));
        assert!(csrs.write(inst::CSR_MISA, u64::MAX));
        assert_eq!(csrs.read(inst::CSR_MISA), Some(MISA_VALUE));
    }

    #[test]
    fn trap_entry_and_return_preserve_machine_interrupt_state() {
        let mut csrs = Csrs::default();
        csrs.write(inst::CSR_MSTATUS, MSTATUS_MIE | MSTATUS_MPP_MASK);

        csrs.enter_trap(0x8000_0004, 11, 0);

        let status = csrs.read(inst::CSR_MSTATUS).unwrap();
        assert_eq!(status & MSTATUS_MIE, 0);
        assert_eq!(status & MSTATUS_MPIE, MSTATUS_MPIE);
        assert_eq!(status & MSTATUS_MPP_MASK, MACHINE_MODE << 11);
        assert_eq!(csrs.read(inst::CSR_MEPC), Some(0x8000_0004));
        assert_eq!(csrs.read(inst::CSR_MCAUSE), Some(11));
        assert_eq!(csrs.read(inst::CSR_MTVAL), Some(0));

        assert_eq!(csrs.return_from_m_trap(), 0x8000_0004);
        let status = csrs.read(inst::CSR_MSTATUS).unwrap();
        assert_eq!(status & MSTATUS_MIE, MSTATUS_MIE);
        assert_eq!(status & MSTATUS_MPIE, MSTATUS_MPIE);
        assert_eq!(status & MSTATUS_MPP_MASK, 0);
    }
}
