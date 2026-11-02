//! OpenSBI firmware boot layout and image loading.

use std::fmt;

use crate::bus::{Bus, MemoryType};
use crate::elf::{ElfError, ElfImage};
use crate::linux::{DEFAULT_BOOTARGS, LinuxImage, LinuxImageError, patch_device_tree};
use crate::memory::{DRAM_BASE, DRAM_PAGE_BYTES, DramLayout};
use crate::trap::Trap;

/// Hart identifier passed to the first booting stage.
pub const BOOT_HART_ID: u64 = 0;

/// QEMU-virt address where OpenSBI enters the next booting stage.
pub const NEXT_STAGE_ADDRESS: u64 = DRAM_BASE + 0x20_0000;

/// QEMU-virt address where OpenSBI expects the flattened device tree.
pub const DEVICE_TREE_ADDRESS: u64 = DRAM_BASE + 0x220_0000;

/// Upper bound reserved for the device tree after [`DEVICE_TREE_ADDRESS`].
const DEVICE_TREE_MAX_BYTES: u64 = 0x20_0000;

/// Space reserved after the effective Linux Image size for early allocation.
const KERNEL_GROWTH_GAP_BYTES: u64 = 16 << 20;

/// Entry state established by the firmware boot loader.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BootInfo {
    /// Firmware entry point.
    pub entry: u64,
    /// Hart identifier passed to firmware.
    pub hart_id: u64,
    /// Physical address of the flattened device tree.
    pub device_tree_address: u64,
}

/// Validates and loads a QEMU-virt-compatible OpenSBI boot.
pub fn load<B: Bus>(
    dram: DramLayout,
    firmware: &ElfImage<'_>,
    device_tree: &[u8],
    kernel: &ElfImage<'_>,
    bus: &mut B,
) -> Result<BootInfo, BootError> {
    validate_firmware(dram, firmware)?;
    if kernel.entry() != NEXT_STAGE_ADDRESS {
        return Err(BootError::KernelEntry {
            address: kernel.entry(),
        });
    }
    for (start, end) in kernel.segment_ranges() {
        if start < NEXT_STAGE_ADDRESS || end > DEVICE_TREE_ADDRESS {
            return Err(BootError::KernelOutsideRegion { start, end });
        }
    }
    let device_tree_size = device_tree.len() as u64;
    if device_tree_size > DEVICE_TREE_MAX_BYTES {
        return Err(BootError::DeviceTreeTooLarge {
            bytes: device_tree_size,
        });
    }

    firmware.load(bus).map_err(BootError::Firmware)?;
    kernel.load(bus).map_err(BootError::Kernel)?;
    bus.load_bytes(DEVICE_TREE_ADDRESS, device_tree, MemoryType::Cacheable)
        .map_err(BootError::DeviceTree)?;

    Ok(BootInfo {
        entry: firmware.entry(),
        hart_id: BOOT_HART_ID,
        device_tree_address: DEVICE_TREE_ADDRESS,
    })
}

/// Validates and loads OpenSBI firmware followed by a raw Linux Image.
pub fn load_linux<B: Bus>(
    dram: DramLayout,
    firmware: &ElfImage<'_>,
    device_tree: &[u8],
    kernel: &LinuxImage<'_>,
    initrd: Option<&[u8]>,
    bootargs: Option<&str>,
    bus: &mut B,
) -> Result<BootInfo, BootError> {
    validate_firmware(dram, firmware)?;
    if kernel.entry() != NEXT_STAGE_ADDRESS {
        return Err(BootError::KernelEntry {
            address: kernel.entry(),
        });
    }
    validate_device_tree_size(device_tree.len() as u64)?;

    let (_, kernel_end) = kernel.range();
    let initrd_len = initrd.map_or(0, |bytes| bytes.len() as u64);
    let initrd_start = linux_initrd_start(kernel_end)?;
    let initrd_end = initrd_start
        .checked_add(initrd_len)
        .ok_or(BootError::BootRegionOverflow)?;
    let initrd_range = (initrd_len != 0).then_some((initrd_start, initrd_end));
    let patched_tree = patch_device_tree(
        device_tree,
        initrd_range,
        bootargs.unwrap_or(DEFAULT_BOOTARGS),
    )
    .map_err(BootError::DeviceTreePatch)?;
    validate_device_tree_size(patched_tree.len() as u64)?;
    let layout = linux_layout(dram, kernel_end, initrd_len, patched_tree.len() as u64)?;
    debug_assert_eq!(layout.device_tree_bytes, patched_tree.len() as u64);

    firmware.load(bus).map_err(BootError::Firmware)?;
    kernel.load(bus).map_err(BootError::KernelImage)?;
    if let (Some(initrd), Some((start, _))) = (initrd, layout.initrd) {
        bus.check_write(start, initrd.len() as u64, MemoryType::Cacheable)
            .map_err(BootError::Initrd)?;
        bus.load_bytes(start, initrd, MemoryType::Cacheable)
            .map_err(BootError::Initrd)?;
    }
    bus.check_write(
        layout.device_tree_address,
        patched_tree.len() as u64,
        MemoryType::Cacheable,
    )
    .map_err(BootError::DeviceTree)?;
    bus.load_bytes(
        layout.device_tree_address,
        &patched_tree,
        MemoryType::Cacheable,
    )
    .map_err(BootError::DeviceTree)?;

    Ok(BootInfo {
        entry: firmware.entry(),
        hart_id: BOOT_HART_ID,
        device_tree_address: layout.device_tree_address,
    })
}

