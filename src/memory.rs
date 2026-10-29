//! Flat, little-endian main memory.

use std::fmt;

use crate::bus::{Bus, MemoryType};
use crate::trap::Trap;

/// Default base address of main memory, the conventional RISC-V DRAM base.
pub const DRAM_BASE: u64 = 0x8000_0000;

const DEFAULT_DRAM_MIB: u64 = 128;

/// Default size of main memory in bytes.
pub const DEFAULT_DRAM_SIZE: u64 = DEFAULT_DRAM_MIB << 20;

/// Architectural page size used to align configurable DRAM ranges.
pub const DRAM_PAGE_BYTES: u64 = 1 << 12;

/// Physical address width declared by the platform.
pub const PHYSICAL_ADDRESS_BITS: u32 = 57;

const PHYSICAL_ADDRESS_LIMIT: u64 = 1 << PHYSICAL_ADDRESS_BITS;

/// A validated, page-aligned DRAM address range used by the CLI implementation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DramLayout {
    base: u64,
    size: u64,
}

impl DramLayout {
    /// Creates a DRAM layout that fits the architectural physical address space.
    pub fn new(base: u64, size: u64) -> Result<Self, DramLayoutError> {
        if size == 0 {
            return Err(DramLayoutError::ZeroSize);
        }
        if !base.is_multiple_of(DRAM_PAGE_BYTES) {
            return Err(DramLayoutError::MisalignedBase);
        }
        if !size.is_multiple_of(DRAM_PAGE_BYTES) {
            return Err(DramLayoutError::MisalignedSize);
        }
        let end = base
            .checked_add(size)
            .ok_or(DramLayoutError::AddressOverflow)?;
        if end > PHYSICAL_ADDRESS_LIMIT {
            return Err(DramLayoutError::ExceedsPhysicalAddressWidth);
        }
        Ok(Self { base, size })
    }

    /// Returns the first address in the DRAM range.
    pub const fn base(self) -> u64 {
        self.base
    }

    /// Returns the size of the DRAM range in bytes.
    pub const fn size(self) -> u64 {
        self.size
    }

    /// Returns the first address after the DRAM range.
    pub const fn end(self) -> u64 {
        self.base + self.size
    }
}

impl Default for DramLayout {
    fn default() -> Self {
        Self {
            base: DRAM_BASE,
            size: DEFAULT_DRAM_SIZE,
        }
    }
}

/// Why a DRAM range is not a legal architectural layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DramLayoutError {
    /// A zero-byte DRAM range is not supported.
    ZeroSize,
    /// The requested base is not page-aligned.
    MisalignedBase,
    /// The requested size is not a whole number of pages.
    MisalignedSize,
    /// The end address does not fit in `u64`.
    AddressOverflow,
    /// The end address exceeds the declared physical address width.
    ExceedsPhysicalAddressWidth,
}

impl fmt::Display for DramLayoutError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroSize => formatter.write_str("DRAM size must be greater than zero"),
            Self::MisalignedBase => {
                write!(
                    formatter,
                    "DRAM base must be aligned to {DRAM_PAGE_BYTES} bytes"
                )
            }
            Self::MisalignedSize => {
                write!(
                    formatter,
                    "DRAM size must be aligned to {DRAM_PAGE_BYTES} bytes"
                )
            }
            Self::AddressOverflow => formatter.write_str("DRAM address range overflows u64"),
            Self::ExceedsPhysicalAddressWidth => write!(
                formatter,
                "DRAM address range exceeds the {PHYSICAL_ADDRESS_BITS}-bit physical address width"
            ),
        }
    }
}

impl std::error::Error for DramLayoutError {}

/// Why a requested DRAM allocation could not be created.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryError {
    /// The architectural size cannot be represented by the host.
    SizeNotAddressable,
    /// The host rejected the allocation.
    AllocationFailed,
}

impl fmt::Display for MemoryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SizeNotAddressable => {
                formatter.write_str("DRAM size cannot be represented on this host")
            }
            Self::AllocationFailed => formatter.write_str("host could not allocate DRAM"),
        }
    }
}

impl std::error::Error for MemoryError {}

/// A flat byte array spanning a configured DRAM address range.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Memory {
    base: u64,
    bytes: Vec<u8>,
}

impl Memory {
    /// Creates zeroed main memory.
    pub fn new() -> Self {
        Self::try_with_layout(DramLayout::default())
            .expect("default DRAM layout must be allocatable")
    }

    /// Creates zeroed memory of `size` bytes, still based at `DRAM_BASE`.
    pub fn with_size(size: usize) -> Self {
        Self {
            base: DRAM_BASE,
            bytes: vec![0; size],
        }
    }

