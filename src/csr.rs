//! Control and status registers for M, S, and U modes.

use crate::cpu::PrivilegeMode;
use crate::inst;

const MEDIUM_INSTRUCTION_BYTES: u64 = 4;
const MTVEC_ALIGNMENT_MASK: u64 = !(MEDIUM_INSTRUCTION_BYTES - 1);
const MEPC_ALIGNMENT_MASK: u64 = !1;
const MSTATUS_UXL_SHIFT: u32 = 32;
const MSTATUS_UXL_64: u64 = 2;
const MSTATUS_UXL: u64 = MSTATUS_UXL_64 << MSTATUS_UXL_SHIFT;
const MSTATUS_SXL_SHIFT: u32 = 34;
const MSTATUS_SXL_64: u64 = 2;
const MSTATUS_SXL: u64 = MSTATUS_SXL_64 << MSTATUS_SXL_SHIFT;
const MSTATUS_SIE: u64 = 1 << 1;
const MSTATUS_MIE: u64 = 1 << 3;
const MSTATUS_SPIE: u64 = 1 << 5;
const MSTATUS_MPIE: u64 = 1 << 7;
const MSTATUS_SPP: u64 = 1 << 8;
const MSTATUS_MPP_SHIFT: u32 = 11;
const MSTATUS_MPP_MASK: u64 = 0b11 << MSTATUS_MPP_SHIFT;
const MSTATUS_FS_SHIFT: u32 = 13;
const MSTATUS_FS_MASK: u64 = 0b11 << MSTATUS_FS_SHIFT;
const MSTATUS_FS_DIRTY: u64 = 0b11 << MSTATUS_FS_SHIFT;
const MSTATUS_MPRV: u64 = 1 << 17;
const MSTATUS_SUM: u64 = 1 << 18;
const MSTATUS_MXR: u64 = 1 << 19;
const MSTATUS_TVM: u64 = 1 << 20;
const MSTATUS_TW: u64 = 1 << 21;
const MSTATUS_TSR: u64 = 1 << 22;
const MSTATUS_SD: u64 = 1 << 63;
const MSTATUS_WRITABLE: u64 = MSTATUS_SIE
    | MSTATUS_MIE
    | MSTATUS_SPIE
    | MSTATUS_MPIE
    | MSTATUS_SPP
    | MSTATUS_MPP_MASK
    | MSTATUS_FS_MASK
    | MSTATUS_MPRV
    | MSTATUS_SUM
    | MSTATUS_MXR
    | MSTATUS_TVM
    | MSTATUS_TW
    | MSTATUS_TSR;
const MSTATUS_FIXED: u64 = MSTATUS_UXL | MSTATUS_SXL;
const SSTATUS_WRITABLE: u64 =
    MSTATUS_SIE | MSTATUS_SPIE | MSTATUS_SPP | MSTATUS_FS_MASK | MSTATUS_SUM | MSTATUS_MXR;
const SSTATUS_VISIBLE: u64 = SSTATUS_WRITABLE | MSTATUS_UXL | MSTATUS_SD;
const MXL_SHIFT: u32 = 62;
const MXL_64: u64 = 2;
const MISA_EXT_A_SHIFT: u32 = 0;
const MISA_EXT_B_SHIFT: u32 = 1;
const MISA_EXT_C_SHIFT: u32 = 2;
const MISA_EXT_D_SHIFT: u32 = 3;
const MISA_EXT_F_SHIFT: u32 = 5;
const MISA_EXT_I_SHIFT: u32 = 8;
const MISA_EXT_M_SHIFT: u32 = 12;
const MISA_EXT_S_SHIFT: u32 = 18;
const MISA_EXT_U_SHIFT: u32 = 20;
const MISA_VALUE: u64 = (MXL_64 << MXL_SHIFT)
    | (1 << MISA_EXT_A_SHIFT)
    | (1 << MISA_EXT_B_SHIFT)
    | (1 << MISA_EXT_C_SHIFT)
    | (1 << MISA_EXT_D_SHIFT)
    | (1 << MISA_EXT_F_SHIFT)
    | (1 << MISA_EXT_I_SHIFT)
    | (1 << MISA_EXT_M_SHIFT)
    | (1 << MISA_EXT_S_SHIFT)
    | (1 << MISA_EXT_U_SHIFT);

