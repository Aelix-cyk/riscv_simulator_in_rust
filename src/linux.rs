//! Raw RISC-V Linux Image validation and boot-time DTB patching.

use std::fmt;

use fdt_edit::{Fdt, Node, Property};

use crate::bus::{Bus, MemoryType};
use crate::memory::DramLayout;
use crate::trap::Trap;

/// Size of the RISC-V Linux Image header.
pub const IMAGE_HEADER_BYTES: usize = 64;

/// Load offset encoded in an RV64 S-mode Linux Image.
pub const IMAGE_LOAD_OFFSET: u64 = 0x20_0000;

/// Header version currently produced by Linux.
pub const IMAGE_VERSION_MAJOR: u32 = 0;
pub const IMAGE_VERSION_MINOR: u32 = 2;

/// Default command line for the project-owned aRVSR platform.
pub const DEFAULT_BOOTARGS: &str = "console=ttyS0 earlycon=sbi";

const IMAGE_TEXT_OFFSET_OFFSET: usize = 8;
const IMAGE_SIZE_OFFSET: usize = 16;
const IMAGE_FLAGS_OFFSET: usize = 24;
const IMAGE_VERSION_OFFSET: usize = 32;
const IMAGE_MAGIC_OFFSET: usize = 48;
const IMAGE_MAGIC2_OFFSET: usize = 56;
const IMAGE_MAGIC: u64 = 0x0056_4353_4952;
const IMAGE_MAGIC2: u32 = 0x0543_5352;
const IMAGE_FLAGS_MASK: u64 = 0b1;
const IMAGE_BIG_ENDIAN: u64 = 0b1;
const IMAGE_VERSION_MINOR_MASK: u32 = u16::MAX as u32;
const IMAGE_VERSION_MAJOR_SHIFT: u32 = u16::BITS;
const ZERO_CHUNK_BYTES: usize = 4 << 10;
const ZERO_CHUNK: [u8; ZERO_CHUNK_BYTES] = [0; ZERO_CHUNK_BYTES];

/// A validated raw RISC-V Linux Image.
#[derive(Debug, Clone, Copy)]
pub struct LinuxImage<'data> {
    data: &'data [u8],
    load_address: u64,
    image_size: u64,
}

impl<'data> LinuxImage<'data> {
    /// Validates the image header and its effective memory range.
    pub fn parse(
        data: &'data [u8],
        dram: DramLayout,
        load_address: u64,
    ) -> Result<Self, LinuxImageError> {
        if data.len() < IMAGE_HEADER_BYTES {
            return Err(LinuxImageError::TruncatedHeader);
        }

        let text_offset = read_u64(data, IMAGE_TEXT_OFFSET_OFFSET);
        let expected_offset = load_address
            .checked_sub(dram.base())
            .ok_or(LinuxImageError::LoadAddressOutsideDram)?;
        if text_offset != expected_offset {
            return Err(LinuxImageError::UnexpectedTextOffset {
                expected: expected_offset,
                actual: text_offset,
            });
        }

        let flags = read_u64(data, IMAGE_FLAGS_OFFSET);
        if flags & IMAGE_BIG_ENDIAN != 0 {
            return Err(LinuxImageError::BigEndian);
        }
        if flags & !IMAGE_FLAGS_MASK != 0 {
            return Err(LinuxImageError::UnsupportedFlags {
                flags: flags & !IMAGE_FLAGS_MASK,
            });
        }

        let version = read_u32(data, IMAGE_VERSION_OFFSET);
        let major = version >> IMAGE_VERSION_MAJOR_SHIFT;
        let minor = version & IMAGE_VERSION_MINOR_MASK;
        if major != IMAGE_VERSION_MAJOR || minor < IMAGE_VERSION_MINOR {
            return Err(LinuxImageError::UnsupportedVersion { major, minor });
        }

        if read_u64(data, IMAGE_MAGIC_OFFSET) != IMAGE_MAGIC
            || read_u32(data, IMAGE_MAGIC2_OFFSET) != IMAGE_MAGIC2
        {
            return Err(LinuxImageError::BadMagic);
        }

        let image_size = read_u64(data, IMAGE_SIZE_OFFSET);
        if image_size < data.len() as u64 {
            return Err(LinuxImageError::ImageSizeTooSmall {
                declared: image_size,
                file_bytes: data.len() as u64,
            });
        }
        let end = load_address
            .checked_add(image_size)
            .ok_or(LinuxImageError::AddressOverflow)?;
        if end > dram.end() {
            return Err(LinuxImageError::OutsideDram {
                start: load_address,
                end,
                dram_end: dram.end(),
            });
        }

        Ok(Self {
            data,
            load_address,
            image_size,
        })
    }