fn validate_firmware(dram: DramLayout, firmware: &ElfImage<'_>) -> Result<(), BootError> {
    if dram != DramLayout::default() {
        return Err(BootError::UnsupportedDram);
    }
    for (start, end) in firmware.segment_ranges() {
        if end > NEXT_STAGE_ADDRESS {
            return Err(BootError::FirmwareOutsideRegion { start, end });
        }
    }
    Ok(())
}

fn validate_device_tree_size(bytes: u64) -> Result<(), BootError> {
    if bytes > DEVICE_TREE_MAX_BYTES {
        return Err(BootError::DeviceTreeTooLarge { bytes });
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct LinuxLayout {
    initrd: Option<(u64, u64)>,
    device_tree_address: u64,
    device_tree_bytes: u64,
}

fn linux_layout(
    dram: DramLayout,
    kernel_end: u64,
    initrd_bytes: u64,
    device_tree_bytes: u64,
) -> Result<LinuxLayout, BootError> {
    let initrd_start = linux_initrd_start(kernel_end)?;
    let initrd_end = initrd_start
        .checked_add(initrd_bytes)
        .ok_or(BootError::BootRegionOverflow)?;
    let initrd = (initrd_bytes != 0).then_some((initrd_start, initrd_end));
    let device_tree_address = align_up(initrd_end, DRAM_PAGE_BYTES)?;
    let device_tree_end = device_tree_address
        .checked_add(device_tree_bytes)
        .ok_or(BootError::BootRegionOverflow)?;

    for (name, start, end) in [
        ("initramfs", initrd_start, initrd_end),
        ("device tree", device_tree_address, device_tree_end),
    ] {
        if end > dram.end() {
            return Err(BootError::BootImageOutsideDram {
                name,
                start,
                end,
                dram_end: dram.end(),
            });
        }
    }

    Ok(LinuxLayout {
        initrd,
        device_tree_address,
        device_tree_bytes,
    })
}

fn linux_initrd_start(kernel_end: u64) -> Result<u64, BootError> {
    let after_kernel_gap = kernel_end
        .checked_add(KERNEL_GROWTH_GAP_BYTES)
        .ok_or(BootError::BootRegionOverflow)?;
    align_up(after_kernel_gap, DRAM_PAGE_BYTES)
}

fn align_up(value: u64, alignment: u64) -> Result<u64, BootError> {
    debug_assert!(alignment.is_power_of_two());
    value
        .checked_add(alignment - 1)
        .map(|rounded| rounded & !(alignment - 1))
        .ok_or(BootError::BootRegionOverflow)
}

/// Why a QEMU-virt-compatible boot could not be prepared.
#[derive(Debug)]
pub enum BootError {
    /// OpenSBI boot currently requires the default DRAM layout.
    UnsupportedDram,
    /// A firmware segment crosses into the next-stage region.
    FirmwareOutsideRegion { start: u64, end: u64 },
    /// The kernel entry is not at the QEMU-virt next-stage address.
    KernelEntry { address: u64 },
    /// A kernel segment crosses a reserved boot region.
    KernelOutsideRegion { start: u64, end: u64 },
    /// The device-tree image does not fit in its reserved region.
    DeviceTreeTooLarge { bytes: u64 },
    /// The firmware ELF could not be loaded.
    Firmware(ElfError),
    /// The kernel ELF could not be loaded.
    Kernel(ElfError),
    /// The raw Linux Image header or contents are invalid.
    KernelImage(LinuxImageError),
    /// The initramfs could not be loaded.
    Initrd(Trap),
    /// Linux boot information could not be patched into the device tree.
    DeviceTreePatch(LinuxImageError),
    /// A boot component does not fit in the configured DRAM.
    BootImageOutsideDram {
        name: &'static str,
        start: u64,
        end: u64,
        dram_end: u64,
    },
    /// A computed boot address overflowed.
    BootRegionOverflow,
    /// The device tree could not be loaded.
    DeviceTree(Trap),
}

impl fmt::Display for BootError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedDram => formatter
                .write_str("OpenSBI boot requires the default 128 MiB DRAM layout at 0x80000000"),
            Self::FirmwareOutsideRegion { start, end } => write!(
                formatter,
                "OpenSBI firmware segment {start:#x}..{end:#x} crosses {NEXT_STAGE_ADDRESS:#x}"
            ),
            Self::KernelEntry { address } => write!(
                formatter,
                "OpenSBI kernel entry must be {NEXT_STAGE_ADDRESS:#x}, not {address:#x}"
            ),
            Self::KernelOutsideRegion { start, end } => write!(
                formatter,
                "OpenSBI kernel segment {start:#x}..{end:#x} must fit between \
                 {NEXT_STAGE_ADDRESS:#x} and {DEVICE_TREE_ADDRESS:#x}"
            ),
            Self::DeviceTreeTooLarge { bytes } => write!(
                formatter,
                "device tree is {bytes} bytes; the boot region holds at most \
                 {DEVICE_TREE_MAX_BYTES} bytes"
            ),
            Self::Firmware(error) => write!(formatter, "cannot load OpenSBI firmware: {error}"),
            Self::Kernel(error) => write!(formatter, "cannot load OpenSBI kernel: {error}"),
            Self::KernelImage(error) => write!(formatter, "cannot load Linux Image: {error}"),
            Self::Initrd(trap) => write!(formatter, "cannot load initramfs: {trap:?}"),
            Self::DeviceTreePatch(error) => {
                write!(formatter, "cannot prepare the Linux device tree: {error}")
            }
            Self::BootImageOutsideDram {
                name,
                start,
                end,
                dram_end,
            } => write!(
                formatter,
                "{name} range {start:#x}..{end:#x} exceeds DRAM end {dram_end:#x}"
            ),
            Self::BootRegionOverflow => {
                formatter.write_str("a Linux boot address range overflowed")
            }
            Self::DeviceTree(trap) => {
                write!(
                    formatter,
                    "cannot load device tree at {DEVICE_TREE_ADDRESS:#x}: {trap:?}"
                )
            }
        }
    }
}