const CSR_PRIVILEGE_SHIFT: u16 = 8;
const CSR_PRIVILEGE_MASK: u16 = 0b11;
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
const INTERRUPT_SSIP: u64 = 1 << 1;
const INTERRUPT_MSIP: u64 = 1 << 3;
const INTERRUPT_STIP: u64 = 1 << 5;
const INTERRUPT_MTIP: u64 = 1 << 7;
const INTERRUPT_SEIP: u64 = 1 << 9;
const INTERRUPT_MEIP: u64 = 1 << 11;
const INTERRUPT_MASK: u64 = INTERRUPT_SSIP
    | INTERRUPT_MSIP
    | INTERRUPT_STIP
    | INTERRUPT_MTIP
    | INTERRUPT_SEIP
    | INTERRUPT_MEIP;
const SUPERVISOR_INTERRUPT_MASK: u64 = INTERRUPT_SSIP | INTERRUPT_STIP | INTERRUPT_SEIP;
const MIP_WRITABLE: u64 = INTERRUPT_SSIP | INTERRUPT_STIP | INTERRUPT_SEIP;
const SUPERVISOR_SOFTWARE_INTERRUPT: u64 = 1;
const SUPERVISOR_TIMER_INTERRUPT: u64 = 5;
const SUPERVISOR_EXTERNAL_INTERRUPT: u64 = 9;
const INTERRUPT_CAUSE_BIT: u64 = 1 << 63;
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
const MENVCFG_CBIE_SHIFT: u32 = 4;
const MENVCFG_CBIE_MASK: u64 = 0b11 << MENVCFG_CBIE_SHIFT;
const MENVCFG_CBIE_RESERVED: u64 = 0b10 << MENVCFG_CBIE_SHIFT;
const MENVCFG_CBCFE: u64 = 1 << 6;
const MENVCFG_CBZE: u64 = 1 << 7;
const MENVCFG_PBMTE_SHIFT: u32 = 62;
const MENVCFG_PBMTE: u64 = 1 << MENVCFG_PBMTE_SHIFT;
const MENVCFG_WRITABLE: u64 = MENVCFG_CBIE_MASK | MENVCFG_CBCFE | MENVCFG_CBZE | MENVCFG_PBMTE;
const SENVCFG_WRITABLE: u64 = MENVCFG_CBIE_MASK | MENVCFG_CBCFE | MENVCFG_CBZE;
const SATP_MODE_SHIFT: u32 = 60;
const SATP_MODE_MASK: u64 = 0xf << SATP_MODE_SHIFT;
const SATP_SV39_MODE: u64 = 8;
const SATP_ASID_SHIFT: u32 = 44;
const SATP_ASID_MASK: u64 = (1 << 16) - 1;
const SATP_PPN_MASK: u64 = (1 << 44) - 1;

const fn exception_bit(cause: u8) -> u64 {
    1_u64 << cause
}

