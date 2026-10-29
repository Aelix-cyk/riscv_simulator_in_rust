//! Byte-addressable bus abstraction.

use crate::trap::Trap;

/// Architectural memory type used for one access.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryType {
    /// Cacheable memory selected by the platform's physical-memory attributes.
    Cacheable,
    /// Non-cacheable, idempotent memory.
    NonCacheable,
    /// Strongly ordered I/O memory.
    Io,
}

/// A little-endian bus. Implementations model main memory or a device.
pub trait Bus {
    /// Reads one byte.
    fn read_u8(&mut self, address: u64, memory_type: MemoryType) -> Result<u8, Trap>;
    /// Writes one byte.
    fn write_u8(&mut self, address: u64, value: u8, memory_type: MemoryType) -> Result<(), Trap>;
    /// Reads a halfword, little-endian.
    fn read_u16(&mut self, address: u64, memory_type: MemoryType) -> Result<u16, Trap> {
        Ok(u16::from_le_bytes(read_le::<Self, 2>(
            self,
            address,
            memory_type,
        )?))
    }
    /// Writes a halfword, little-endian.
    fn write_u16(&mut self, address: u64, value: u16, memory_type: MemoryType) -> Result<(), Trap> {
        write_le::<Self, 2>(self, address, value.to_le_bytes(), memory_type)
    }
    /// Reads a word, little-endian.
    fn read_u32(&mut self, address: u64, memory_type: MemoryType) -> Result<u32, Trap> {
        Ok(u32::from_le_bytes(read_le::<Self, 4>(
            self,
            address,
            memory_type,
        )?))
    }
    /// Writes a word, little-endian.
    fn write_u32(&mut self, address: u64, value: u32, memory_type: MemoryType) -> Result<(), Trap> {
        write_le::<Self, 4>(self, address, value.to_le_bytes(), memory_type)
    }
    /// Reads a doubleword, little-endian.
    fn read_u64(&mut self, address: u64, memory_type: MemoryType) -> Result<u64, Trap> {
        Ok(u64::from_le_bytes(read_le::<Self, 8>(
            self,
            address,
            memory_type,
        )?))
    }
    /// Writes a doubleword, little-endian.
    fn write_u64(&mut self, address: u64, value: u64, memory_type: MemoryType) -> Result<(), Trap> {
        write_le::<Self, 8>(self, address, value.to_le_bytes(), memory_type)
    }
    /// Checks whether a write of `width` bytes at `address` is permitted
    /// without changing memory.
    ///
    /// The default probes read access byte by byte. Implementations whose read
    /// and write permissions differ must override this method.
    fn check_write(
        &mut self,
        address: u64,
        width: u64,
        memory_type: MemoryType,
    ) -> Result<(), Trap> {
        for offset in 0..width {
            self.read_u8(offset_address(address, offset)?, memory_type)?;
        }
        Ok(())
    }
    /// Copies `bytes` starting at `address`.
    ///
    /// The default writes byte by byte and may leave a partial image when a write
    /// faults partway through.
    fn load_bytes(
        &mut self,
        address: u64,
        bytes: &[u8],
        memory_type: MemoryType,
    ) -> Result<(), Trap> {
        for (index, byte) in bytes.iter().enumerate() {
            self.write_u8(offset_address(address, index as u64)?, *byte, memory_type)?;
        }
        Ok(())
    }
}

/// Reads `N` bytes through [`Bus::read_u8`].
fn read_le<B: Bus + ?Sized, const N: usize>(
    bus: &mut B,
    address: u64,
    memory_type: MemoryType,
) -> Result<[u8; N], Trap> {
    let mut bytes = [0; N];
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = bus.read_u8(offset_address(address, index as u64)?, memory_type)?;
    }
    Ok(bytes)
}

/// Writes `N` bytes through [`Bus::write_u8`].
fn write_le<B: Bus + ?Sized, const N: usize>(
    bus: &mut B,
    address: u64,
    bytes: [u8; N],
    memory_type: MemoryType,
) -> Result<(), Trap> {
    for (index, byte) in bytes.iter().enumerate() {
        bus.write_u8(offset_address(address, index as u64)?, *byte, memory_type)?;
    }
    Ok(())
}