    /// Returns the architectural entry address.
    pub const fn entry(self) -> u64 {
        self.load_address
    }

    /// Returns the half-open effective memory range, including the BSS tail.
    pub fn range(self) -> (u64, u64) {
        (self.load_address, self.load_address + self.image_size)
    }

    /// Loads file-backed bytes and clears the zero-initialized tail.
    pub fn load<B: Bus>(self, bus: &mut B) -> Result<(), LinuxImageError> {
        bus.check_write(self.load_address, self.image_size, MemoryType::Cacheable)
            .map_err(LinuxImageError::Access)?;
        bus.load_bytes(self.load_address, self.data, MemoryType::Cacheable)
            .map_err(LinuxImageError::Access)?;

        let mut address = self.load_address + self.data.len() as u64;
        let image_end = self.load_address + self.image_size;
        while address < image_end {
            let remaining = image_end - address;
            let chunk_bytes = remaining.min(ZERO_CHUNK_BYTES as u64) as usize;
            bus.load_bytes(address, &ZERO_CHUNK[..chunk_bytes], MemoryType::Cacheable)
                .map_err(LinuxImageError::Access)?;
            address += chunk_bytes as u64;
        }
        Ok(())
    }
}

/// Patches Linux boot information into the `/chosen` node.
pub fn patch_device_tree(
    device_tree: &[u8],
    initrd: Option<(u64, u64)>,
    bootargs: &str,
) -> Result<Vec<u8>, LinuxImageError> {
    let mut fdt = Fdt::from_bytes(device_tree)
        .map_err(|error| LinuxImageError::DeviceTree(error.to_string()))?;
    let chosen = match fdt.get_by_path_id("/chosen") {
        Some(id) => id,
        None => fdt.add_node(fdt.root_id(), Node::new("chosen")),
    };
    let chosen = fdt
        .node_mut(chosen)
        .ok_or(LinuxImageError::MissingChosenNode)?;

    let mut bootargs_property = Property::new("bootargs", Vec::new());
    bootargs_property.set_string(bootargs);
    chosen.set_property(bootargs_property);

    match initrd {
        Some((start, end)) => {
            let mut start_property = Property::new("linux,initrd-start", Vec::new());
            start_property.set_u64(start);
            chosen.set_property(start_property);

            let mut end_property = Property::new("linux,initrd-end", Vec::new());
            end_property.set_u64(end);
            chosen.set_property(end_property);
        }
        None => {
            chosen.remove_property("linux,initrd-start");
            chosen.remove_property("linux,initrd-end");
        }
    }

    Ok(fdt.encode().as_ref().to_vec())
}

fn read_u32(data: &[u8], offset: usize) -> u32 {
    let bytes: [u8; 4] = data[offset..offset + 4]
        .try_into()
        .expect("four-byte slice");
    u32::from_le_bytes(bytes)
}

fn read_u64(data: &[u8], offset: usize) -> u64 {
    let bytes: [u8; 8] = data[offset..offset + 8]
        .try_into()
        .expect("eight-byte slice");
    u64::from_le_bytes(bytes)
}

/// Why a raw RISC-V Linux Image cannot be loaded.
#[derive(Debug)]
pub enum LinuxImageError {
    /// The 64-byte RISC-V Image header is incomplete.
    TruncatedHeader,
    /// The configured load address lies outside DRAM.
    LoadAddressOutsideDram,
    /// The header describes a different physical load offset.
    UnexpectedTextOffset { expected: u64, actual: u64 },
    /// The image requests an unsupported byte order.
    BigEndian,
    /// The header contains flags this loader does not understand.
    UnsupportedFlags { flags: u64 },
    /// The header version is not compatible with the documented 0.2 format.
    UnsupportedVersion { major: u32, minor: u32 },
    /// Neither RISC-V Image magic field matched.
    BadMagic,
    /// The effective image size cannot contain the supplied file data.
    ImageSizeTooSmall { declared: u64, file_bytes: u64 },
    /// Adding the effective image size overflows `u64`.
    AddressOverflow,
    /// The effective image extends past the configured DRAM range.
    OutsideDram { start: u64, end: u64, dram_end: u64 },
    /// The device-tree input is malformed.
    DeviceTree(String),
    /// The editable device tree lost its selected `/chosen` node.
    MissingChosenNode,
    /// The destination bus rejected an access.
    Access(Trap),
}

