//! OpenSBI firmware boot layout and image loading.

use std::fmt;

use crate::bus::{Bus, MemoryType};
use crate::elf::{ElfError, ElfImage};
use crate::memory::{DRAM_BASE, DramLayout};
use crate::trap::Trap;

/// Hart identifier passed to the first booting stage.
pub const BOOT_HART_ID: u64 = 0;

/// QEMU-virt address where OpenSBI enters the next booting stage.
pub const NEXT_STAGE_ADDRESS: u64 = DRAM_BASE + 0x20_0000;

/// QEMU-virt address where OpenSBI expects the flattened device tree.
pub const DEVICE_TREE_ADDRESS: u64 = DRAM_BASE + 0x220_0000;

/// Upper bound reserved for the device tree after [`DEVICE_TREE_ADDRESS`].
const DEVICE_TREE_MAX_BYTES: u64 = 0x20_0000;

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
    if dram != DramLayout::default() {
        return Err(BootError::UnsupportedDram);
    }
    for (start, end) in firmware.segment_ranges() {
        if end > NEXT_STAGE_ADDRESS {
            return Err(BootError::FirmwareOutsideRegion { start, end });
        }
    }
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