impl std::error::Error for BootError {}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_DRAM_SIZE: u64 = 128 << 20;
    const TEST_KERNEL_END: u64 = NEXT_STAGE_ADDRESS + 0x20_0000;
    const TEST_INITRD_BYTES: u64 = 0x2000;
    const TEST_DTB_BYTES: u64 = 0x1000;
    const NO_BYTES: u64 = 0;

    fn default_layout() -> DramLayout {
        DramLayout::new(DRAM_BASE, TEST_DRAM_SIZE).unwrap()
    }

    #[test]
    fn linux_layout_reserves_growth_space_and_aligns_components() {
        let layout = linux_layout(
            default_layout(),
            TEST_KERNEL_END,
            TEST_INITRD_BYTES,
            TEST_DTB_BYTES,
        )
        .unwrap();
        let (initrd_start, initrd_end) = layout.initrd.unwrap();

        assert!(initrd_start >= TEST_KERNEL_END + KERNEL_GROWTH_GAP_BYTES);
        assert!(initrd_start.is_multiple_of(DRAM_PAGE_BYTES));
        assert_eq!(initrd_end - initrd_start, TEST_INITRD_BYTES);
        assert!(layout.device_tree_address.is_multiple_of(DRAM_PAGE_BYTES));
        assert!(layout.device_tree_address >= initrd_end);
        assert_eq!(layout.device_tree_bytes, TEST_DTB_BYTES);
    }

    #[test]
    fn linux_layout_rejects_images_past_dram_end() {
        let error = linux_layout(
            default_layout(),
            DRAM_BASE + default_layout().size() - 1,
            NO_BYTES,
            TEST_DTB_BYTES,
        )
        .unwrap_err();

        assert!(matches!(error, BootError::BootImageOutsideDram { .. }));
    }
}