const MEDELEG_WRITABLE: u64 = exception_bit(inst::CAUSE_MISALIGNED_FETCH)
    | exception_bit(inst::CAUSE_FETCH_ACCESS)
    | exception_bit(inst::CAUSE_ILLEGAL_INSTRUCTION)
    | exception_bit(inst::CAUSE_BREAKPOINT)
    | exception_bit(inst::CAUSE_MISALIGNED_LOAD)
    | exception_bit(inst::CAUSE_LOAD_ACCESS)
    | exception_bit(inst::CAUSE_MISALIGNED_STORE)
    | exception_bit(inst::CAUSE_STORE_ACCESS)
    | exception_bit(inst::CAUSE_USER_ECALL)
    | exception_bit(inst::CAUSE_SUPERVISOR_ECALL)
    | exception_bit(inst::CAUSE_FETCH_PAGE_FAULT)
    | exception_bit(inst::CAUSE_LOAD_PAGE_FAULT)
    | exception_bit(inst::CAUSE_STORE_PAGE_FAULT);

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
    scounteren: u64,
    mcountinhibit: u64,
    menvcfg: u64,
    senvcfg: u64,
    stvec: u64,
    sscratch: u64,
    sepc: u64,
    scause: u64,
    stval: u64,
    satp: u64,
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
            inst::CSR_SCOUNTEREN => Some(self.scounteren),
            inst::CSR_MCOUNTINHIBIT => Some(self.mcountinhibit),
            inst::CSR_MENVCFG => Some(self.menvcfg),
            inst::CSR_SENVCFG => Some(self.senvcfg),
            inst::CSR_SSTATUS => Some(self.sstatus_value()),
            inst::CSR_SIE => Some(self.mie & self.mideleg),
            inst::CSR_SIP => Some(self.mip & self.mideleg),
            inst::CSR_STVEC => Some(self.stvec),
            inst::CSR_SSCRATCH => Some(self.sscratch),
            inst::CSR_SEPC => Some(self.sepc),
            inst::CSR_SCAUSE => Some(self.scause),
            inst::CSR_STVAL => Some(self.stval),
            inst::CSR_SATP => Some(self.satp),
            inst::CSR_MHPMEVENT3..=inst::CSR_MHPMEVENT31 => self.read_mhpmevent(address),
            inst::CSR_PMPCFG0 => Some(self.pmpcfg0),
            inst::CSR_PMPADDR0 => Some(self.pmpaddr0),
            inst::CSR_MNSTATUS => Some(self.mnstatus),
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
        if Self::is_read_only(address) || !self.can_write(address, privilege) {
            return false;
        }
        match address {
            inst::CSR_MSTATUS => self.mstatus = Self::mstatus_writable(value),
            inst::CSR_SSTATUS => {
                self.mstatus = (self.mstatus & !SSTATUS_WRITABLE) | (value & SSTATUS_WRITABLE);
            }
            // MXL and the implemented extension bits are fixed, so WARL writes
            // are accepted and ignored.
            inst::CSR_MISA => {}
            inst::CSR_MTVEC => self.mtvec = value & MTVEC_ALIGNMENT_MASK,
            inst::CSR_MSCRATCH => self.mscratch = value,
            inst::CSR_MEPC => self.mepc = value & MEPC_ALIGNMENT_MASK,
            inst::CSR_MCAUSE => self.mcause = value,
            inst::CSR_MTVAL => self.mtval = value,
            inst::CSR_MIE => self.mie = value & INTERRUPT_MASK,
            inst::CSR_SIE => {
                let delegated = self.mideleg & SUPERVISOR_INTERRUPT_MASK;
                self.mie = (self.mie & !delegated) | (value & delegated);
            }
            inst::CSR_MIP => {
                self.mip = (self.mip & !MIP_WRITABLE) | (value & MIP_WRITABLE);
            }
            inst::CSR_SIP => {
                let mask = self.mideleg & MIP_WRITABLE;
                self.mip = (self.mip & !mask) | (value & mask);
            }
            inst::CSR_MEDELEG => self.medeleg = value & MEDELEG_WRITABLE,
            inst::CSR_MIDELEG => self.mideleg = value & SUPERVISOR_INTERRUPT_MASK,
            inst::CSR_MCOUNTEREN => self.mcounteren = value & COUNTER_ENABLE_MASK,
            inst::CSR_SCOUNTEREN => self.scounteren = value & COUNTER_ENABLE_MASK,
            inst::CSR_MCOUNTINHIBIT => self.mcountinhibit = value & MCOUNTINHIBIT_MASK,
            inst::CSR_MENVCFG => self.menvcfg = Self::menvcfg_value(value & MENVCFG_WRITABLE),
            inst::CSR_SENVCFG => {
                self.senvcfg = Self::menvcfg_value(value & SENVCFG_WRITABLE);
            }
            inst::CSR_STVEC => self.stvec = value & MTVEC_ALIGNMENT_MASK,
            inst::CSR_SSCRATCH => self.sscratch = value,
            inst::CSR_SEPC => self.sepc = value & MEPC_ALIGNMENT_MASK,
            inst::CSR_SCAUSE => self.scause = value,
            inst::CSR_STVAL => self.stval = value,
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
            inst::CSR_SATP => {
                if let Some(satp) = Self::satp_value(value) {
                    self.satp = satp;
                }
            }
            _ => return false,
        }
        true
    }

    fn can_read(self, address: u16, privilege: PrivilegeMode) -> bool {
        match address {
            inst::CSR_CYCLE => self.counter_readable(privilege, COUNTER_CY_BIT),
            inst::CSR_TIME => self.counter_readable(privilege, COUNTER_TM_BIT),
            inst::CSR_INSTRET => self.counter_readable(privilege, COUNTER_IR_BIT),
            inst::CSR_HPMCOUNTER3..=inst::CSR_HPMCOUNTER31 => {
                let index = usize::from(address - inst::CSR_HPMCOUNTER3);
                self.counter_readable(privilege, 1 << (HPM_COUNTER_FIRST as usize + index))
            }
            inst::CSR_FFLAGS | inst::CSR_FRM | inst::CSR_FCSR => self.fp_enabled(),
            inst::CSR_SATP
                if privilege == PrivilegeMode::Supervisor && self.mstatus & MSTATUS_TVM != 0 =>
            {
                false
            }
            _ => self.csr_privilege_allows(address, privilege),
        }
    }

    fn can_write(self, address: u16, privilege: PrivilegeMode) -> bool {
        if address == inst::CSR_SATP
            && privilege == PrivilegeMode::Supervisor
            && self.mstatus & MSTATUS_TVM != 0
        {
            return false;
        }
        self.csr_privilege_allows(address, privilege)
    }

    fn csr_privilege_allows(self, address: u16, privilege: PrivilegeMode) -> bool {
        let encoded = ((address >> CSR_PRIVILEGE_SHIFT) & CSR_PRIVILEGE_MASK) as u8;
        let required_rank = match encoded {
            0b11 => PrivilegeMode::Machine.rank(),
            _ => encoded,
        };
        privilege.rank() >= required_rank
    }

    fn counter_readable(self, privilege: PrivilegeMode, bit: u64) -> bool {
        match privilege {
            PrivilegeMode::Machine => true,
            PrivilegeMode::Supervisor => self.mcounteren & bit != 0,
            PrivilegeMode::User => self.mcounteren & bit != 0 && self.scounteren & bit != 0,
        }
    }

    fn satp_value(value: u64) -> Option<u64> {
        let mode = value & SATP_MODE_MASK;
        if mode != 0 && mode != SATP_SV39_MODE << SATP_MODE_SHIFT {
            return None;
        }
        Some(mode | (value & ((SATP_ASID_MASK << SATP_ASID_SHIFT) | SATP_PPN_MASK)))
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

    fn menvcfg_value(value: u64) -> u64 {
        let cbie = value & MENVCFG_CBIE_MASK;
        let canonical_cbie = if cbie == MENVCFG_CBIE_RESERVED {
            0
        } else {
            cbie
        };
        canonical_cbie | (value & (MENVCFG_CBCFE | MENVCFG_CBZE | MENVCFG_PBMTE))
    }

    fn mstatus_writable(value: u64) -> u64 {
        let mut status = value & MSTATUS_WRITABLE;
        let mpp = status & MSTATUS_MPP_MASK;
        if mpp >> MSTATUS_MPP_SHIFT == 0b10 {
            status = (status & !MSTATUS_MPP_MASK)
                | (PrivilegeMode::Supervisor.mpp_bits() << MSTATUS_MPP_SHIFT);
        }
        status
    }

    fn mstatus_value(self) -> u64 {
        let dirty = if self.mstatus & MSTATUS_FS_MASK == MSTATUS_FS_DIRTY {
            MSTATUS_SD
        } else {
            0
        };
        self.mstatus | MSTATUS_FIXED | dirty
    }

    fn sstatus_value(self) -> u64 {
        self.mstatus_value() & SSTATUS_VISIBLE
    }

    /// Whether an exception from S or U mode is delegated to S mode.
    pub(crate) fn delegates_exception(self, cause: u8) -> bool {
        cause < u64::BITS as u8 && self.medeleg & exception_bit(cause) != 0
    }

    /// Highest-priority enabled delegated supervisor interrupt for the current
    /// privilege mode.
    pub(crate) fn pending_supervisor_interrupt(self, privilege: PrivilegeMode) -> Option<u64> {
        if privilege == PrivilegeMode::Machine || self.mstatus & MSTATUS_SIE == 0 {
            return None;
        }
        let pending = self.mip & self.mie & self.mideleg & SUPERVISOR_INTERRUPT_MASK;
        [
            (INTERRUPT_SEIP, SUPERVISOR_EXTERNAL_INTERRUPT),
            (INTERRUPT_SSIP, SUPERVISOR_SOFTWARE_INTERRUPT),
            (INTERRUPT_STIP, SUPERVISOR_TIMER_INTERRUPT),
        ]
        .into_iter()
        .find_map(|(bit, cause)| (pending & bit != 0).then_some(INTERRUPT_CAUSE_BIT | cause))
    }

    /// Whether S-mode accesses to translation-control state are blocked.
    pub(crate) fn tvm(self) -> bool {
        self.mstatus & MSTATUS_TVM != 0
    }

    pub(crate) fn satp(self) -> u64 {
        self.satp
    }

    pub(crate) fn sum(self) -> bool {
        self.mstatus & MSTATUS_SUM != 0
    }

    pub(crate) fn mxr(self) -> bool {
        self.mstatus & MSTATUS_MXR != 0
    }

    /// Whether page-based memory types are enabled for translated accesses.
    pub(crate) fn pbmt_enabled(self) -> bool {
        self.menvcfg & MENVCFG_PBMTE != 0
    }

    /// Effective privilege for an explicit load/store, honoring `MPRV`.
    pub(crate) fn effective_privilege(
        self,
        current: PrivilegeMode,
        explicit_access: bool,
    ) -> PrivilegeMode {
        if current == PrivilegeMode::Machine && explicit_access && self.mstatus & MSTATUS_MPRV != 0
        {
            self.mpp_mode()
        } else {
            current
        }
    }

    /// Whether S-mode `sret` is blocked by `mstatus.TSR`.
    pub(crate) fn tsr(self) -> bool {
        self.mstatus & MSTATUS_TSR != 0
    }

    /// Whether U/S-mode `wfi` is blocked by `mstatus.TW`.
    pub(crate) fn tw(self) -> bool {
        self.mstatus & MSTATUS_TW != 0
    }

    /// Whether S-mode cache-block controls are enabled by `menvcfg`.
    pub(crate) fn supervisor_cache_block_clean_flush_enabled(self) -> bool {
        self.menvcfg & MENVCFG_CBCFE != 0
    }

    /// Whether U-mode cache-block controls are enabled by `menvcfg` and `senvcfg`.
    pub(crate) fn user_cache_block_clean_flush_enabled(self) -> bool {
        self.menvcfg & MENVCFG_CBCFE != 0 && self.senvcfg & MENVCFG_CBCFE != 0
    }

    /// Whether S-mode cache-block zero operations are enabled by `menvcfg`.
    pub(crate) fn supervisor_cache_block_zero_enabled(self) -> bool {
        self.menvcfg & MENVCFG_CBZE != 0
    }

    /// Whether U-mode cache-block zero operations are enabled by both envcfgs.
    pub(crate) fn user_cache_block_zero_enabled(self) -> bool {
        self.menvcfg & MENVCFG_CBZE != 0 && self.senvcfg & MENVCFG_CBZE != 0
    }

    /// Whether S-mode cache-block invalidation is enabled by `menvcfg`.
    pub(crate) fn supervisor_cache_block_invalidate_enabled(self) -> bool {
        self.menvcfg & MENVCFG_CBIE_MASK != 0
    }

    /// Whether U-mode cache-block invalidation is enabled by both envcfgs.
    pub(crate) fn user_cache_block_invalidate_enabled(self) -> bool {
        self.menvcfg & MENVCFG_CBIE_MASK != 0 && self.senvcfg & MENVCFG_CBIE_MASK != 0
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
        if previous != PrivilegeMode::Machine {
            self.mstatus &= !MSTATUS_MPRV;
        }
        (self.mepc, previous)
    }

    /// Records a synchronous trap delegated to S mode.
    pub(crate) fn enter_s_trap(
        &mut self,
        epc: u64,
        cause: u64,
        tval: u64,
        previous: PrivilegeMode,
    ) {
        if self.mstatus & MSTATUS_SIE != 0 {
            self.mstatus |= MSTATUS_SPIE;
        } else {
            self.mstatus &= !MSTATUS_SPIE;
        }
        self.mstatus &= !MSTATUS_SIE;
        if previous == PrivilegeMode::Supervisor {
            self.mstatus |= MSTATUS_SPP;
        } else {
            self.mstatus &= !MSTATUS_SPP;
        }

        self.sepc = epc & MEPC_ALIGNMENT_MASK;
        self.scause = cause;
        self.stval = tval;
    }

    /// Restores supervisor state for `sret` and returns the saved PC.
    pub(crate) fn return_from_s_trap(&mut self) -> (u64, PrivilegeMode) {
        let previous = if self.mstatus & MSTATUS_SPP != 0 {
            PrivilegeMode::Supervisor
        } else {
            PrivilegeMode::User
        };
        if self.mstatus & MSTATUS_SPIE != 0 {
            self.mstatus |= MSTATUS_SIE;
        } else {
            self.mstatus &= !MSTATUS_SIE;
        }
        self.mstatus |= MSTATUS_SPIE;
        self.mstatus &= !MSTATUS_SPP;
        self.mstatus &= !MSTATUS_MPRV;
        (self.sepc, previous)
    }

    /// Address entered by a synchronous trap.
    pub(crate) fn trap_vector(self) -> u64 {
        self.mtvec & MTVEC_ALIGNMENT_MASK
    }

    /// Address entered by a synchronous trap delegated to S mode.
    pub(crate) fn supervisor_trap_vector(self) -> u64 {
        self.stvec & MTVEC_ALIGNMENT_MASK
    }

    /// PC saved by the most recent trap.
    pub(crate) fn exception_pc(self) -> u64 {
        self.mepc
    }

    /// PC saved by the most recent supervisor trap.
    pub(crate) fn supervisor_exception_pc(self) -> u64 {
        self.sepc
    }

    fn mpp_mode(self) -> PrivilegeMode {
        match (self.mstatus & MSTATUS_MPP_MASK) >> MSTATUS_MPP_SHIFT {
            0b01 => PrivilegeMode::Supervisor,
            0b11 => PrivilegeMode::Machine,
            _ => PrivilegeMode::User,
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
        assert!(csrs.write(inst::CSR_SSTATUS, u64::MAX));
        assert!(csrs.write(inst::CSR_STVEC, u64::MAX));
        assert!(csrs.write(inst::CSR_SSCRATCH, u64::MAX));
        assert!(csrs.write(inst::CSR_SEPC, u64::MAX));
        assert!(csrs.write(inst::CSR_SCAUSE, u64::MAX));
        assert!(csrs.write(inst::CSR_STVAL, u64::MAX));
        assert!(csrs.write(inst::CSR_SCOUNTEREN, u64::MAX));
        assert!(csrs.write(inst::CSR_SENVCFG, u64::MAX));
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
        assert_eq!(csrs.read(inst::CSR_MIE), Some(INTERRUPT_MASK));
        assert_eq!(csrs.read(inst::CSR_MEDELEG), Some(MEDELEG_WRITABLE));
        assert_eq!(
            csrs.read(inst::CSR_MIDELEG),
            Some(SUPERVISOR_INTERRUPT_MASK)
        );
        assert_eq!(
            csrs.read(inst::CSR_SSTATUS),
            Some(SSTATUS_WRITABLE | MSTATUS_UXL | MSTATUS_SD)
        );
        assert_eq!(csrs.read(inst::CSR_STVEC), Some(MTVEC_ALIGNMENT_MASK));
        assert_eq!(csrs.read(inst::CSR_SSCRATCH), Some(u64::MAX));
        assert_eq!(csrs.read(inst::CSR_SEPC), Some(MEPC_ALIGNMENT_MASK));
        assert_eq!(csrs.read(inst::CSR_SCAUSE), Some(u64::MAX));
        assert_eq!(csrs.read(inst::CSR_STVAL), Some(u64::MAX));
        assert_eq!(csrs.read(inst::CSR_SCOUNTEREN), Some(COUNTER_ENABLE_MASK));
        assert_eq!(
            csrs.read(inst::CSR_SENVCFG),
            Some(MENVCFG_CBIE_MASK | MENVCFG_CBCFE | MENVCFG_CBZE)
        );
        assert_eq!(csrs.read(inst::CSR_PMPCFG0), Some(u64::MAX));
        assert_eq!(csrs.read(inst::CSR_PMPADDR0), Some(u64::MAX));
        assert_eq!(csrs.read(inst::CSR_MNSTATUS), Some(u64::MAX));
        assert_eq!(csrs.read(inst::CSR_SATP), Some(0));
    }

    #[test]
    fn unsupported_csrs_are_rejected() {
        let mut csrs = Csrs::default();

        assert_eq!(csrs.read(inst::CSR_MIP), Some(0));
        assert!(csrs.write(inst::CSR_MIP, 1));
        assert_eq!(csrs.read(inst::CSR_MIP), Some(0));
    }

    #[test]
    fn mip_and_sip_write_only_writable_pending_bits_and_preserve_the_rest() {
        let mut csrs = Csrs::default();
        let supervisor_pending = INTERRUPT_STIP | INTERRUPT_SEIP;
        let machine_pending = INTERRUPT_MSIP | INTERRUPT_MTIP | INTERRUPT_MEIP;

        assert!(csrs.write(inst::CSR_MIP, supervisor_pending));
        assert_eq!(csrs.read(inst::CSR_MIP), Some(supervisor_pending));

        csrs.mip |= machine_pending;
        assert!(csrs.write(inst::CSR_MIDELEG, SUPERVISOR_INTERRUPT_MASK));
        assert!(csrs.write_with_privilege(
            inst::CSR_SIP,
            INTERRUPT_STIP,
            PrivilegeMode::Supervisor
        ));
        assert_eq!(
            csrs.read(inst::CSR_MIP),
            Some(INTERRUPT_STIP | machine_pending)
        );

        assert!(csrs.write(inst::CSR_MIP, INTERRUPT_STIP));
        assert!(csrs.write(inst::CSR_MIDELEG, INTERRUPT_STIP));
        assert!(csrs.write_with_privilege(
            inst::CSR_SIP,
            INTERRUPT_SSIP | INTERRUPT_STIP | INTERRUPT_SEIP,
            PrivilegeMode::Supervisor
        ));
        assert_eq!(
            csrs.read(inst::CSR_MIP),
            Some(INTERRUPT_STIP | machine_pending)
        );
    }

    #[test]
    fn enabled_delegated_supervisor_interrupts_use_architectural_priority() {
        let mut csrs = Csrs::default();
        assert!(csrs.write(inst::CSR_MSTATUS, MSTATUS_SIE));
        assert!(csrs.write(inst::CSR_MIDELEG, SUPERVISOR_INTERRUPT_MASK));
        assert!(csrs.write(inst::CSR_MIE, SUPERVISOR_INTERRUPT_MASK));
        assert!(csrs.write(
            inst::CSR_MIP,
            INTERRUPT_SSIP | INTERRUPT_STIP | INTERRUPT_SEIP
        ));

        assert_eq!(
            csrs.pending_supervisor_interrupt(PrivilegeMode::Supervisor),
            Some(INTERRUPT_CAUSE_BIT | SUPERVISOR_EXTERNAL_INTERRUPT)
        );
        assert_eq!(
            csrs.pending_supervisor_interrupt(PrivilegeMode::Machine),
            None
        );
    }

    #[test]
    fn misa_is_a_fixed_rv64imafdcbsu_warl() {
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
    fn supervisor_csrs_alias_machine_state_and_canonicalize_satp() {
        let mut csrs = Csrs::default();
        let writable = MSTATUS_SIE | MSTATUS_SPIE | MSTATUS_SPP;

        assert!(csrs.write_with_privilege(
            inst::CSR_SSTATUS,
            writable | MSTATUS_MIE | MSTATUS_MPRV,
            PrivilegeMode::Supervisor
        ));
        assert_eq!(
            csrs.read_with_privilege(inst::CSR_SSTATUS, PrivilegeMode::Supervisor),
            Some(writable | MSTATUS_UXL)
        );
        assert_eq!(
            csrs.read_with_privilege(inst::CSR_MSTATUS, PrivilegeMode::Machine)
                .unwrap()
                & (MSTATUS_MIE | MSTATUS_MPRV),
            0
        );

        assert!(csrs.write_with_privilege(inst::CSR_SATP, u64::MAX, PrivilegeMode::Supervisor));
        assert_eq!(
            csrs.read_with_privilege(inst::CSR_SATP, PrivilegeMode::Supervisor),
            Some(0)
        );
        assert!(csrs.write(inst::CSR_MSTATUS, MSTATUS_TVM));
        assert_eq!(
            csrs.read(inst::CSR_MSTATUS).unwrap() & MSTATUS_TVM,
            MSTATUS_TVM
        );
        assert_eq!(
            csrs.read_with_privilege(inst::CSR_SATP, PrivilegeMode::Supervisor),
            None
        );
        assert!(!csrs.write_with_privilege(inst::CSR_SATP, 0, PrivilegeMode::Supervisor));
        assert_eq!(
            csrs.read_with_privilege(inst::CSR_SATP, PrivilegeMode::Machine),
            Some(0)
        );
    }

    #[test]
    fn satp_retains_sv39_mode_asid_and_ppn() {
        let mut csrs = Csrs::default();
        let satp =
            SATP_SV39_MODE << SATP_MODE_SHIFT | 0xabcd_u64 << SATP_ASID_SHIFT | 0x123_4567_89ab;

        assert!(csrs.write(inst::CSR_SATP, satp));
        assert_eq!(csrs.read(inst::CSR_SATP), Some(satp));
        for mode in (0_u64..=0xf).filter(|mode| *mode != 0 && *mode != SATP_SV39_MODE) {
            assert!(csrs.write(
                inst::CSR_SATP,
                mode << SATP_MODE_SHIFT | 0xffff_ffff_ffff_ffff
            ));
            assert_eq!(csrs.read(inst::CSR_SATP), Some(satp), "mode {mode}");
        }
    }

    #[test]
    fn supervisor_trap_entry_and_return_preserve_supervisor_state() {
        let mut csrs = Csrs::default();
        csrs.write(inst::CSR_MSTATUS, MSTATUS_SIE | MSTATUS_MPRV);

        csrs.enter_s_trap(0x8000_0008, 9, 0x1234, PrivilegeMode::Supervisor);

        let status = csrs.read(inst::CSR_MSTATUS).unwrap();
        assert_eq!(status & MSTATUS_SIE, 0);
        assert_eq!(status & MSTATUS_SPIE, MSTATUS_SPIE);
        assert_eq!(status & MSTATUS_SPP, MSTATUS_SPP);
        assert_eq!(csrs.read(inst::CSR_SEPC), Some(0x8000_0008));
        assert_eq!(csrs.read(inst::CSR_SCAUSE), Some(9));
        assert_eq!(csrs.read(inst::CSR_STVAL), Some(0x1234));

        assert_eq!(
            csrs.return_from_s_trap(),
            (0x8000_0008, PrivilegeMode::Supervisor)
        );
        let status = csrs.read(inst::CSR_MSTATUS).unwrap();
        assert_eq!(status & MSTATUS_SIE, MSTATUS_SIE);
        assert_eq!(status & MSTATUS_SPIE, MSTATUS_SPIE);
        assert_eq!(status & MSTATUS_SPP, 0);
        assert_eq!(status & MSTATUS_MPRV, 0);
    }

    #[test]
    fn delegation_masks_only_supported_supervisor_interrupts_and_exceptions() {
        let mut csrs = Csrs::default();
        assert!(csrs.write(inst::CSR_MEDELEG, u64::MAX));
        assert!(csrs.write(inst::CSR_MIDELEG, u64::MAX));

        assert_eq!(csrs.read(inst::CSR_MEDELEG), Some(MEDELEG_WRITABLE));
        assert_eq!(
            csrs.read(inst::CSR_MIDELEG),
            Some(SUPERVISOR_INTERRUPT_MASK)
        );
        assert!(csrs.delegates_exception(inst::CAUSE_ILLEGAL_INSTRUCTION));
        assert!(!csrs.delegates_exception(inst::CAUSE_MACHINE_ECALL));
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
            csrs.read_with_privilege(inst::CSR_CYCLE, PrivilegeMode::Supervisor),
            Some(0)
        );
        assert_eq!(
            csrs.read_with_privilege(inst::CSR_CYCLE, PrivilegeMode::User),
            None
        );
        assert!(csrs.write(inst::CSR_SCOUNTEREN, COUNTER_CY_BIT | COUNTER_IR_BIT));
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
    fn act4_declares_every_retained_hpm_event_selector() {
        const CONFIG: &str = include_str!("../tests/compliance/act4/arvsr-rv64imafdcb.yaml");
        let values = CONFIG
            .lines()
            .find_map(|line| line.trim().strip_prefix("HPM_EVENTS:"))
            .expect("ACT4 config must declare HPM_EVENTS");
        let declared: Vec<u64> = values
            .trim()
            .trim_start_matches('[')
            .trim_end_matches(']')
            .split(',')
            .map(|value| value.trim().parse().expect("numeric HPM event"))
            .collect();
        let mut csrs = Csrs::default();

        for selector in 0..(1_u64 << HPM_EVENT_BITS) {
            assert!(csrs.write(inst::CSR_MHPMEVENT3, selector));
            let retained = csrs.read(inst::CSR_MHPMEVENT3).unwrap();

            assert_eq!(retained, selector);
            assert!(
                declared.contains(&retained),
                "HPM_EVENTS is missing retained selector {retained}"
            );
        }
    }

    #[test]
    fn menvcfg_exposes_cmo_controls_and_canonicalizes_cbie() {
        let mut csrs = Csrs::default();
        assert!(csrs.write(inst::CSR_MENVCFG, u64::MAX));
        assert_eq!(
            csrs.read(inst::CSR_MENVCFG),
            Some(MENVCFG_CBIE_MASK | MENVCFG_CBCFE | MENVCFG_CBZE | MENVCFG_PBMTE)
        );
        assert!(csrs.pbmt_enabled());
        assert!(csrs.supervisor_cache_block_invalidate_enabled());
        assert!(csrs.supervisor_cache_block_clean_flush_enabled());
        assert!(csrs.supervisor_cache_block_zero_enabled());
        assert!(!csrs.user_cache_block_invalidate_enabled());
        assert!(!csrs.user_cache_block_clean_flush_enabled());
        assert!(!csrs.user_cache_block_zero_enabled());

        assert!(csrs.write(inst::CSR_SENVCFG, u64::MAX));
        assert!(csrs.user_cache_block_invalidate_enabled());
        assert!(csrs.user_cache_block_clean_flush_enabled());
        assert!(csrs.user_cache_block_zero_enabled());
        assert_eq!(
            csrs.read(inst::CSR_SENVCFG),
            Some(MENVCFG_CBIE_MASK | MENVCFG_CBCFE | MENVCFG_CBZE)
        );

        assert!(csrs.write(inst::CSR_MENVCFG, 0b10 << MENVCFG_CBIE_SHIFT));
        assert_eq!(csrs.read(inst::CSR_MENVCFG), Some(0));
        assert!(!csrs.pbmt_enabled());
        assert!(!csrs.supervisor_cache_block_invalidate_enabled());
        assert!(!csrs.supervisor_cache_block_clean_flush_enabled());
        assert!(!csrs.supervisor_cache_block_zero_enabled());
        assert!(!csrs.user_cache_block_invalidate_enabled());
        assert!(!csrs.user_cache_block_clean_flush_enabled());
        assert!(!csrs.user_cache_block_zero_enabled());
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
