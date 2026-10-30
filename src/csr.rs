//! Control and status registers for M, S, and U modes.

use crate::bus::InterruptLines;
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
const PMP_ENTRY_COUNT: usize = 16;
const PMP_ENTRIES_PER_CFG: usize = 8;
const PMP_ADDRESS_BITS: u32 = 54;
const PMP_ADDRESS_MASK: u64 = (1 << PMP_ADDRESS_BITS) - 1;
const PMP_CFG_READ: u8 = 1 << 0;
const PMP_CFG_WRITE: u8 = 1 << 1;
const PMP_CFG_EXECUTE: u8 = 1 << 2;
const PMP_CFG_ADDRESS_MASK: u8 = 0b11 << 3;
const PMP_CFG_ADDRESS_OFF: u8 = 0;
const PMP_CFG_ADDRESS_TOR: u8 = 1 << 3;
const PMP_CFG_ADDRESS_NA4: u8 = 2 << 3;
const PMP_CFG_ADDRESS_NAPOT: u8 = 3 << 3;
const PMP_CFG_LOCK: u8 = 1 << 7;
const PMP_CFG_FIELDS: u8 =
    PMP_CFG_READ | PMP_CFG_WRITE | PMP_CFG_EXECUTE | PMP_CFG_ADDRESS_MASK | PMP_CFG_LOCK;
const PMP_SHIFT: u32 = 2;
const NA4_BYTES: u64 = 1 << PMP_SHIFT;
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
const MACHINE_SOFTWARE_INTERRUPT: u64 = 3;
const MACHINE_TIMER_INTERRUPT: u64 = 7;
const MACHINE_EXTERNAL_INTERRUPT: u64 = 11;
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

