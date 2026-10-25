//! Machine-mode control and status registers.

use crate::cpu::PrivilegeMode;
use crate::inst;

const MEDIUM_INSTRUCTION_BYTES: u64 = 4;
const MTVEC_ALIGNMENT_MASK: u64 = !(MEDIUM_INSTRUCTION_BYTES - 1);
const MEPC_ALIGNMENT_MASK: u64 = !1;
const MSTATUS_UXL_SHIFT: u32 = 32;
const MSTATUS_UXL_64: u64 = 2;
const MSTATUS_UXL: u64 = MSTATUS_UXL_64 << MSTATUS_UXL_SHIFT;
const MSTATUS_MIE: u64 = 1 << 3;
const MSTATUS_MPIE: u64 = 1 << 7;
const MSTATUS_MPP_SHIFT: u32 = 11;
const MSTATUS_MPP_MASK: u64 = 0b11 << MSTATUS_MPP_SHIFT;
const MSTATUS_FS_SHIFT: u32 = 13;
const MSTATUS_FS_MASK: u64 = 0b11 << MSTATUS_FS_SHIFT;
const MSTATUS_FS_DIRTY: u64 = 0b11 << MSTATUS_FS_SHIFT;
const MSTATUS_SD: u64 = 1 << 63;
const MSTATUS_WRITABLE: u64 = MSTATUS_MIE | MSTATUS_MPIE | MSTATUS_MPP_MASK | MSTATUS_FS_MASK;
const MSTATUS_FIXED: u64 = MSTATUS_UXL;
const MXL_SHIFT: u32 = 62;
const MXL_64: u64 = 2;
const MISA_EXT_A_SHIFT: u32 = 0;
const MISA_EXT_B_SHIFT: u32 = 1;
const MISA_EXT_C_SHIFT: u32 = 2;
const MISA_EXT_D_SHIFT: u32 = 3;
const MISA_EXT_F_SHIFT: u32 = 5;
const MISA_EXT_I_SHIFT: u32 = 8;
const MISA_EXT_M_SHIFT: u32 = 12;
const MISA_EXT_U_SHIFT: u32 = 20;
const MISA_VALUE: u64 = (MXL_64 << MXL_SHIFT)
    | (1 << MISA_EXT_A_SHIFT)
    | (1 << MISA_EXT_B_SHIFT)
    | (1 << MISA_EXT_C_SHIFT)
    | (1 << MISA_EXT_D_SHIFT)
    | (1 << MISA_EXT_F_SHIFT)
    | (1 << MISA_EXT_I_SHIFT)
    | (1 << MISA_EXT_M_SHIFT)
    | (1 << MISA_EXT_U_SHIFT);

const CSR_PRIVILEGE_SHIFT: u16 = 8;
const CSR_PRIVILEGE_MASK: u16 = 0b11;
const MACHINE_CSR_PRIVILEGE: u16 = 0b11;
const CSR_READ_ONLY_SHIFT: u16 = 10;
const CSR_READ_ONLY_MASK: u16 = 0b11;
const CSR_READ_ONLY: u16 = 0b11;
const COUNTER_CY_BIT: u64 = 1 << 0;
const COUNTER_TM_BIT: u64 = 1 << 1;
const COUNTER_IR_BIT: u64 = 1 << 2;
const HPM_COUNTER_FIRST: u16 = 3;
const HPM_COUNTER_COUNT: usize = 3;
const HPM_COUNTER_BITS: u64 = ((1 << HPM_COUNTER_COUNT) - 1) << HPM_COUNTER_FIRST;
const COUNTER_ENABLE_MASK: u64 =
    COUNTER_CY_BIT | COUNTER_TM_BIT | COUNTER_IR_BIT | HPM_COUNTER_BITS;
