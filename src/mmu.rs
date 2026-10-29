//! Sv39 address translation and the architectural translation cache model.

use crate::bus::{Bus, MemoryType};
use crate::cpu::PrivilegeMode;

const PAGE_BITS: u32 = 12;
const VPN_BITS_PER_LEVEL: u32 = 9;
const SV39_LEVELS: u8 = 3;
const SV39_VIRTUAL_BITS: u32 = 39;
const SV39_MODE: u64 = 8;
const SATP_MODE_SHIFT: u32 = 60;
const SATP_ASID_SHIFT: u32 = 44;
const SATP_ASID_MASK: u64 = (1 << 16) - 1;
const PTE_VALID: u64 = 1 << 0;
const PTE_READ: u64 = 1 << 1;
const PTE_WRITE: u64 = 1 << 2;
const PTE_EXECUTE: u64 = 1 << 3;
const PTE_USER: u64 = 1 << 4;
const PTE_GLOBAL: u64 = 1 << 5;
const PTE_ACCESSED: u64 = 1 << 6;
const PTE_DIRTY: u64 = 1 << 7;
const PTE_PPN_SHIFT: u32 = 10;
const PTE_PPN_MASK: u64 = (1 << 44) - 1;
const PTE_PBMT_SHIFT: u32 = 61;
const PTE_PBMT_MASK: u64 = 0b11 << PTE_PBMT_SHIFT;
const PTE_RESERVED_MASK: u64 = ((1 << 10) - 1) << 54 & !PTE_PBMT_MASK;
const PTE_BYTES: u64 = 8;
const VPN_INDEX_MASK: u64 = (1 << VPN_BITS_PER_LEVEL) - 1;
const ASID_MASK: u64 = SATP_ASID_MASK;
const TLB_ENTRY_COUNT: usize = 64;

/// Translation purpose for one memory access.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Access {
    Fetch,
    Load,
    Store,
    CacheManagement,
}

/// Why translation could not produce a physical address.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FaultKind {
    Page,
    Access,
}

/// Translation failure with the original access classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Fault {
    pub(crate) access: Access,
    pub(crate) kind: FaultKind,
}

/// Translation state from `satp` and `mstatus`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Config {
    pub(crate) satp: u64,
    pub(crate) privilege: PrivilegeMode,
    pub(crate) sum: bool,
    pub(crate) mxr: bool,
    pub(crate) pbmt_enabled: bool,
}

/// Physical address and attributes produced by one translation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Translation {
    pub(crate) address: u64,
    pub(crate) memory_type: MemoryType,
}

#[derive(Debug, Clone, Copy)]
struct TlbEntry {
    valid: bool,
    global: bool,
    asid: u64,
    virtual_page: u64,
    physical_page: u64,
    page_bytes: u64,
    readable: bool,
    writable: bool,
    executable: bool,
    user: bool,
    accessed: bool,
    dirty: bool,
    memory_type: MemoryType,
}

impl TlbEntry {
    const EMPTY: Self = Self {
        valid: false,
        global: false,
        asid: 0,
        virtual_page: 0,
        physical_page: 0,
        page_bytes: 0,
        readable: false,
        writable: false,
        executable: false,
        user: false,
        accessed: false,
        dirty: false,
        memory_type: MemoryType::Cacheable,
    };

    fn matches(self, virtual_address: u64, asid: u64) -> bool {
        self.valid
            && self.virtual_page == page_base(virtual_address, self.page_bytes)
            && (self.global || self.asid == asid)
    }
}

/// Sv39 translation unit with a 64-entry fully-associative FIFO TLB.
#[derive(Debug, Clone)]
pub(crate) struct Mmu {
    entries: [TlbEntry; TLB_ENTRY_COUNT],
    replace: usize,
    writes_ordered_for_invalidation: bool,
    invalidations_ordered_for_implicit_reads: bool,
}

impl Default for Mmu {
    fn default() -> Self {
        Self {
            entries: [TlbEntry::EMPTY; TLB_ENTRY_COUNT],
            replace: 0,
            writes_ordered_for_invalidation: false,
            invalidations_ordered_for_implicit_reads: false,
        }
    }
}