impl fmt::Display for LinuxImageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TruncatedHeader => {
                write!(
                    formatter,
                    "Linux Image is shorter than {IMAGE_HEADER_BYTES} bytes"
                )
            }
            Self::LoadAddressOutsideDram => {
                formatter.write_str("Linux Image load address is outside DRAM")
            }
            Self::UnexpectedTextOffset { expected, actual } => write!(
                formatter,
                "Linux Image text offset is {actual:#x}; expected {expected:#x}"
            ),
            Self::BigEndian => formatter.write_str("big-endian Linux Images are not supported"),
            Self::UnsupportedFlags { flags } => {
                write!(formatter, "Linux Image has unsupported flags {flags:#x}")
            }
            Self::UnsupportedVersion { major, minor } => write!(
                formatter,
                "Linux Image header version {major}.{minor} is incompatible with \
                 major {IMAGE_VERSION_MAJOR}, minimum minor {IMAGE_VERSION_MINOR}"
            ),
            Self::BadMagic => formatter.write_str("input is not a RISC-V Linux Image"),
            Self::ImageSizeTooSmall {
                declared,
                file_bytes,
            } => write!(
                formatter,
                "Linux Image declares {declared} bytes but the file contains {file_bytes}"
            ),
            Self::AddressOverflow => formatter.write_str("Linux Image address range overflows u64"),
            Self::OutsideDram {
                start,
                end,
                dram_end,
            } => write!(
                formatter,
                "Linux Image range {start:#x}..{end:#x} exceeds DRAM end {dram_end:#x}"
            ),
            Self::DeviceTree(error) => write!(formatter, "invalid device tree: {error}"),
            Self::MissingChosenNode => formatter.write_str("cannot update /chosen"),
            Self::Access(trap) => write!(formatter, "cannot load Linux Image: {trap:?}"),
        }
    }
}

impl std::error::Error for LinuxImageError {}

#[cfg(test)]
mod tests {
    use super::*;
    use fdt_edit::Fdt;

    use crate::memory::{DRAM_BASE, Memory};

    const TEST_DRAM_SIZE: u64 = 8 << 20;
    const TEST_IMAGE_SIZE: u64 = 0x3000;
    const TEST_FILE_BYTES: usize = 0x1000;
    const TEST_LOAD_ADDRESS: u64 = DRAM_BASE + IMAGE_LOAD_OFFSET;
    const TEST_INITRD_START: u64 = TEST_LOAD_ADDRESS + TEST_IMAGE_SIZE;
    const TEST_INITRD_END: u64 = TEST_INITRD_START + TEST_FILE_BYTES as u64;
    const TEST_BOOTARGS: &str = "console=ttyS0 test";
    const HEADER_VERSION: u32 =
        (IMAGE_VERSION_MAJOR << IMAGE_VERSION_MAJOR_SHIFT) | IMAGE_VERSION_MINOR;

    fn valid_image(image_size: u64) -> Vec<u8> {
        let mut image = vec![0xaa; TEST_FILE_BYTES.max(IMAGE_HEADER_BYTES)];
        image[IMAGE_FLAGS_OFFSET..IMAGE_FLAGS_OFFSET + 8].fill(0);
        image[IMAGE_TEXT_OFFSET_OFFSET..IMAGE_TEXT_OFFSET_OFFSET + 8]
            .copy_from_slice(&IMAGE_LOAD_OFFSET.to_le_bytes());
        image[IMAGE_SIZE_OFFSET..IMAGE_SIZE_OFFSET + 8].copy_from_slice(&image_size.to_le_bytes());
        image[IMAGE_VERSION_OFFSET..IMAGE_VERSION_OFFSET + 4]
            .copy_from_slice(&HEADER_VERSION.to_le_bytes());
        image[IMAGE_MAGIC_OFFSET..IMAGE_MAGIC_OFFSET + 8]
            .copy_from_slice(&IMAGE_MAGIC.to_le_bytes());
        image[IMAGE_MAGIC2_OFFSET..IMAGE_MAGIC2_OFFSET + 4]
            .copy_from_slice(&IMAGE_MAGIC2.to_le_bytes());
        image
    }