const MCOUNTINHIBIT_CY_BIT: u64 = 1 << 0;
const MCOUNTINHIBIT_IR_BIT: u64 = 1 << 2;
const MCOUNTINHIBIT_MASK: u64 = MCOUNTINHIBIT_CY_BIT | MCOUNTINHIBIT_IR_BIT | HPM_COUNTER_BITS;
const HPM_EVENT_BITS: u32 = 4;
const HPM_EVENT_MASK: u64 = (1 << HPM_EVENT_BITS) - 1;
pub(crate) const HPM_EVENT_INSTRUCTION: u8 = 1 << 0;
pub(crate) const HPM_EVENT_LOAD: u8 = 1 << 1;
pub(crate) const HPM_EVENT_STORE: u8 = 1 << 2;
pub(crate) const HPM_EVENT_BRANCH: u8 = 1 << 3;
const FFLAGS_BITS: u32 = 5;
const FFLAGS_MASK: u64 = (1 << FFLAGS_BITS) - 1;
const FRM_SHIFT: u32 = FFLAGS_BITS;
const FRM_BITS: u32 = 3;
const FRM_VALUE_MASK: u64 = (1 << FRM_BITS) - 1;
const FRM_MASK: u64 = FRM_VALUE_MASK << FRM_SHIFT;
const FCSR_MASK: u64 = FFLAGS_MASK | FRM_MASK;

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
    mip: u64,
    mcounteren: u64,
    mcountinhibit: u64,
    mhpmcounters: [u64; HPM_COUNTER_COUNT],
    mhpmevents: [u64; HPM_COUNTER_COUNT],
    mcycle: u64,
    minstret: u64,
    fcsr: u64,
}

impl Csrs {
    /// Reads a CSR using machine-mode visibility.
    #[cfg(test)]
    pub(crate) fn read(self, address: u16) -> Option<u64> {
        self.read_with_privilege(address, PrivilegeMode::Machine)
    }

    /// Reads an implemented CSR.
    pub(crate) fn read_with_privilege(self, address: u16, privilege: PrivilegeMode) -> Option<u64> {
        if !self.can_read(address, privilege) {
            return None;
        }
        match address {
            inst::CSR_MSTATUS => Some(self.mstatus_value()),
            inst::CSR_MISA => Some(MISA_VALUE),
            inst::CSR_MHARTID => Some(0),
            inst::CSR_MVENDORID | inst::CSR_MARCHID | inst::CSR_MIMPID => Some(0),
            inst::CSR_MTVEC => Some(self.mtvec),
            inst::CSR_MSCRATCH => Some(self.mscratch),
            inst::CSR_MEPC => Some(self.mepc),
            inst::CSR_MCAUSE => Some(self.mcause),
            inst::CSR_MTVAL => Some(self.mtval),
            inst::CSR_MIE => Some(self.mie),
            inst::CSR_MIP => Some(self.mip),
            inst::CSR_MEDELEG => Some(self.medeleg),
            inst::CSR_MIDELEG => Some(self.mideleg),
            inst::CSR_MCOUNTEREN => Some(self.mcounteren),
            inst::CSR_MCOUNTINHIBIT => Some(self.mcountinhibit),
            inst::CSR_MENVCFG => Some(0),
            inst::CSR_MHPMEVENT3..=inst::CSR_MHPMEVENT31 => self.read_mhpmevent(address),
            inst::CSR_PMPCFG0 => Some(self.pmpcfg0),
            inst::CSR_PMPADDR0 => Some(self.pmpaddr0),
            inst::CSR_MNSTATUS => Some(self.mnstatus),
            inst::CSR_SATP => Some(0),
            inst::CSR_CYCLE | inst::CSR_TIME => Some(self.mcycle),
            inst::CSR_INSTRET => Some(self.minstret),
            inst::CSR_MCYCLE => Some(self.mcycle),
            inst::CSR_MINSTRET => Some(self.minstret),
            inst::CSR_HPMCOUNTER3..=inst::CSR_HPMCOUNTER31 => self.read_hpmcounter(address),
            inst::CSR_MHPMCOUNTER3..=inst::CSR_MHPMCOUNTER31 => self.read_mhpmcounter(address),
            inst::CSR_FFLAGS => Some(self.fcsr & FFLAGS_MASK),
            inst::CSR_FRM => Some((self.fcsr & FRM_MASK) >> FRM_SHIFT),
            inst::CSR_FCSR => Some(self.fcsr),
            _ => None,
        }
    }

    /// Writes a CSR using machine-mode permissions.
    #[cfg(test)]
    pub(crate) fn write(&mut self, address: u16, value: u64) -> bool {
        self.write_with_privilege(address, value, PrivilegeMode::Machine)
    }