    /// Creates zeroed memory for a validated DRAM layout.
    pub fn try_with_layout(layout: DramLayout) -> Result<Self, MemoryError> {
        let size = usize::try_from(layout.size()).map_err(|_| MemoryError::SizeNotAddressable)?;
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(size)
            .map_err(|_| MemoryError::AllocationFailed)?;
        bytes.resize(size, 0);
        Ok(Self {
            base: layout.base(),
            bytes,
        })
    }

    /// Returns the address of the first byte in this memory region.
    pub const fn base(&self) -> u64 {
        self.base
    }

    /// Returns the size of this memory region in bytes.
    pub fn len(&self) -> u64 {
        self.bytes.len() as u64
    }

    /// Returns whether this memory region has no bytes.
    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    /// Translates `address` into a buffer offset, faulting when any of the
    /// `width` bytes fall outside memory.
    fn offset(&self, address: u64, width: u64) -> Result<usize, Trap> {
        let offset = address.checked_sub(self.base).ok_or(Trap::AccessFault)?;
        let end = offset.checked_add(width).ok_or(Trap::AccessFault)?;

        if end > self.bytes.len() as u64 {
            return Err(Trap::AccessFault);
        }
        // The bounds check above keeps `offset` within the host address space,
        // so the cast cannot truncate on 64-bit machines.
        Ok(offset as usize)
    }

    /// Reads `N` bytes little-endian, already in memory order.
    fn read_bytes<const N: usize>(&self, address: u64) -> Result<[u8; N], Trap> {
        let offset = self.offset(address, N as u64)?;
        let mut buffer = [0; N];
        buffer.copy_from_slice(&self.bytes[offset..offset + N]);
        Ok(buffer)
    }

    /// Writes `N` bytes little-endian, already in memory order.
    fn write_bytes<const N: usize>(&mut self, address: u64, bytes: [u8; N]) -> Result<(), Trap> {
        let offset = self.offset(address, N as u64)?;
        self.bytes[offset..offset + N].copy_from_slice(&bytes);
        Ok(())
    }
}

impl Default for Memory {
    fn default() -> Self {
        Self::new()
    }
}

impl Bus for Memory {
    fn read_u8(&mut self, address: u64, _memory_type: MemoryType) -> Result<u8, Trap> {
        self.read_bytes::<1>(address).map(|bytes| bytes[0])
    }

    fn write_u8(&mut self, address: u64, value: u8, _memory_type: MemoryType) -> Result<(), Trap> {
        self.write_bytes::<1>(address, [value])
    }

    fn read_u16(&mut self, address: u64, _memory_type: MemoryType) -> Result<u16, Trap> {
        self.read_bytes::<2>(address).map(u16::from_le_bytes)
    }

    fn write_u16(
        &mut self,
        address: u64,
        value: u16,
        _memory_type: MemoryType,
    ) -> Result<(), Trap> {
        self.write_bytes::<2>(address, value.to_le_bytes())
    }

    fn read_u32(&mut self, address: u64, _memory_type: MemoryType) -> Result<u32, Trap> {
        self.read_bytes::<4>(address).map(u32::from_le_bytes)
    }

    fn write_u32(
        &mut self,
        address: u64,
        value: u32,
        _memory_type: MemoryType,
    ) -> Result<(), Trap> {
        self.write_bytes::<4>(address, value.to_le_bytes())
    }

    fn read_u64(&mut self, address: u64, _memory_type: MemoryType) -> Result<u64, Trap> {
        self.read_bytes::<8>(address).map(u64::from_le_bytes)
    }

    fn write_u64(
        &mut self,
        address: u64,
        value: u64,
        _memory_type: MemoryType,
    ) -> Result<(), Trap> {
        self.write_bytes::<8>(address, value.to_le_bytes())
    }

    fn check_write(
        &mut self,
        address: u64,
        width: u64,
        _memory_type: MemoryType,
    ) -> Result<(), Trap> {
        self.offset(address, width).map(|_| ())
    }