/// Converts asserted platform interrupt lines into architectural `mip` bits.
pub(crate) fn device_interrupt_mask(lines: InterruptLines) -> u64 {
    let mut pending = 0;
    if lines.machine_software {
        pending |= INTERRUPT_MSIP;
    }
    if lines.machine_timer {
        pending |= INTERRUPT_MTIP;
    }
    if lines.machine_external {
        pending |= INTERRUPT_MEIP;
    }
    if lines.supervisor_software {
        pending |= INTERRUPT_SSIP;
    }
    if lines.supervisor_timer {
        pending |= INTERRUPT_STIP;
    }
    if lines.supervisor_external {
        pending |= INTERRUPT_SEIP;
    }
    pending
}

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
    pmpcfg: [u64; PMP_ENTRY_COUNT / PMP_ENTRIES_PER_CFG],
    pmpaddr: [u64; PMP_ENTRY_COUNT],
    mnstatus: u64,
    mip: u64,
    device_pending: u64,
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
            inst::CSR_MIP => Some(self.pending_mip()),
            inst::CSR_MEDELEG => Some(self.medeleg),
            inst::CSR_MIDELEG => Some(self.mideleg),
            inst::CSR_MCOUNTEREN => Some(self.mcounteren),
            inst::CSR_SCOUNTEREN => Some(self.scounteren),
            inst::CSR_MCOUNTINHIBIT => Some(self.mcountinhibit),
            inst::CSR_MENVCFG => Some(self.menvcfg),
            inst::CSR_SENVCFG => Some(self.senvcfg),
            inst::CSR_SSTATUS => Some(self.sstatus_value()),
            inst::CSR_SIE => Some(self.mie & self.mideleg),
            inst::CSR_SIP => Some(self.pending_mip() & self.mideleg),
            inst::CSR_STVEC => Some(self.stvec),
            inst::CSR_SSCRATCH => Some(self.sscratch),
            inst::CSR_SEPC => Some(self.sepc),
            inst::CSR_SCAUSE => Some(self.scause),
            inst::CSR_STVAL => Some(self.stval),
            inst::CSR_SATP => Some(self.satp),
            inst::CSR_MHPMEVENT3..=inst::CSR_MHPMEVENT31 => self.read_mhpmevent(address),
            inst::CSR_PMPCFG0 => Some(self.pmpcfg[0]),
            inst::CSR_PMPCFG2 => Some(self.pmpcfg[1]),
            inst::CSR_PMPADDR0..=inst::CSR_PMPADDR15 => {
                Some(self.pmpaddr[usize::from(address - inst::CSR_PMPADDR0)])
            }
            inst::CSR_MNSTATUS => Some(self.mnstatus),
            inst::CSR_TCONTROL
            | inst::CSR_TSELECT
            | inst::CSR_TDATA1
            | inst::CSR_TDATA2
            | inst::CSR_TDATA3 => Some(0),
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
            inst::CSR_PMPCFG0 => self.write_pmpcfg(0, value),
            inst::CSR_PMPCFG2 => self.write_pmpcfg(1, value),
            inst::CSR_PMPADDR0..=inst::CSR_PMPADDR15 => {
                let index = usize::from(address - inst::CSR_PMPADDR0);
                if self.pmp_address_writable(index) {
                    self.pmpaddr[index] = value & PMP_ADDRESS_MASK;
                }
            }
            inst::CSR_MNSTATUS => self.mnstatus = value,
            // This hart has no debug triggers. The trigger CSRs remain
            // accessible so software can discover that zero triggers exist.
            inst::CSR_TCONTROL
            | inst::CSR_TSELECT
            | inst::CSR_TDATA1
            | inst::CSR_TDATA2
            | inst::CSR_TDATA3 => {}
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

    fn write_pmpcfg(&mut self, bank: usize, value: u64) {
        let mut updated = self.pmpcfg[bank];
        for byte_index in 0..PMP_ENTRIES_PER_CFG {
            let shift = (byte_index * u8::BITS as usize) as u32;
            let mask = u64::from(u8::MAX) << shift;
            let old = ((self.pmpcfg[bank] & mask) >> shift) as u8;
            if old & PMP_CFG_LOCK != 0 {
                continue;
            }
            let new = ((value & mask) >> shift) as u8;
            updated = (updated & !mask) | (u64::from(canonical_pmp_cfg(new)) << shift);
        }
        self.pmpcfg[bank] = updated;
    }

    fn pmp_config(self, index: usize) -> u8 {
        let bank = index / PMP_ENTRIES_PER_CFG;
        let within = index % PMP_ENTRIES_PER_CFG;
        (self.pmpcfg[bank] >> (within * u8::BITS as usize)) as u8
    }

    fn pmp_entry_locked(self, index: usize) -> bool {
        self.pmp_config(index) & PMP_CFG_LOCK != 0
    }

    fn pmp_address_writable(self, index: usize) -> bool {
        if self.pmp_entry_locked(index) {
            return false;
        }
        index + 1 >= PMP_ENTRY_COUNT
            || !self.pmp_entry_locked(index + 1)
            || self.pmp_config(index + 1) & PMP_CFG_ADDRESS_MASK != PMP_CFG_ADDRESS_TOR
    }

    /// Checks a physical access against the lowest-numbered matching PMP entry.
    pub(crate) fn pmp_allows(
        self,
        start: u64,
        width: u64,
        privilege: PrivilegeMode,
        read: bool,
        write: bool,
        execute: bool,
    ) -> bool {
        let Some(end) = start.checked_add(width) else {
            return false;
        };
        let mut selected = None;
        for index in 0..PMP_ENTRY_COUNT {
            let config = self.pmp_config(index);
            if config & PMP_CFG_ADDRESS_MASK == PMP_CFG_ADDRESS_OFF {
                continue;
            }
            let Some(range) = self.pmp_range(index, config) else {
                continue;
            };
            if range.0 < end && start < range.1 {
                selected = Some((config, range));
                break;
            }
        }
        let Some((config, range)) = selected else {
            return privilege == PrivilegeMode::Machine;
        };
        if start < range.0 || end > range.1 {
            return false;
        }
        if privilege == PrivilegeMode::Machine && config & PMP_CFG_LOCK == 0 {
            return true;
        }
        (!read || config & PMP_CFG_READ != 0)
            && (!write || config & PMP_CFG_WRITE != 0)
            && (!execute || config & PMP_CFG_EXECUTE != 0)
    }

    fn pmp_range(self, index: usize, config: u8) -> Option<(u64, u64)> {
        let address = self.pmpaddr[index];
        match config & PMP_CFG_ADDRESS_MASK {
            PMP_CFG_ADDRESS_OFF => None,
            PMP_CFG_ADDRESS_TOR => {
                let start = if index == 0 {
                    0
                } else {
                    self.pmpaddr[index - 1] << PMP_SHIFT
                };
                let end = address << PMP_SHIFT;
                (start < end).then_some((start, end))
            }
            PMP_CFG_ADDRESS_NA4 => {
                let start = address << PMP_SHIFT;
                Some((start, start + NA4_BYTES))
            }
            PMP_CFG_ADDRESS_NAPOT => {
                let trailing_ones = (!address) & PMP_ADDRESS_MASK;
                let trailing = if trailing_ones == 0 {
                    PMP_ADDRESS_BITS
                } else {
                    trailing_ones.trailing_zeros()
                };
                let mask = (1_u64 << trailing) - 1;
                let start = (address & !mask) << PMP_SHIFT;
                let size = 1_u64 << (trailing + PMP_SHIFT + 1);
                Some((start, start + size))
            }
            _ => None,
        }
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

    /// Installs pending bits asserted by devices connected to the platform.
    pub(crate) fn set_device_pending(&mut self, pending: u64) {
        self.device_pending = pending & INTERRUPT_MASK;
    }

    /// Highest-priority enabled interrupt and the privilege mode that should
    /// take it.
    pub(crate) fn pending_interrupt(
        self,
        privilege: PrivilegeMode,
    ) -> Option<(PrivilegeMode, u64)> {
        let pending = self.pending_mip() & self.mie;
        [
            (INTERRUPT_MEIP, MACHINE_EXTERNAL_INTERRUPT),
            (INTERRUPT_MSIP, MACHINE_SOFTWARE_INTERRUPT),
            (INTERRUPT_MTIP, MACHINE_TIMER_INTERRUPT),
            (INTERRUPT_SEIP, SUPERVISOR_EXTERNAL_INTERRUPT),
            (INTERRUPT_SSIP, SUPERVISOR_SOFTWARE_INTERRUPT),
            (INTERRUPT_STIP, SUPERVISOR_TIMER_INTERRUPT),
        ]
        .into_iter()
        .find_map(|(bit, cause)| {
            if pending & bit == 0 {
                return None;
            }
            let delegated = self.mideleg & bit != 0;
            let (target, enabled) = if delegated {
                if privilege == PrivilegeMode::Machine {
                    return None;
                }
                (
                    PrivilegeMode::Supervisor,
                    privilege == PrivilegeMode::User || self.mstatus & MSTATUS_SIE != 0,
                )
            } else {
                (
                    PrivilegeMode::Machine,
                    privilege != PrivilegeMode::Machine || self.mstatus & MSTATUS_MIE != 0,
                )
            };
            enabled.then_some((target, INTERRUPT_CAUSE_BIT | cause))
        })
    }

    /// Returns the CSR value used as the write base of a CSR read-modify-write.
    ///
    /// External interrupt-controller signals are visible to the CSR read but
    /// never participate in the value written back by CSRRS/CSRRC.
    pub(crate) fn read_for_rmw_with_privilege(
        self,
        address: u16,
        privilege: PrivilegeMode,
    ) -> Option<u64> {
        if !self.can_read(address, privilege) {
            return None;
        }
        match address {
            inst::CSR_MIP => Some(self.mip),
            inst::CSR_SIP => Some(self.mip & self.mideleg),
            _ => self.read_with_privilege(address, privilege),
        }
    }

    /// Whether any locally enabled interrupt is pending, independent of the
    /// current global interrupt-enable state.
    pub(crate) fn has_enabled_interrupt(self) -> bool {
        self.pending_mip() & self.mie != 0
    }

    /// Highest-priority enabled delegated supervisor interrupt for the current
    /// privilege mode.
    #[cfg(test)]
    pub(crate) fn pending_supervisor_interrupt(self, privilege: PrivilegeMode) -> Option<u64> {
        self.pending_interrupt(privilege)
            .filter(|(target, _)| *target == PrivilegeMode::Supervisor)
            .map(|(_, cause)| cause)
    }

    fn pending_mip(self) -> u64 {
        self.mip | self.device_pending
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

fn canonical_pmp_cfg(value: u8) -> u8 {
    let mut config = value & PMP_CFG_FIELDS;
    if config & PMP_CFG_WRITE != 0 && config & PMP_CFG_READ == 0 {
        config &= !PMP_CFG_WRITE;
    }
    config
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
        assert!(csrs.write(inst::CSR_PMPADDR0, u64::MAX));
        assert!(csrs.write(inst::CSR_PMPADDR15, u64::MAX));
        assert!(csrs.write(inst::CSR_PMPCFG0, u64::MAX));
        assert!(csrs.write(inst::CSR_PMPCFG2, u64::MAX));
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
        let expected_pmpcfg = (0..PMP_ENTRIES_PER_CFG).fold(0, |value, index| {
            value | (u64::from(canonical_pmp_cfg(u8::MAX)) << (index * u8::BITS as usize))
        });
        assert_eq!(csrs.read(inst::CSR_PMPCFG0), Some(expected_pmpcfg));
        assert_eq!(csrs.read(inst::CSR_PMPCFG2), Some(expected_pmpcfg));
        assert_eq!(csrs.read(inst::CSR_PMPADDR0), Some(PMP_ADDRESS_MASK));
        assert_eq!(csrs.read(inst::CSR_PMPADDR15), Some(PMP_ADDRESS_MASK));
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
    fn pmp_decodes_ranges_and_enforces_privilege() {
        let mut csrs = Csrs::default();
        let base = 0x1000;
        assert!(csrs.write(inst::CSR_PMPADDR0, base >> PMP_SHIFT));
        assert!(csrs.write(
            inst::CSR_PMPCFG0,
            u64::from(PMP_CFG_READ | PMP_CFG_ADDRESS_NA4)
        ));

        assert!(csrs.pmp_allows(base, 1, PrivilegeMode::Supervisor, true, false, false));
        assert!(!csrs.pmp_allows(base, 1, PrivilegeMode::Supervisor, false, true, false));
        assert!(!csrs.pmp_allows(
            base + NA4_BYTES,
            1,
            PrivilegeMode::Supervisor,
            true,
            false,
            false
        ));
        assert!(csrs.pmp_allows(
            base + NA4_BYTES,
            1,
            PrivilegeMode::Machine,
            true,
            true,
            true
        ));

        assert!(csrs.write(
            inst::CSR_PMPCFG0,
            u64::from(PMP_CFG_READ | PMP_CFG_ADDRESS_NA4 | PMP_CFG_LOCK)
        ));
        assert!(!csrs.pmp_allows(base, 1, PrivilegeMode::Machine, false, true, false));
        assert!(csrs.write(inst::CSR_PMPCFG0, 0));
        assert_ne!(csrs.read(inst::CSR_PMPCFG0), Some(0));
    }

    #[test]
    fn pmp_uses_the_first_matching_entry_for_the_whole_access() {
        let mut csrs = Csrs::default();
        assert!(csrs.write(inst::CSR_PMPADDR0, 0));
        assert!(csrs.write(inst::CSR_PMPADDR1, 1));
        let two_na4_entries = u64::from(PMP_CFG_READ | PMP_CFG_ADDRESS_NA4)
            | (u64::from(PMP_CFG_READ | PMP_CFG_ADDRESS_NA4) << u8::BITS);
        assert!(csrs.write(inst::CSR_PMPCFG0, two_na4_entries));

        assert!(csrs.pmp_allows(0, NA4_BYTES, PrivilegeMode::Supervisor, true, false, false));
        assert!(!csrs.pmp_allows(
            0,
            NA4_BYTES * 2,
            PrivilegeMode::Supervisor,
            true,
            false,
            false
        ));
    }

    #[test]
    fn pmp_m_mode_bypasses_only_after_selecting_the_first_match() {
        let mut csrs = Csrs::default();
        assert!(csrs.write(inst::CSR_PMPADDR0, u64::MAX));
        assert!(csrs.write(inst::CSR_PMPADDR1, u64::MAX));
        let unlocked = u64::from(PMP_CFG_ADDRESS_NAPOT);
        let locked = u64::from(PMP_CFG_ADDRESS_NAPOT | PMP_CFG_LOCK) << u8::BITS;
        assert!(csrs.write(inst::CSR_PMPCFG0, unlocked | locked));
        assert!(csrs.pmp_allows(0, NA4_BYTES, PrivilegeMode::Machine, true, true, true));

        let locked_first = u64::from(PMP_CFG_ADDRESS_NAPOT | PMP_CFG_LOCK);
        let permissive_second =
            u64::from(PMP_CFG_READ | PMP_CFG_WRITE | PMP_CFG_EXECUTE | PMP_CFG_ADDRESS_NAPOT)
                << u8::BITS;
        assert!(csrs.write(inst::CSR_PMPCFG0, locked_first | permissive_second));
        assert!(!csrs.pmp_allows(0, NA4_BYTES, PrivilegeMode::Machine, true, false, false));
    }

    #[test]
    fn locked_tor_entry_protects_its_lower_bound_address() {
        let mut csrs = Csrs::default();
        assert!(csrs.write(inst::CSR_PMPADDR0, 0x10));
        assert!(csrs.write(inst::CSR_PMPADDR1, 0x20));
        let locked_tor = u64::from(PMP_CFG_ADDRESS_TOR | PMP_CFG_LOCK) << u8::BITS;
        assert!(csrs.write(inst::CSR_PMPCFG0, locked_tor));

        assert!(csrs.write(inst::CSR_PMPADDR0, 0x40));
        assert_eq!(csrs.read(inst::CSR_PMPADDR0), Some(0x10));

        assert!(csrs.write(inst::CSR_PMPCFG0, 0));
        assert_eq!(
            csrs.read(inst::CSR_PMPCFG0),
            Some(u64::from(PMP_CFG_ADDRESS_TOR | PMP_CFG_LOCK) << u8::BITS)
        );

        let mut unlocked_tor = Csrs::default();
        assert!(unlocked_tor.write(inst::CSR_PMPADDR0, 0x10));
        assert!(unlocked_tor.write(inst::CSR_PMPADDR1, 0x20));
        assert!(unlocked_tor.write(
            inst::CSR_PMPCFG0,
            u64::from(PMP_CFG_ADDRESS_TOR) << u8::BITS
        ));
        assert!(unlocked_tor.write(inst::CSR_PMPADDR0, 0x40));
        assert_eq!(unlocked_tor.read(inst::CSR_PMPADDR0), Some(0x40));

        let mut locked_napot = Csrs::default();
        assert!(locked_napot.write(inst::CSR_PMPADDR0, 0x10));
        assert!(locked_napot.write(inst::CSR_PMPADDR1, 0x20));
        assert!(locked_napot.write(
            inst::CSR_PMPCFG0,
            u64::from(PMP_CFG_ADDRESS_NAPOT | PMP_CFG_LOCK) << u8::BITS
        ));
        assert!(locked_napot.write(inst::CSR_PMPADDR0, 0x40));
        assert_eq!(locked_napot.read(inst::CSR_PMPADDR0), Some(0x40));
    }

    #[test]
    fn debug_trigger_csrs_report_that_no_triggers_exist() {
        let mut csrs = Csrs::default();

        for address in [
            inst::CSR_TCONTROL,
            inst::CSR_TSELECT,
            inst::CSR_TDATA1,
            inst::CSR_TDATA2,
            inst::CSR_TDATA3,
        ] {
            assert!(csrs.write(address, u64::MAX));
            assert_eq!(csrs.read(address), Some(0));
        }
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
    fn csr_rmw_base_excludes_device_pending_bits() {
        let mut csrs = Csrs::default();
        let software_pending = INTERRUPT_STIP;
        let device_pending = INTERRUPT_SEIP;
        csrs.set_device_pending(device_pending);
        assert!(csrs.write(inst::CSR_MIP, software_pending));
        assert!(csrs.write(inst::CSR_MIDELEG, device_pending | software_pending));

        assert_eq!(
            csrs.read(inst::CSR_MIP),
            Some(software_pending | device_pending)
        );
        assert_eq!(
            csrs.read_for_rmw_with_privilege(inst::CSR_MIP, PrivilegeMode::Machine),
            Some(software_pending)
        );
        assert_eq!(
            csrs.read(inst::CSR_SIP),
            Some(software_pending | device_pending)
        );
        assert_eq!(
            csrs.read_for_rmw_with_privilege(inst::CSR_SIP, PrivilegeMode::Machine),
            Some(software_pending)
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
    fn machine_interrupts_respect_mie_and_use_architectural_priority() {
        let mut csrs = Csrs::default();
        let external = device_interrupt_mask(InterruptLines {
            machine_external: true,
            ..InterruptLines::default()
        });
        let timer = device_interrupt_mask(InterruptLines {
            machine_timer: true,
            ..InterruptLines::default()
        });
        csrs.set_device_pending(external | timer);
        assert!(csrs.write(inst::CSR_MIE, INTERRUPT_MEIP | INTERRUPT_MTIP));

        assert_eq!(
            csrs.pending_interrupt(PrivilegeMode::Machine),
            None,
            "mstatus.MIE must enable machine interrupts"
        );
        for privilege in [PrivilegeMode::Supervisor, PrivilegeMode::User] {
            assert_eq!(
                csrs.pending_interrupt(privilege),
                Some((
                    PrivilegeMode::Machine,
                    INTERRUPT_CAUSE_BIT | MACHINE_EXTERNAL_INTERRUPT
                )),
                "machine interrupts are always globally enabled below M-mode"
            );
        }

        assert!(csrs.write(inst::CSR_MSTATUS, MSTATUS_MIE));
        assert_eq!(
            csrs.pending_interrupt(PrivilegeMode::Machine),
            Some((
                PrivilegeMode::Machine,
                INTERRUPT_CAUSE_BIT | MACHINE_EXTERNAL_INTERRUPT
            ))
        );
        assert_eq!(
            csrs.read(inst::CSR_MIP).unwrap() & (INTERRUPT_MEIP | INTERRUPT_MTIP),
            INTERRUPT_MEIP | INTERRUPT_MTIP
        );
    }

    #[test]
    fn delegated_interrupts_can_preempt_user_mode_without_sie() {
        let mut csrs = Csrs::default();
        let external = device_interrupt_mask(InterruptLines {
            supervisor_external: true,
            ..InterruptLines::default()
        });
        csrs.set_device_pending(external);
        assert!(csrs.write(inst::CSR_MIDELEG, INTERRUPT_SEIP));
        assert!(csrs.write(inst::CSR_MIE, INTERRUPT_SEIP));

        assert_eq!(
            csrs.pending_interrupt(PrivilegeMode::User),
            Some((
                PrivilegeMode::Supervisor,
                INTERRUPT_CAUSE_BIT | SUPERVISOR_EXTERNAL_INTERRUPT
            ))
        );
        assert_eq!(csrs.pending_interrupt(PrivilegeMode::Supervisor), None);
        assert_eq!(csrs.pending_interrupt(PrivilegeMode::Machine), None);
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