    fn test_layout() -> DramLayout {
        DramLayout::new(DRAM_BASE, TEST_DRAM_SIZE).unwrap()
    }

    fn tree_with_chosen() -> Fdt {
        let mut fdt = Fdt::new();
        fdt.add_node(fdt.root_id(), Node::new("chosen"));
        fdt
    }

    #[test]
    fn linux_image_loads_file_data_and_clears_its_tail() {
        let image_bytes = valid_image(TEST_IMAGE_SIZE);
        let image = LinuxImage::parse(&image_bytes, test_layout(), TEST_LOAD_ADDRESS).unwrap();
        let mut memory = Memory::with_size(TEST_DRAM_SIZE as usize);

        image.load(&mut memory).unwrap();

        assert_eq!(image.entry(), TEST_LOAD_ADDRESS);
        assert_eq!(
            memory
                .read_u8(
                    TEST_LOAD_ADDRESS + image_bytes.len() as u64,
                    MemoryType::Cacheable
                )
                .unwrap(),
            0
        );
    }

    #[test]
    fn linux_image_rejects_bad_header_and_range() {
        let valid = valid_image(TEST_IMAGE_SIZE);
        assert!(LinuxImage::parse(&valid, test_layout(), TEST_LOAD_ADDRESS).is_ok());
        assert!(
            LinuxImage::parse(
                &valid[..IMAGE_HEADER_BYTES - 1],
                test_layout(),
                TEST_LOAD_ADDRESS
            )
            .is_err()
        );

        let mut bad_magic = valid.clone();
        bad_magic[IMAGE_MAGIC2_OFFSET] ^= u8::MAX;
        assert!(LinuxImage::parse(&bad_magic, test_layout(), TEST_LOAD_ADDRESS).is_err());

        let mut truncated_size = valid;
        truncated_size[IMAGE_SIZE_OFFSET..IMAGE_SIZE_OFFSET + 8]
            .copy_from_slice(&(TEST_FILE_BYTES as u64 - 1).to_le_bytes());
        assert!(LinuxImage::parse(&truncated_size, test_layout(), TEST_LOAD_ADDRESS).is_err());
    }

    #[test]
    fn device_tree_patch_round_trips_boot_information() {
        let encoded = tree_with_chosen().encode();
        let patched = patch_device_tree(
            encoded.as_ref(),
            Some((TEST_INITRD_START, TEST_INITRD_END)),
            TEST_BOOTARGS,
        )
        .unwrap();
        let fdt = Fdt::from_bytes(&patched).unwrap();
        let chosen = fdt.get_by_path_id("/chosen").unwrap();
        let chosen = fdt.node(chosen).unwrap();

        assert_eq!(
            chosen
                .get_property("linux,initrd-start")
                .and_then(Property::get_u64),
            Some(TEST_INITRD_START)
        );
        assert_eq!(
            chosen
                .get_property("linux,initrd-end")
                .and_then(Property::get_u64),
            Some(TEST_INITRD_END)
        );
        assert_eq!(
            chosen.get_property("bootargs").and_then(Property::as_str),
            Some(TEST_BOOTARGS)
        );
    }

    #[test]
    fn device_tree_patch_can_remove_stale_initrd_properties() {
        let encoded = tree_with_chosen().encode();
        let patched = patch_device_tree(
            encoded.as_ref(),
            Some((TEST_INITRD_START, TEST_INITRD_END)),
            TEST_BOOTARGS,
        )
        .unwrap();
        let patched = patch_device_tree(&patched, None, TEST_BOOTARGS).unwrap();
        let fdt = Fdt::from_bytes(&patched).unwrap();
        let chosen = fdt.get_by_path_id("/chosen").unwrap();
        let chosen = fdt.node(chosen).unwrap();

        assert!(chosen.get_property("linux,initrd-start").is_none());
        assert!(chosen.get_property("linux,initrd-end").is_none());
    }
}