/// Adds a byte offset to an address, faulting on overflow.
fn offset_address(address: u64, offset: u64) -> Result<u64, Trap> {
    address.checked_add(offset).ok_or(Trap::AccessFault)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A device that implements only the byte-wide methods, so every wider
    /// access goes through the trait's defaults.
    struct TinyDevice {
        bytes: [u8; 8],
        reads: u64,
        last_memory_type: Option<MemoryType>,
    }

    impl TinyDevice {
        fn new() -> Self {
            Self {
                bytes: [0; 8],
                reads: 0,
                last_memory_type: None,
            }
        }

        fn index(address: u64) -> usize {
            // 64-bit machines only: the cast cannot truncate.
            address as usize
        }
    }

    impl Bus for TinyDevice {
        fn read_u8(&mut self, address: u64, memory_type: MemoryType) -> Result<u8, Trap> {
            let index = Self::index(address);
            self.reads += 1;
            self.last_memory_type = Some(memory_type);
            self.bytes.get(index).copied().ok_or(Trap::AccessFault)
        }

        fn write_u8(
            &mut self,
            address: u64,
            value: u8,
            memory_type: MemoryType,
        ) -> Result<(), Trap> {
            let index = Self::index(address);
            self.last_memory_type = Some(memory_type);
            *self.bytes.get_mut(index).ok_or(Trap::AccessFault)? = value;
            Ok(())
        }
    }

    #[test]
    fn default_wide_reads_are_little_endian() {
        let mut device = TinyDevice::new();
        device.write_u8(0, 0x01, MemoryType::Cacheable).unwrap();
        device.write_u8(1, 0x02, MemoryType::Cacheable).unwrap();
        device.write_u8(2, 0x03, MemoryType::Cacheable).unwrap();
        device.write_u8(3, 0x04, MemoryType::Cacheable).unwrap();

        assert_eq!(device.read_u16(0, MemoryType::Cacheable).unwrap(), 0x0201);
        assert_eq!(
            device.read_u32(0, MemoryType::Cacheable).unwrap(),
            0x0403_0201
        );
    }

    #[test]
    fn default_wide_writes_round_trip() {
        let mut device = TinyDevice::new();

        device
            .write_u64(0, 0x0102_0304_0506_0708, MemoryType::Cacheable)
            .unwrap();

        assert_eq!(device.read_u8(0, MemoryType::Cacheable).unwrap(), 0x08);
        assert_eq!(
            device.read_u64(0, MemoryType::Cacheable).unwrap(),
            0x0102_0304_0506_0708
        );
    }

    #[test]
    fn default_16_and_32_bit_writes_are_little_endian() {
        let mut device = TinyDevice::new();

        device.write_u16(2, 0x1234, MemoryType::Cacheable).unwrap();
        device
            .write_u32(4, 0xdead_beef, MemoryType::Cacheable)
            .unwrap();

        assert_eq!(device.bytes[2..4], [0x34, 0x12]);
        assert_eq!(device.bytes[4..8], [0xef, 0xbe, 0xad, 0xde]);
    }

    #[test]
    fn default_wide_reads_go_through_read_u8() {
        let mut device = TinyDevice::new();

        let _ = device.read_u32(0, MemoryType::Cacheable).unwrap();

        assert_eq!(device.reads, 4, "a default wide read is four read_u8 calls");
    }

    #[test]
    fn default_load_bytes_then_read() {
        let mut device = TinyDevice::new();

        device
            .load_bytes(0, &[0xaa, 0xbb, 0xcc, 0xdd], MemoryType::Cacheable)
            .unwrap();

        assert_eq!(
            device.read_u32(0, MemoryType::Cacheable).unwrap(),
            0xddcc_bbaa
        );
    }

    #[test]
    fn default_methods_propagate_faults() {
        let mut device = TinyDevice::new();

        assert_eq!(
            device.read_u32(6, MemoryType::Cacheable),
            Err(Trap::AccessFault)
        );
        assert_eq!(
            device.write_u64(4, 0, MemoryType::Cacheable),
            Err(Trap::AccessFault)
        );
        assert_eq!(
            device.check_write(6, 4, MemoryType::Cacheable),
            Err(Trap::AccessFault)
        );
        assert_eq!(
            device.load_bytes(7, &[0, 1], MemoryType::Cacheable),
            Err(Trap::AccessFault)
        );
    }

    #[test]
    fn default_methods_forward_memory_type() {
        let mut device = TinyDevice::new();

        device.write_u32(0, 0xdead_beef, MemoryType::Io).unwrap();
        assert_eq!(device.last_memory_type, Some(MemoryType::Io));

        device.read_u64(0, MemoryType::NonCacheable).unwrap();
        assert_eq!(device.last_memory_type, Some(MemoryType::NonCacheable));
    }
}