impl Mmu {
    /// Translates one virtual address.
    pub(crate) fn translate<B: Bus>(
        &mut self,
        bus: &mut B,
        config: Config,
        virtual_address: u64,
        access: Access,
    ) -> Result<Translation, Fault> {
        let mode = (config.satp >> SATP_MODE_SHIFT) & 0xf;
        if config.privilege == PrivilegeMode::Machine || mode == 0 {
            return Ok(Translation {
                address: virtual_address,
                memory_type: MemoryType::Cacheable,
            });
        }
        if mode != SV39_MODE || !canonical_sv39(virtual_address) {
            return Err(fault(access, FaultKind::Page));
        }

        let asid = (config.satp >> SATP_ASID_SHIFT) & ASID_MASK;
        let entry = if let Some(entry) = self
            .entries
            .iter()
            .copied()
            .find(|entry| entry.matches(virtual_address, asid))
        {
            entry
        } else {
            self.insert(walk(bus, config, virtual_address, access)?)
        };

        if !permits(entry, config, access) {
            return Err(fault(access, FaultKind::Page));
        }
        Ok(Translation {
            address: entry.physical_page | (virtual_address & (entry.page_bytes - 1)),
            memory_type: entry.memory_type,
        })
    }

    /// Invalidates cached mappings selected by an `SFENCE.VMA` operation.
    pub(crate) fn sfence_vma(&mut self, virtual_address: Option<u64>, asid: Option<u64>) {
        self.invalidate_vma(virtual_address, asid);
        self.writes_ordered_for_invalidation = true;
        self.invalidations_ordered_for_implicit_reads = true;
    }

    /// Invalidates cached mappings without the ordering guarantees of
    /// `SFENCE.VMA`.
    pub(crate) fn sinval_vma(&mut self, virtual_address: Option<u64>, asid: Option<u64>) {
        self.invalidate_vma(virtual_address, asid);
        self.invalidations_ordered_for_implicit_reads = false;
    }

    /// Orders prior explicit writes before later invalidation operations.
    pub(crate) fn sfence_w_inval(&mut self) {
        self.writes_ordered_for_invalidation = true;
    }

    /// Orders prior invalidation effects before later implicit reads.
    pub(crate) fn sfence_inval_ir(&mut self) {
        self.invalidations_ordered_for_implicit_reads = true;
    }

    /// Records an explicit write that later ordered invalidations must observe.
    pub(crate) fn note_explicit_write(&mut self) {
        self.writes_ordered_for_invalidation = false;
    }

    fn invalidate_vma(&mut self, virtual_address: Option<u64>, asid: Option<u64>) {
        for entry in &mut self.entries {
            let address_matches = !entry.valid
                || virtual_address.is_none_or(|address| {
                    entry.virtual_page == page_base(address, entry.page_bytes)
                });
            let asid_matches = asid.is_none_or(|expected| entry.global || entry.asid == expected);
            if address_matches && asid_matches {
                entry.valid = false;
            }
        }
    }

    fn insert(&mut self, entry: TlbEntry) -> TlbEntry {
        let slot = self.replace;
        self.entries[slot] = entry;
        self.replace = (slot + 1) % TLB_ENTRY_COUNT;
        entry
    }
}