    /// Writes an implemented CSR, applying its writable-field rules.
    pub(crate) fn write_with_privilege(
        &mut self,
        address: u16,
        value: u64,
        privilege: PrivilegeMode,
    ) -> bool {
        if matches!(address, inst::CSR_FFLAGS | inst::CSR_FRM | inst::CSR_FCSR) {
            if !self.fp_enabled() {
                return false;
            }
            match address {
                inst::CSR_FFLAGS => {
                    self.fcsr = (self.fcsr & !FFLAGS_MASK) | (value & FFLAGS_MASK);
                }
                inst::CSR_FRM => {
                    self.fcsr = (self.fcsr & !FRM_MASK) | ((value & FRM_VALUE_MASK) << FRM_SHIFT);
                }
                inst::CSR_FCSR => self.fcsr = value & FCSR_MASK,
                _ => unreachable!(),
            }
            self.mark_fp_dirty();
            return true;
        }
        if privilege != PrivilegeMode::Machine || Self::is_read_only(address) {
            return false;
        }
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
            // No interrupt sources are connected, so MIP has no writable bits.
            inst::CSR_MIP => {}
            inst::CSR_MEDELEG => self.medeleg = value,
            inst::CSR_MIDELEG => self.mideleg = value,
            inst::CSR_MCOUNTEREN => self.mcounteren = value & COUNTER_ENABLE_MASK,
            inst::CSR_MCOUNTINHIBIT => self.mcountinhibit = value & MCOUNTINHIBIT_MASK,
            inst::CSR_MENVCFG => {}
            inst::CSR_MHPMEVENT3..=inst::CSR_MHPMEVENT31 => {
                self.write_mhpmevent(address, value);
            }
            inst::CSR_PMPCFG0 => self.pmpcfg0 = value,
            inst::CSR_PMPADDR0 => self.pmpaddr0 = value,
            inst::CSR_MNSTATUS => self.mnstatus = value,
            inst::CSR_MCYCLE => self.mcycle = value,
            inst::CSR_MINSTRET => self.minstret = value,
            inst::CSR_MHPMCOUNTER3..=inst::CSR_MHPMCOUNTER31 => {
                self.write_mhpmcounter(address, value);
            }
            // Only Bare translation is implemented.
            inst::CSR_SATP => {}
            _ => return false,
        }
        true
    }

    fn can_read(self, address: u16, privilege: PrivilegeMode) -> bool {
        match address {
            inst::CSR_CYCLE => {
                privilege == PrivilegeMode::Machine
                    || (privilege == PrivilegeMode::User && self.mcounteren & COUNTER_CY_BIT != 0)
            }
            inst::CSR_TIME => {
                privilege == PrivilegeMode::Machine
                    || (privilege == PrivilegeMode::User && self.mcounteren & COUNTER_TM_BIT != 0)
            }
            inst::CSR_INSTRET => {
                privilege == PrivilegeMode::Machine
                    || (privilege == PrivilegeMode::User && self.mcounteren & COUNTER_IR_BIT != 0)
            }
            inst::CSR_HPMCOUNTER3..=inst::CSR_HPMCOUNTER31 => {
                let index = usize::from(address - inst::CSR_HPMCOUNTER3);
                privilege == PrivilegeMode::Machine
                    || (privilege == PrivilegeMode::User
                        && self.mcounteren & (1 << (HPM_COUNTER_FIRST as usize + index)) != 0)
            }
            inst::CSR_FFLAGS | inst::CSR_FRM | inst::CSR_FCSR => self.fp_enabled(),
            _ => {
                privilege == PrivilegeMode::Machine
                    && (address >> CSR_PRIVILEGE_SHIFT) & CSR_PRIVILEGE_MASK
                        == MACHINE_CSR_PRIVILEGE
            }
        }
    }

    fn is_read_only(address: u16) -> bool {
        (address >> CSR_READ_ONLY_SHIFT) & CSR_READ_ONLY_MASK == CSR_READ_ONLY
    }

    /// Whether `mstatus.FS` enables floating-point register accesses.
    pub(crate) fn fp_enabled(self) -> bool {
        self.mstatus & MSTATUS_FS_MASK != 0
    }

    /// Marks floating-point register state dirty.
    pub(crate) fn mark_fp_dirty(&mut self) {
        if self.fp_enabled() {
            self.mstatus = (self.mstatus & !MSTATUS_FS_MASK) | MSTATUS_FS_DIRTY;
        }
    }

    fn mstatus_value(self) -> u64 {
        let dirty = if self.mstatus & MSTATUS_FS_MASK == MSTATUS_FS_DIRTY {
            MSTATUS_SD
        } else {
            0
        };
        self.mstatus | MSTATUS_FIXED | dirty
    }

    /// Advances the cycle counter unless software inhibits it.
    pub(crate) fn advance_cycle(&mut self) {
        if self.mcountinhibit & MCOUNTINHIBIT_CY_BIT == 0 {
            self.mcycle = self.mcycle.wrapping_add(1);
        }
    }

    /// Retires one instruction in the instruction counter.
    pub(crate) fn retire_instruction(&mut self) {
        if self.mcountinhibit & MCOUNTINHIBIT_IR_BIT == 0 {
            self.minstret = self.minstret.wrapping_add(1);
        }
    }

    /// Records retirement events in the enabled hardware performance counters.
    pub(crate) fn record_retirement(&mut self, events: u8, written_csr: Option<u16>) {
        if written_csr != Some(inst::CSR_MINSTRET) {
            self.retire_instruction();
        }
        for (index, (counter, selector)) in self
            .mhpmcounters
            .iter_mut()
            .zip(self.mhpmevents.iter())
            .enumerate()
        {
            let inhibit = 1 << (HPM_COUNTER_FIRST as usize + index);
            let written = (inst::CSR_MHPMCOUNTER3 as usize + index)
                == written_csr.map_or(usize::MAX, usize::from);
            if !written && self.mcountinhibit & inhibit == 0 && selector & u64::from(events) != 0 {
                *counter = counter.wrapping_add(1);
            }
        }
    }

    fn read_mhpmevent(self, address: u16) -> Option<u64> {
        self.hpm_index(address, inst::CSR_MHPMEVENT3)
            .map_or(Some(0), |index| Some(self.mhpmevents[index]))
    }

    fn read_hpmcounter(self, address: u16) -> Option<u64> {
        self.hpm_index(address, inst::CSR_HPMCOUNTER3)
            .map_or(Some(0), |index| Some(self.mhpmcounters[index]))
    }

    fn read_mhpmcounter(self, address: u16) -> Option<u64> {
        self.hpm_index(address, inst::CSR_MHPMCOUNTER3)
            .map_or(Some(0), |index| Some(self.mhpmcounters[index]))
    }

    fn write_mhpmevent(&mut self, address: u16, value: u64) {
        if let Some(index) = self.hpm_index(address, inst::CSR_MHPMEVENT3) {
            self.mhpmevents[index] = value & HPM_EVENT_MASK;
        }
    }

    fn write_mhpmcounter(&mut self, address: u16, value: u64) {
        if let Some(index) = self.hpm_index(address, inst::CSR_MHPMCOUNTER3) {
            self.mhpmcounters[index] = value;
        }
    }

    fn hpm_index(self, address: u16, first_address: u16) -> Option<usize> {
        let index = usize::from(address.checked_sub(first_address)?);
        (index < HPM_COUNTER_COUNT).then_some(index)
    }

    /// Records a synchronous machine-mode trap.
    pub(crate) fn enter_trap(&mut self, epc: u64, cause: u64, tval: u64, previous: PrivilegeMode) {
        if self.mstatus & MSTATUS_MIE != 0 {
            self.mstatus |= MSTATUS_MPIE;
        } else {
            self.mstatus &= !MSTATUS_MPIE;
        }
        self.mstatus &= !MSTATUS_MIE;
        self.mstatus =
            (self.mstatus & !MSTATUS_MPP_MASK) | (previous.mpp_bits() << MSTATUS_MPP_SHIFT);

        self.mepc = epc & MEPC_ALIGNMENT_MASK;
        self.mcause = cause;
        self.mtval = tval;
    }

    /// Restores machine state for `mret` and returns the saved PC.
    pub(crate) fn return_from_m_trap(&mut self) -> (u64, PrivilegeMode) {
        let previous = self.mpp_mode();
        if self.mstatus & MSTATUS_MPIE != 0 {
            self.mstatus |= MSTATUS_MIE;
        } else {
            self.mstatus &= !MSTATUS_MIE;
        }
        self.mstatus |= MSTATUS_MPIE;
        self.mstatus &= !MSTATUS_MPP_MASK;
        (self.mepc, previous)
    }

    /// Address entered by a synchronous trap.
    pub(crate) fn trap_vector(self) -> u64 {
        self.mtvec & MTVEC_ALIGNMENT_MASK
    }

    /// PC saved by the most recent trap.
    pub(crate) fn exception_pc(self) -> u64 {
        self.mepc
    }

    fn mpp_mode(self) -> PrivilegeMode {
        if (self.mstatus & MSTATUS_MPP_MASK)
            == (PrivilegeMode::Machine.mpp_bits() << MSTATUS_MPP_SHIFT)
        {
            PrivilegeMode::Machine
        } else {
            PrivilegeMode::User
        }
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

        assert_eq!(
            csrs.read(inst::CSR_MSTATUS),
            Some(MSTATUS_WRITABLE | MSTATUS_FIXED | MSTATUS_SD)
        );
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
        assert_eq!(csrs.read(inst::CSR_SATP), None);
    }

    #[test]
    fn unsupported_csrs_are_rejected() {
        let mut csrs = Csrs::default();

        assert_eq!(csrs.read(inst::CSR_MIP), Some(0));
        assert!(csrs.write(inst::CSR_MIP, 1));
        assert_eq!(csrs.read(inst::CSR_MIP), Some(0));
    }

    #[test]
    fn misa_is_a_fixed_rv64imafdcbu_warl() {
        let mut csrs = Csrs::default();

        assert_eq!(csrs.read(inst::CSR_MISA), Some(MISA_VALUE));
        assert!(csrs.write(inst::CSR_MISA, u64::MAX));
        assert_eq!(csrs.read(inst::CSR_MISA), Some(MISA_VALUE));
    }

    #[test]
    fn trap_entry_and_return_preserve_machine_interrupt_state() {
        let mut csrs = Csrs::default();
        csrs.write(inst::CSR_MSTATUS, MSTATUS_MIE | MSTATUS_MPP_MASK);

        csrs.enter_trap(0x8000_0004, 11, 0, PrivilegeMode::Machine);

        let status = csrs.read(inst::CSR_MSTATUS).unwrap();
        assert_eq!(status & MSTATUS_MIE, 0);
        assert_eq!(status & MSTATUS_MPIE, MSTATUS_MPIE);
        assert_eq!(
            status & MSTATUS_MPP_MASK,
            PrivilegeMode::Machine.mpp_bits() << MSTATUS_MPP_SHIFT
        );
        assert_eq!(csrs.read(inst::CSR_MEPC), Some(0x8000_0004));
        assert_eq!(csrs.read(inst::CSR_MCAUSE), Some(11));
        assert_eq!(csrs.read(inst::CSR_MTVAL), Some(0));

        assert_eq!(
            csrs.return_from_m_trap(),
            (0x8000_0004, PrivilegeMode::Machine)
        );
        let status = csrs.read(inst::CSR_MSTATUS).unwrap();
        assert_eq!(status & MSTATUS_MIE, MSTATUS_MIE);
        assert_eq!(status & MSTATUS_MPIE, MSTATUS_MPIE);
        assert_eq!(status & MSTATUS_MPP_MASK, 0);
    }

    #[test]
    fn user_mode_counter_access_requires_mcounteren() {
        let mut csrs = Csrs::default();

        assert_eq!(
            csrs.read_with_privilege(inst::CSR_CYCLE, PrivilegeMode::User),
            None
        );

        assert!(csrs.write(inst::CSR_MCOUNTEREN, COUNTER_CY_BIT | COUNTER_IR_BIT));
        assert_eq!(
            csrs.read_with_privilege(inst::CSR_CYCLE, PrivilegeMode::User),
            Some(0)
        );
        assert_eq!(
            csrs.read_with_privilege(inst::CSR_INSTRET, PrivilegeMode::User),
            Some(0)
        );
        assert_eq!(
            csrs.read_with_privilege(inst::CSR_TIME, PrivilegeMode::User),
            None
        );
        assert!(!csrs.write_with_privilege(inst::CSR_CYCLE, 1, PrivilegeMode::User));
    }

    #[test]
    fn counters_increment_and_wrap() {
        let mut csrs = Csrs::default();
        csrs.write(inst::CSR_MCYCLE, u64::MAX);
        csrs.write(inst::CSR_MINSTRET, u64::MAX);

        csrs.advance_cycle();
        csrs.retire_instruction();

        assert_eq!(csrs.read(inst::CSR_MCYCLE), Some(0));
        assert_eq!(csrs.read(inst::CSR_MINSTRET), Some(0));
    }

    #[test]
    fn hpm_csrs_apply_masks_and_expose_shadows() {
        let mut csrs = Csrs::default();

        assert!(csrs.write(inst::CSR_MHPMEVENT3, u64::MAX));
        assert_eq!(csrs.read(inst::CSR_MHPMEVENT3), Some(HPM_EVENT_MASK));
        assert!(csrs.write(inst::CSR_MHPMCOUNTER3, 0x0123_4567_89ab_cdef));
        assert_eq!(
            csrs.read(inst::CSR_MHPMCOUNTER3),
            Some(0x0123_4567_89ab_cdef)
        );
        assert_eq!(
            csrs.read(inst::CSR_HPMCOUNTER3),
            Some(0x0123_4567_89ab_cdef)
        );

        assert!(csrs.write(inst::CSR_MHPMEVENT6, u64::MAX));
        assert!(csrs.write(inst::CSR_MHPMCOUNTER6, u64::MAX));
        assert_eq!(csrs.read(inst::CSR_MHPMEVENT6), Some(0));
        assert_eq!(csrs.read(inst::CSR_MHPMCOUNTER6), Some(0));
        assert_eq!(csrs.read(inst::CSR_HPMCOUNTER31), Some(0));

        assert!(csrs.write(inst::CSR_MCOUNTEREN, u64::MAX));
        assert_eq!(csrs.read(inst::CSR_MCOUNTEREN), Some(COUNTER_ENABLE_MASK));
        assert!(csrs.write(inst::CSR_MCOUNTINHIBIT, u64::MAX));
        assert_eq!(csrs.read(inst::CSR_MCOUNTINHIBIT), Some(MCOUNTINHIBIT_MASK));
    }

    #[test]
    fn hpm_counters_record_matching_events_and_wrap() {
        let mut csrs = Csrs::default();
        assert!(csrs.write(
            inst::CSR_MHPMEVENT3,
            u64::from(HPM_EVENT_INSTRUCTION | HPM_EVENT_LOAD)
        ));
        assert!(csrs.write(inst::CSR_MHPMEVENT4, u64::from(HPM_EVENT_STORE)));
        assert!(csrs.write(
            inst::CSR_MHPMEVENT5,
            u64::from(HPM_EVENT_INSTRUCTION | HPM_EVENT_BRANCH)
        ));
        assert!(csrs.write(inst::CSR_MHPMCOUNTER3, u64::MAX));

        csrs.record_retirement(HPM_EVENT_INSTRUCTION | HPM_EVENT_LOAD, None);
        csrs.record_retirement(HPM_EVENT_INSTRUCTION | HPM_EVENT_BRANCH, None);

        assert_eq!(csrs.read(inst::CSR_MHPMCOUNTER3), Some(1));
        assert_eq!(csrs.read(inst::CSR_MHPMCOUNTER4), Some(0));
        assert_eq!(csrs.read(inst::CSR_MHPMCOUNTER5), Some(2));
        assert_eq!(csrs.read(inst::CSR_MINSTRET), Some(2));

        csrs.record_retirement(HPM_EVENT_INSTRUCTION, Some(inst::CSR_MHPMCOUNTER3));
        assert_eq!(csrs.read(inst::CSR_MHPMCOUNTER3), Some(1));
        assert_eq!(csrs.read(inst::CSR_MINSTRET), Some(3));
    }

    #[test]
    fn fcsr_aliases_require_enabled_state_and_set_fs_dirty() {
        let mut csrs = Csrs::default();

        assert!(!csrs.write(inst::CSR_FCSR, u64::MAX));
        assert_eq!(csrs.read(inst::CSR_FCSR), None);

        assert!(csrs.write(inst::CSR_MSTATUS, 0b01 << MSTATUS_FS_SHIFT));
        assert_eq!(csrs.read(inst::CSR_MSTATUS).unwrap() & MSTATUS_SD, 0);
        assert!(csrs.write_with_privilege(inst::CSR_FFLAGS, 0x1f, PrivilegeMode::User));
        assert!(csrs.write_with_privilege(inst::CSR_FRM, 0x5, PrivilegeMode::User));

        assert_eq!(csrs.read(inst::CSR_FFLAGS), Some(0x1f));
        assert_eq!(csrs.read(inst::CSR_FRM), Some(0x5));
        assert_eq!(csrs.read(inst::CSR_FCSR), Some(0xbf));
        let status = csrs.read(inst::CSR_MSTATUS).unwrap();
        assert_eq!(status & MSTATUS_FS_MASK, MSTATUS_FS_DIRTY);
        assert_eq!(status & MSTATUS_SD, MSTATUS_SD);
    }
}