    fn load_bytes(
        &mut self,
        address: u64,
        bytes: &[u8],
        _memory_type: MemoryType,
    ) -> Result<(), Trap> {
        // Check the whole span before copying so a fault leaves memory untouched.
        let offset = self.offset(address, bytes.len() as u64)?;
        self.bytes[offset..offset + bytes.len()].copy_from_slice(bytes);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_MEMORY_BYTES: usize = 0x100;
    const TEST_CUSTOM_BASE: u64 = DRAM_BASE + 0x1_0000;
    const TEST_PAGE_BYTES: u64 = 0x1000;
    const LAST_BYTE: u64 = DRAM_BASE + TEST_MEMORY_BYTES as u64 - 1;

    fn memory() -> Memory {
        Memory::with_size(TEST_MEMORY_BYTES)
    }

    #[test]
    fn dram_layout_defaults_to_the_documented_range() {
        let layout = DramLayout::default();

        assert_eq!(layout.base(), DRAM_BASE);
        assert_eq!(layout.size(), DEFAULT_DRAM_SIZE);
        assert_eq!(layout.end(), DRAM_BASE + DEFAULT_DRAM_SIZE);
    }

    #[test]
    fn dram_layout_rejects_invalid_ranges() {
        assert_eq!(
            DramLayout::new(DRAM_BASE, 0),
            Err(DramLayoutError::ZeroSize)
        );
        assert_eq!(
            DramLayout::new(DRAM_BASE + 1, TEST_PAGE_BYTES),
            Err(DramLayoutError::MisalignedBase)
        );
        assert_eq!(
            DramLayout::new(DRAM_BASE, TEST_PAGE_BYTES + 1),
            Err(DramLayoutError::MisalignedSize)
        );
        assert_eq!(
            DramLayout::new(u64::MAX - TEST_PAGE_BYTES + 1, TEST_PAGE_BYTES),
            Err(DramLayoutError::AddressOverflow)
        );
        assert_eq!(
            DramLayout::new(1 << PHYSICAL_ADDRESS_BITS, TEST_PAGE_BYTES),
            Err(DramLayoutError::ExceedsPhysicalAddressWidth)
        );
        assert!(DramLayout::new(PHYSICAL_ADDRESS_LIMIT - TEST_PAGE_BYTES, TEST_PAGE_BYTES).is_ok());
    }

    #[test]
    fn custom_layout_bounds_memory_to_its_configured_range() {
        let layout = DramLayout::new(TEST_CUSTOM_BASE, TEST_PAGE_BYTES).unwrap();
        let mut memory = Memory::try_with_layout(layout).unwrap();
        let last_byte = layout.end() - 1;

        memory
            .write_u8(layout.base(), 0xaa, MemoryType::Cacheable)
            .unwrap();
        memory
            .write_u8(last_byte, 0xbb, MemoryType::Cacheable)
            .unwrap();

        assert_eq!(
            memory
                .read_u8(layout.base(), MemoryType::Cacheable)
                .unwrap(),
            0xaa
        );
        assert_eq!(
            memory.read_u8(last_byte, MemoryType::Cacheable).unwrap(),
            0xbb
        );
        assert_eq!(
            memory.read_u8(layout.base() - 1, MemoryType::Cacheable),
            Err(Trap::AccessFault)
        );
        assert_eq!(
            memory.read_u8(layout.end(), MemoryType::Cacheable),
            Err(Trap::AccessFault)
        );
    }

    #[test]
    fn byte_round_trips() {
        let mut memory = memory();

        memory
            .write_u8(DRAM_BASE, 0xab, MemoryType::Cacheable)
            .unwrap();

        assert_eq!(
            memory.read_u8(DRAM_BASE, MemoryType::Cacheable).unwrap(),
            0xab
        );
    }

    #[test]
    fn halfword_is_little_endian() {
        let mut memory = memory();

        memory
            .write_u16(DRAM_BASE, 0x1234, MemoryType::Cacheable)
            .unwrap();

        assert_eq!(
            memory.read_u8(DRAM_BASE, MemoryType::Cacheable).unwrap(),
            0x34
        );
        assert_eq!(
            memory
                .read_u8(DRAM_BASE + 1, MemoryType::Cacheable)
                .unwrap(),
            0x12
        );
        assert_eq!(
            memory.read_u16(DRAM_BASE, MemoryType::Cacheable).unwrap(),
            0x1234
        );
    }

    #[test]
    fn word_and_doubleword_round_trip() {
        let mut memory = memory();

        memory
            .write_u32(DRAM_BASE, 0xdead_beef, MemoryType::Cacheable)
            .unwrap();
        memory
            .write_u64(DRAM_BASE + 8, 0x0123_4567_89ab_cdef, MemoryType::Cacheable)
            .unwrap();

        assert_eq!(
            memory.read_u32(DRAM_BASE, MemoryType::Cacheable).unwrap(),
            0xdead_beef
        );
        assert_eq!(
            memory
                .read_u64(DRAM_BASE + 8, MemoryType::Cacheable)
                .unwrap(),
            0x0123_4567_89ab_cdef
        );
    }

    #[test]
    fn misaligned_access_is_supported() {
        let mut memory = memory();

        memory
            .write_u64(DRAM_BASE + 1, 0x8877_6655_4433_2211, MemoryType::Cacheable)
            .unwrap();

        assert_eq!(
            memory
                .read_u64(DRAM_BASE + 1, MemoryType::Cacheable)
                .unwrap(),
            0x8877_6655_4433_2211
        );
        assert_eq!(
            memory
                .read_u8(DRAM_BASE + 1, MemoryType::Cacheable)
                .unwrap(),
            0x11
        );
    }

    #[test]
    fn load_bytes_then_read() {
        let mut memory = memory();

        memory
            .load_bytes(DRAM_BASE, &[0x13, 0x05, 0x10, 0x00], MemoryType::Cacheable)
            .unwrap();

        assert_eq!(
            memory.read_u32(DRAM_BASE, MemoryType::Cacheable).unwrap(),
            0x0010_0513
        );
    }

    #[test]
    fn reads_below_dram_fault() {
        let mut memory = memory();

        assert_eq!(
            memory.read_u8(DRAM_BASE - 1, MemoryType::Cacheable),
            Err(Trap::AccessFault)
        );
    }

    #[test]
    fn reads_past_end_fault() {
        let mut memory = memory();

        assert_eq!(
            memory.read_u8(LAST_BYTE, MemoryType::Cacheable).map(|_| ()),
            Ok(())
        );
        assert_eq!(
            memory.read_u8(LAST_BYTE + 1, MemoryType::Cacheable),
            Err(Trap::AccessFault)
        );
        assert_eq!(
            memory.read_u32(LAST_BYTE, MemoryType::Cacheable),
            Err(Trap::AccessFault)
        );
    }

    #[test]
    fn writes_past_end_fault() {
        let mut memory = memory();

        assert_eq!(
            memory.write_u8(LAST_BYTE + 1, 0, MemoryType::Cacheable),
            Err(Trap::AccessFault)
        );
        assert_eq!(
            memory.write_u64(LAST_BYTE - 3, 0, MemoryType::Cacheable),
            Err(Trap::AccessFault)
        );
    }

    #[test]
    fn load_bytes_beyond_end_faults() {
        let mut memory = memory();

        assert_eq!(
            memory.load_bytes(LAST_BYTE, &[0, 1], MemoryType::Cacheable),
            Err(Trap::AccessFault)
        );
    }

    #[test]
    fn check_write_checks_the_entire_span_without_mutating_memory() {
        let mut memory = Memory::with_size(16);
        memory
            .write_u64(DRAM_BASE + 8, u64::MAX, MemoryType::Cacheable)
            .unwrap();

        memory
            .check_write(DRAM_BASE + 8, 8, MemoryType::Cacheable)
            .unwrap();
        assert_eq!(
            memory
                .read_u64(DRAM_BASE + 8, MemoryType::Cacheable)
                .unwrap(),
            u64::MAX
        );
        assert_eq!(
            memory.check_write(DRAM_BASE + 8, 16, MemoryType::Cacheable),
            Err(Trap::AccessFault)
        );
        assert_eq!(
            memory
                .read_u64(DRAM_BASE + 8, MemoryType::Cacheable)
                .unwrap(),
            u64::MAX
        );
    }

    #[test]
    fn with_size_bounds_the_region() {
        let mut memory = Memory::with_size(16);

        memory
            .write_u64(DRAM_BASE + 8, u64::MAX, MemoryType::Cacheable)
            .unwrap();

        assert_eq!(
            memory
                .read_u64(DRAM_BASE + 8, MemoryType::Cacheable)
                .unwrap(),
            u64::MAX
        );
        assert_eq!(
            memory.read_u8(DRAM_BASE + 16, MemoryType::Cacheable),
            Err(Trap::AccessFault)
        );
    }

    #[test]
    fn wide_access_near_the_top_of_the_address_space_faults() {
        let mut memory = memory();

        assert_eq!(
            memory.read_u8(u64::MAX, MemoryType::Cacheable),
            Err(Trap::AccessFault)
        );
        assert_eq!(
            memory.read_u64(u64::MAX - 3, MemoryType::Cacheable),
            Err(Trap::AccessFault)
        );
    }

    #[test]
    fn default_matches_new() {
        assert_eq!(Memory::default().len(), DEFAULT_DRAM_SIZE);
        assert_eq!(Memory::new().len(), DEFAULT_DRAM_SIZE);
    }

    #[test]
    fn memory_is_usable_as_a_dyn_bus() {
        let mut memory = memory();
        let bus: &mut dyn Bus = &mut memory;

        bus.write_u32(DRAM_BASE, 0x0000_0013, MemoryType::Cacheable)
            .unwrap();

        assert_eq!(
            bus.read_u32(DRAM_BASE, MemoryType::Cacheable).unwrap(),
            0x0000_0013
        );
    }
}