fn walk<B: Bus>(
    bus: &mut B,
    config: Config,
    virtual_address: u64,
    access: Access,
) -> Result<TlbEntry, Fault> {
    let asid = (config.satp >> SATP_ASID_SHIFT) & ASID_MASK;
    let mut table = config.satp & PTE_PPN_MASK;
    let mut global = false;
    for level in (0..SV39_LEVELS).rev() {
        let index = vpn_index(virtual_address, level);
        let address = (table << PAGE_BITS).wrapping_add(index * PTE_BYTES);
        let pte = bus
            .read_u64(address, MemoryType::Cacheable)
            .map_err(|_| fault(access, FaultKind::Access))?;
        if pte & PTE_VALID == 0 || pte & PTE_RESERVED_MASK != 0 {
            return Err(fault(access, FaultKind::Page));
        }
        let readable = pte & PTE_READ != 0;
        let writable = pte & PTE_WRITE != 0;
        let executable = pte & PTE_EXECUTE != 0;
        let pbmt = (pte & PTE_PBMT_MASK) >> PTE_PBMT_SHIFT;
        if writable && !readable {
            return Err(fault(access, FaultKind::Page));
        }
        global |= pte & PTE_GLOBAL != 0;
        if readable || executable {
            if pbmt == 3 || (pbmt != 0 && !config.pbmt_enabled) {
                return Err(fault(access, FaultKind::Page));
            }
            let ppn = (pte >> PTE_PPN_SHIFT) & PTE_PPN_MASK;
            if superpage_ppn_misaligned(ppn, level) {
                return Err(fault(access, FaultKind::Page));
            }
            let page_shift = PAGE_BITS + VPN_BITS_PER_LEVEL * u32::from(level);
            let entry = TlbEntry {
                valid: true,
                global,
                asid,
                virtual_page: page_base(virtual_address, 1 << page_shift),
                physical_page: ppn << PAGE_BITS,
                page_bytes: 1 << page_shift,
                readable,
                writable,
                executable,
                user: pte & PTE_USER != 0,
                accessed: pte & PTE_ACCESSED != 0,
                dirty: pte & PTE_DIRTY != 0,
                memory_type: match pbmt {
                    0 => MemoryType::Cacheable,
                    1 => MemoryType::NonCacheable,
                    2 => MemoryType::Io,
                    _ => unreachable!("reserved PBMT value was rejected"),
                },
            };
            return Ok(entry);
        }
        if pte & (PTE_USER | PTE_ACCESSED | PTE_DIRTY) != 0 || pbmt != 0 {
            return Err(fault(access, FaultKind::Page));
        }
        table = (pte >> PTE_PPN_SHIFT) & PTE_PPN_MASK;
    }
    Err(fault(access, FaultKind::Page))
}

fn permits(entry: TlbEntry, config: Config, access: Access) -> bool {
    if !entry.valid
        || !entry.accessed
        || access == Access::Store && !entry.dirty
        || pte_access_not_permitted(entry, config, access)
    {
        return false;
    }
    match access {
        Access::Fetch => entry.executable,
        Access::Load => entry.readable || config.mxr && entry.executable,
        Access::Store => entry.writable,
        Access::CacheManagement => entry.readable || entry.writable,
    }
}

fn pte_access_not_permitted(entry: TlbEntry, config: Config, access: Access) -> bool {
    match config.privilege {
        PrivilegeMode::Machine => false,
        PrivilegeMode::User => !entry.user,
        PrivilegeMode::Supervisor => entry.user && (!config.sum || access == Access::Fetch),
    }
}

fn fault(access: Access, kind: FaultKind) -> Fault {
    Fault { access, kind }
}

fn vpn_index(virtual_address: u64, level: u8) -> u64 {
    (virtual_address >> (PAGE_BITS + VPN_BITS_PER_LEVEL * u32::from(level))) & VPN_INDEX_MASK
}

fn page_base(address: u64, page_bytes: u64) -> u64 {
    address & !(page_bytes - 1)
}

fn canonical_sv39(address: u64) -> bool {
    let upper = address >> SV39_VIRTUAL_BITS;
    let sign = (address >> (SV39_VIRTUAL_BITS - 1)) & 1;
    if sign == 0 {
        upper == 0
    } else {
        upper == (1 << (u64::BITS - SV39_VIRTUAL_BITS)) - 1
    }
}

