//! Flat, little-endian main memory.

use crate::bus::Bus;
use crate::trap::Trap;

/// Base address of main memory, the conventional RISC-V DRAM base.
pub const DRAM_BASE: u64 = 0x8000_0000;

/// Size of main memory in bytes.
pub const DRAM_SIZE: u64 = 1 << 20;

/// A flat byte array spanning `DRAM_BASE .. DRAM_BASE + DRAM_SIZE`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Memory {
    bytes: Vec<u8>,
}

impl Memory {
    /// Creates zeroed main memory.
    pub fn new() -> Self {
        // 64-bit machines only: `usize` is 64 bits wide, so this cannot truncate.
        Self::with_size(DRAM_SIZE as usize)
    }

    /// Creates zeroed memory of `size` bytes, still based at `DRAM_BASE`.
    pub fn with_size(size: usize) -> Self {
        Self {
            bytes: vec![0; size],
        }
    }

    /// Translates `address` into a buffer offset, faulting when any of the
    /// `width` bytes fall outside memory.
    fn offset(&self, address: u64, width: u64) -> Result<usize, Trap> {
        let offset = address.checked_sub(DRAM_BASE).ok_or(Trap::AccessFault)?;
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
    fn read_u8(&mut self, address: u64) -> Result<u8, Trap> {
        self.read_bytes::<1>(address).map(|bytes| bytes[0])
    }

    fn write_u8(&mut self, address: u64, value: u8) -> Result<(), Trap> {
        self.write_bytes::<1>(address, [value])
    }

    fn read_u16(&mut self, address: u64) -> Result<u16, Trap> {
        self.read_bytes::<2>(address).map(u16::from_le_bytes)
    }

    fn write_u16(&mut self, address: u64, value: u16) -> Result<(), Trap> {
        self.write_bytes::<2>(address, value.to_le_bytes())
    }

    fn read_u32(&mut self, address: u64) -> Result<u32, Trap> {
        self.read_bytes::<4>(address).map(u32::from_le_bytes)
    }

    fn write_u32(&mut self, address: u64, value: u32) -> Result<(), Trap> {
        self.write_bytes::<4>(address, value.to_le_bytes())
    }

    fn read_u64(&mut self, address: u64) -> Result<u64, Trap> {
        self.read_bytes::<8>(address).map(u64::from_le_bytes)
    }

    fn write_u64(&mut self, address: u64, value: u64) -> Result<(), Trap> {
        self.write_bytes::<8>(address, value.to_le_bytes())
    }

    fn check_write(&mut self, address: u64, width: u64) -> Result<(), Trap> {
        self.offset(address, width).map(|_| ())
    }

    fn load_bytes(&mut self, address: u64, bytes: &[u8]) -> Result<(), Trap> {
        // Check the whole span before copying so a fault leaves memory untouched.
        let offset = self.offset(address, bytes.len() as u64)?;
        self.bytes[offset..offset + bytes.len()].copy_from_slice(bytes);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LAST_BYTE: u64 = DRAM_BASE + DRAM_SIZE - 1;

    #[test]
    fn byte_round_trips() {
        let mut memory = Memory::new();

        memory.write_u8(DRAM_BASE, 0xab).unwrap();

        assert_eq!(memory.read_u8(DRAM_BASE).unwrap(), 0xab);
    }

    #[test]
    fn halfword_is_little_endian() {
        let mut memory = Memory::new();

        memory.write_u16(DRAM_BASE, 0x1234).unwrap();

        assert_eq!(memory.read_u8(DRAM_BASE).unwrap(), 0x34);
        assert_eq!(memory.read_u8(DRAM_BASE + 1).unwrap(), 0x12);
        assert_eq!(memory.read_u16(DRAM_BASE).unwrap(), 0x1234);
    }

    #[test]
    fn word_and_doubleword_round_trip() {
        let mut memory = Memory::new();

        memory.write_u32(DRAM_BASE, 0xdead_beef).unwrap();
        memory
            .write_u64(DRAM_BASE + 8, 0x0123_4567_89ab_cdef)
            .unwrap();

        assert_eq!(memory.read_u32(DRAM_BASE).unwrap(), 0xdead_beef);
        assert_eq!(
            memory.read_u64(DRAM_BASE + 8).unwrap(),
            0x0123_4567_89ab_cdef
        );
    }

    #[test]
    fn misaligned_access_is_supported() {
        let mut memory = Memory::new();

        memory
            .write_u64(DRAM_BASE + 1, 0x8877_6655_4433_2211)
            .unwrap();

        assert_eq!(
            memory.read_u64(DRAM_BASE + 1).unwrap(),
            0x8877_6655_4433_2211
        );
        assert_eq!(memory.read_u8(DRAM_BASE + 1).unwrap(), 0x11);
    }

    #[test]
    fn load_bytes_then_read() {
        let mut memory = Memory::new();

        memory
            .load_bytes(DRAM_BASE, &[0x13, 0x05, 0x10, 0x00])
            .unwrap();

        assert_eq!(memory.read_u32(DRAM_BASE).unwrap(), 0x0010_0513);
    }

    #[test]
    fn reads_below_dram_fault() {
        let mut memory = Memory::new();

        assert_eq!(memory.read_u8(DRAM_BASE - 1), Err(Trap::AccessFault));
    }

    #[test]
    fn reads_past_end_fault() {
        let mut memory = Memory::new();

        assert_eq!(memory.read_u8(LAST_BYTE).map(|_| ()), Ok(()));
        assert_eq!(memory.read_u8(LAST_BYTE + 1), Err(Trap::AccessFault));
        assert_eq!(memory.read_u32(LAST_BYTE), Err(Trap::AccessFault));
    }

    #[test]
    fn writes_past_end_fault() {
        let mut memory = Memory::new();

        assert_eq!(memory.write_u8(LAST_BYTE + 1, 0), Err(Trap::AccessFault));
        assert_eq!(memory.write_u64(LAST_BYTE - 3, 0), Err(Trap::AccessFault));
    }

    #[test]
    fn load_bytes_beyond_end_faults() {
        let mut memory = Memory::new();

        assert_eq!(
            memory.load_bytes(LAST_BYTE, &[0, 1]),
            Err(Trap::AccessFault)
        );
    }

    #[test]
    fn check_write_checks_the_entire_span_without_mutating_memory() {
        let mut memory = Memory::with_size(16);
        memory.write_u64(DRAM_BASE + 8, u64::MAX).unwrap();

        memory.check_write(DRAM_BASE + 8, 8).unwrap();
        assert_eq!(memory.read_u64(DRAM_BASE + 8).unwrap(), u64::MAX);
        assert_eq!(
            memory.check_write(DRAM_BASE + 8, 16),
            Err(Trap::AccessFault)
        );
        assert_eq!(memory.read_u64(DRAM_BASE + 8).unwrap(), u64::MAX);
    }

    #[test]
    fn with_size_bounds_the_region() {
        let mut memory = Memory::with_size(16);

        memory.write_u64(DRAM_BASE + 8, u64::MAX).unwrap();

        assert_eq!(memory.read_u64(DRAM_BASE + 8).unwrap(), u64::MAX);
        assert_eq!(memory.read_u8(DRAM_BASE + 16), Err(Trap::AccessFault));
    }

    #[test]
    fn wide_access_near_the_top_of_the_address_space_faults() {
        let mut memory = Memory::new();

        assert_eq!(memory.read_u8(u64::MAX), Err(Trap::AccessFault));
        assert_eq!(memory.read_u64(u64::MAX - 3), Err(Trap::AccessFault));
    }

    #[test]
    fn default_matches_new() {
        assert_eq!(Memory::default(), Memory::new());
    }

    #[test]
    fn memory_is_usable_as_a_dyn_bus() {
        let mut memory = Memory::new();
        let bus: &mut dyn Bus = &mut memory;

        bus.write_u32(DRAM_BASE, 0x0000_0013).unwrap();

        assert_eq!(bus.read_u32(DRAM_BASE).unwrap(), 0x0000_0013);
    }
}