fn superpage_ppn_misaligned(ppn: u64, level: u8) -> bool {
    let low_bits = VPN_BITS_PER_LEVEL * u32::from(level);
    low_bits != 0 && ppn & ((1 << low_bits) - 1) != 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{DRAM_BASE, Memory};

    const MEMORY_BYTES: usize = 0x5000;
    const ROOT_TABLE: u64 = DRAM_BASE;
    const LEVEL_1_TABLE: u64 = DRAM_BASE + 0x1000;
    const LEVEL_0_TABLE: u64 = DRAM_BASE + 0x2000;
    const DATA_PAGE: u64 = DRAM_BASE + 0x3000;
    const SECOND_DATA_PAGE: u64 = DRAM_BASE + 0x4000;
    const TEST_VA: u64 = 0x1000;
    const TEST_OFFSET: u64 = 0x234;
    const ASID_ONE: u64 = 1;
    const ASID_TWO: u64 = 2;
    const PTE_NONLEAF: u64 = PTE_VALID;
    const PTE_LEAF: u64 = PTE_VALID | PTE_READ | PTE_WRITE | PTE_EXECUTE | PTE_ACCESSED | PTE_DIRTY;

    fn pte(address: u64, flags: u64) -> u64 {
        ((address >> PAGE_BITS) << PTE_PPN_SHIFT) | flags
    }

    fn translation(address: u64) -> Translation {
        Translation {
            address,
            memory_type: MemoryType::Cacheable,
        }
    }

    fn config(asid: u64) -> Config {
        Config {
            satp: (SV39_MODE << SATP_MODE_SHIFT)
                | ((asid & SATP_ASID_MASK) << SATP_ASID_SHIFT)
                | (ROOT_TABLE >> PAGE_BITS),
            privilege: PrivilegeMode::Supervisor,
            sum: false,
            mxr: false,
            pbmt_enabled: false,
        }
    }

    fn install_4k_mapping(memory: &mut Memory, leaf: u64) {
        memory
            .write_u64(
                ROOT_TABLE,
                pte(LEVEL_1_TABLE, PTE_NONLEAF),
                MemoryType::Cacheable,
            )
            .unwrap();
        memory
            .write_u64(
                LEVEL_1_TABLE,
                pte(LEVEL_0_TABLE, PTE_NONLEAF),
                MemoryType::Cacheable,
            )
            .unwrap();
        memory
            .write_u64(LEVEL_0_TABLE + 8, leaf, MemoryType::Cacheable)
            .unwrap();
    }

    #[test]
    fn bare_mode_is_identity() {
        let mut memory = Memory::with_size(MEMORY_BYTES);
        let mut mmu = Mmu::default();
        let config = Config {
            satp: 0,
            privilege: PrivilegeMode::Supervisor,
            sum: false,
            mxr: false,
            pbmt_enabled: false,
        };

        assert_eq!(
            mmu.translate(&mut memory, config, DATA_PAGE + 7, Access::Load),
            Ok(translation(DATA_PAGE + 7))
        );
    }

    #[test]
    fn sv39_translates_4k_pages_with_permissions() {
        let mut memory = Memory::with_size(MEMORY_BYTES);
        install_4k_mapping(&mut memory, pte(DATA_PAGE, PTE_LEAF));
        let mut mmu = Mmu::default();

        assert_eq!(
            mmu.translate(
                &mut memory,
                config(ASID_ONE),
                TEST_VA + TEST_OFFSET,
                Access::Store
            ),
            Ok(translation(DATA_PAGE + TEST_OFFSET))
        );
        assert_eq!(
            mmu.translate(
                &mut memory,
                config(ASID_ONE),
                TEST_VA + TEST_OFFSET,
                Access::Fetch
            ),
            Ok(translation(DATA_PAGE + TEST_OFFSET))
        );
    }

    #[test]
    fn sv39_enforces_user_sum_mxr_and_ad_bits() {
        let mut memory = Memory::with_size(MEMORY_BYTES);
        let execute_only = PTE_VALID | PTE_EXECUTE | PTE_USER | PTE_ACCESSED;
        install_4k_mapping(&mut memory, pte(DATA_PAGE, execute_only));
        let mut mmu = Mmu::default();
        let mut config = config(ASID_ONE);
        config.privilege = PrivilegeMode::User;

        assert_eq!(
            mmu.translate(&mut memory, config, TEST_VA, Access::Load),
            Err(Fault {
                access: Access::Load,
                kind: FaultKind::Page
            })
        );
        config.mxr = true;
        assert_eq!(
            mmu.translate(&mut memory, config, TEST_VA, Access::Load),
            Ok(translation(DATA_PAGE))
        );
        config.mxr = false;
        config.privilege = PrivilegeMode::Supervisor;
        assert_eq!(
            mmu.translate(&mut memory, config, TEST_VA, Access::Fetch),
            Err(Fault {
                access: Access::Fetch,
                kind: FaultKind::Page
            })
        );
        config.sum = true;
        assert_eq!(
            mmu.translate(&mut memory, config, TEST_VA, Access::Fetch),
            Err(Fault {
                access: Access::Fetch,
                kind: FaultKind::Page
            })
        );

        install_4k_mapping(
            &mut memory,
            pte(DATA_PAGE, PTE_VALID | PTE_READ | PTE_WRITE),
        );
        mmu.sfence_vma(None, None);
        assert_eq!(
            mmu.translate(&mut memory, config, TEST_VA, Access::Load),
            Err(Fault {
                access: Access::Load,
                kind: FaultKind::Page
            })
        );
        install_4k_mapping(
            &mut memory,
            pte(DATA_PAGE, PTE_VALID | PTE_READ | PTE_WRITE | PTE_ACCESSED),
        );
        mmu.sfence_vma(None, None);
        assert_eq!(
            mmu.translate(&mut memory, config, TEST_VA, Access::Store),
            Err(Fault {
                access: Access::Store,
                kind: FaultKind::Page
            })
        );
    }

    #[test]
    fn sv39_supports_aligned_superpages_and_rejects_misaligned_ones() {
        let mut memory = Memory::with_size(MEMORY_BYTES);
        let leaf = pte(DRAM_BASE, PTE_LEAF);
        memory
            .write_u64(ROOT_TABLE, leaf, MemoryType::Cacheable)
            .unwrap();
        let mut mmu = Mmu::default();

        assert_eq!(
            mmu.translate(&mut memory, config(ASID_ONE), TEST_OFFSET, Access::Load),
            Ok(translation(DRAM_BASE + TEST_OFFSET))
        );

        memory
            .write_u64(
                ROOT_TABLE,
                leaf | (1 << PTE_PPN_SHIFT),
                MemoryType::Cacheable,
            )
            .unwrap();
        mmu.sfence_vma(None, None);
        assert_eq!(
            mmu.translate(&mut memory, config(ASID_ONE), TEST_OFFSET, Access::Load),
            Err(Fault {
                access: Access::Load,
                kind: FaultKind::Page
            })
        );
    }

    #[test]
    fn svpbmt_translates_memory_types_and_validates_encodings() {
        let mut memory = Memory::with_size(MEMORY_BYTES);
        let mut mmu = Mmu::default();
        let mut pbmt_config = config(ASID_ONE);

        install_4k_mapping(
            &mut memory,
            pte(DATA_PAGE, PTE_LEAF | (2 << PTE_PBMT_SHIFT)),
        );
        assert_eq!(
            mmu.translate(&mut memory, pbmt_config, TEST_VA, Access::Load),
            Err(Fault {
                access: Access::Load,
                kind: FaultKind::Page
            })
        );

        pbmt_config.pbmt_enabled = true;
        assert_eq!(
            mmu.translate(&mut memory, pbmt_config, TEST_VA, Access::Load),
            Ok(Translation {
                address: DATA_PAGE,
                memory_type: MemoryType::Io,
            })
        );

        install_4k_mapping(
            &mut memory,
            pte(DATA_PAGE, PTE_LEAF | (1 << PTE_PBMT_SHIFT)),
        );
        mmu.sfence_vma(None, None);
        assert_eq!(
            mmu.translate(&mut memory, pbmt_config, TEST_VA, Access::Load),
            Ok(Translation {
                address: DATA_PAGE,
                memory_type: MemoryType::NonCacheable,
            })
        );

        install_4k_mapping(&mut memory, pte(DATA_PAGE, PTE_LEAF | PTE_PBMT_MASK));
        mmu.sfence_vma(None, None);
        assert_eq!(
            mmu.translate(&mut memory, pbmt_config, TEST_VA, Access::Load),
            Err(Fault {
                access: Access::Load,
                kind: FaultKind::Page
            })
        );
    }

    #[test]
    fn svpbmt_rejects_nonzero_pbmt_on_nonleaf_entries() {
        let mut memory = Memory::with_size(MEMORY_BYTES);
        let mut mmu = Mmu::default();
        let mut pbmt_config = config(ASID_ONE);
        pbmt_config.pbmt_enabled = true;

        memory
            .write_u64(
                ROOT_TABLE,
                pte(LEVEL_1_TABLE, PTE_NONLEAF | (1 << PTE_PBMT_SHIFT)),
                MemoryType::Cacheable,
            )
            .unwrap();

        assert_eq!(
            mmu.translate(&mut memory, pbmt_config, TEST_VA, Access::Load),
            Err(Fault {
                access: Access::Load,
                kind: FaultKind::Page
            })
        );
    }

    #[test]
    fn cache_management_accepts_read_or_write_access_without_dirty() {
        let mut memory = Memory::with_size(MEMORY_BYTES);
        let mut mmu = Mmu::default();

        install_4k_mapping(
            &mut memory,
            pte(DATA_PAGE, PTE_VALID | PTE_READ | PTE_ACCESSED),
        );
        assert_eq!(
            mmu.translate(
                &mut memory,
                config(ASID_ONE),
                TEST_VA,
                Access::CacheManagement
            ),
            Ok(translation(DATA_PAGE))
        );
        assert_eq!(
            mmu.translate(&mut memory, config(ASID_ONE), TEST_VA, Access::Store),
            Err(Fault {
                access: Access::Store,
                kind: FaultKind::Page
            })
        );

        install_4k_mapping(
            &mut memory,
            pte(DATA_PAGE, PTE_VALID | PTE_READ | PTE_WRITE | PTE_ACCESSED),
        );
        mmu.sfence_vma(None, None);
        assert_eq!(
            mmu.translate(
                &mut memory,
                config(ASID_ONE),
                TEST_VA,
                Access::CacheManagement
            ),
            Ok(translation(DATA_PAGE))
        );
        assert_eq!(
            mmu.translate(&mut memory, config(ASID_ONE), TEST_VA, Access::Store),
            Err(Fault {
                access: Access::Store,
                kind: FaultKind::Page
            })
        );
    }

    #[test]
    fn sfence_invalidates_matching_asid_and_address_entries() {
        let mut memory = Memory::with_size(MEMORY_BYTES);
        install_4k_mapping(&mut memory, pte(DATA_PAGE, PTE_LEAF));
        let mut mmu = Mmu::default();

        assert_eq!(
            mmu.translate(&mut memory, config(ASID_ONE), TEST_VA, Access::Load),
            Ok(translation(DATA_PAGE))
        );
        install_4k_mapping(&mut memory, pte(SECOND_DATA_PAGE, PTE_LEAF));
        assert_eq!(
            mmu.translate(&mut memory, config(ASID_ONE), TEST_VA, Access::Load),
            Ok(translation(DATA_PAGE))
        );
        mmu.sfence_vma(Some(TEST_VA), Some(ASID_TWO));
        assert_eq!(
            mmu.translate(&mut memory, config(ASID_ONE), TEST_VA, Access::Load),
            Ok(translation(DATA_PAGE))
        );
        mmu.sfence_vma(Some(TEST_VA), Some(ASID_ONE));
        assert_eq!(
            mmu.translate(&mut memory, config(ASID_ONE), TEST_VA, Access::Load),
            Ok(translation(SECOND_DATA_PAGE))
        );
    }

    #[test]
    fn svinval_invalidates_without_claiming_full_fence_ordering() {
        let mut memory = Memory::with_size(MEMORY_BYTES);
        install_4k_mapping(&mut memory, pte(DATA_PAGE, PTE_LEAF));
        let mut mmu = Mmu::default();

        mmu.sfence_w_inval();
        mmu.sfence_inval_ir();
        assert_eq!(
            mmu.translate(&mut memory, config(ASID_ONE), TEST_VA, Access::Load),
            Ok(translation(DATA_PAGE))
        );

        install_4k_mapping(&mut memory, pte(SECOND_DATA_PAGE, PTE_LEAF));
        mmu.sinval_vma(None, None);
        assert!(!mmu.invalidations_ordered_for_implicit_reads);
        assert_eq!(
            mmu.translate(&mut memory, config(ASID_ONE), TEST_VA, Access::Load),
            Ok(translation(SECOND_DATA_PAGE))
        );

        mmu.sfence_w_inval();
        mmu.sfence_inval_ir();
        assert!(mmu.writes_ordered_for_invalidation);
        assert!(mmu.invalidations_ordered_for_implicit_reads);

        mmu.note_explicit_write();
        assert!(!mmu.writes_ordered_for_invalidation);

        mmu.sfence_vma(None, None);
        assert!(mmu.writes_ordered_for_invalidation);
        assert!(mmu.invalidations_ordered_for_implicit_reads);
    }

    #[test]
    fn noncanonical_addresses_raise_page_faults() {
        let mut memory = Memory::with_size(MEMORY_BYTES);
        let mut mmu = Mmu::default();
        let noncanonical = 1 << (SV39_VIRTUAL_BITS - 1);

        assert_eq!(
            mmu.translate(&mut memory, config(ASID_ONE), noncanonical, Access::Load),
            Err(Fault {
                access: Access::Load,
                kind: FaultKind::Page
            })
        );
    }
}
