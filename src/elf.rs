//! Static RV64 ELF executable loading.

use std::fmt;

use object::Endianness;
use object::elf as obj_elf;
use object::read::elf::{ElfFile64, FileHeader as _, ProgramHeader as _};

use crate::bus::{Bus, MemoryType};
use crate::memory::DramLayout;
use crate::trap::Trap;

const ELF_MAGIC: [u8; 4] = *b"\x7fELF";
const ZERO_CHUNK_BYTES: usize = 4 << 10;
const ZERO_CHUNK: [u8; ZERO_CHUNK_BYTES] = [0; ZERO_CHUNK_BYTES];

/// Whether `image` starts with the ELF magic bytes.
pub fn is_elf(image: &[u8]) -> bool {
    image.starts_with(&ELF_MAGIC)
}

/// A validated static ELF64 executable ready to load.
#[derive(Debug)]
pub struct ElfImage<'data> {
    entry: u64,
    segments: Vec<ElfSegment<'data>>,
}

#[derive(Debug, Clone, Copy)]
struct ElfSegment<'data> {
    address: u64,
    file_data: &'data [u8],
    memory_size: u64,
    flags: obj_elf::ProgramFlags,
}

impl<'data> ElfImage<'data> {
    /// Parses and validates a static RV64 little-endian executable.
    pub fn parse(image: &'data [u8], dram: DramLayout) -> Result<Self, ElfError> {
        Self::parse_with_bias(image, dram, None)
    }

    /// Parses static executable or relocatable PIE firmware.
    ///
    /// OpenSBI firmware is linked as an `ET_DYN` PIE and relocates itself before
    /// installing its trap handlers. It still must not depend on an interpreter
    /// or dynamic linker. A `PT_DYNAMIC` segment is permitted because static PIE
    /// firmware uses it for self-relocation metadata.
    pub fn parse_firmware(
        image: &'data [u8],
        dram: DramLayout,
        load_address: u64,
    ) -> Result<Self, ElfError> {
        Self::parse_with_bias(image, dram, Some(load_address))
    }

    fn parse_with_bias(
        image: &'data [u8],
        dram: DramLayout,
        firmware_load_address: Option<u64>,
    ) -> Result<Self, ElfError> {
        if !is_elf(image) {
            return Err(ElfError::NotElf);
        }
        let file = ElfFile64::<Endianness>::parse(image).map_err(ElfError::Parse)?;
        let endian = file.endian();
        let header = file.elf_header();
        let ident = header.e_ident();
        if ident.class != obj_elf::ELFCLASS64 || ident.data != obj_elf::ELFDATA2LSB {
            return Err(ElfError::UnsupportedFormat);
        }
        let file_type = header.e_type(endian);
        let file_type_supported = file_type == obj_elf::ET_EXEC
            || firmware_load_address.is_some() && file_type == obj_elf::ET_DYN;
        if !file_type_supported {
            return Err(ElfError::UnsupportedType);
        }
        if header.e_machine(endian) != obj_elf::EM_RISCV {
            return Err(ElfError::UnsupportedMachine);
        }

        let mut segments = Vec::new();
        for program in file.elf_program_headers() {
            match program.p_type(endian) {
                obj_elf::PT_INTERP => {
                    return Err(ElfError::DynamicExecutable);
                }
                obj_elf::PT_DYNAMIC if firmware_load_address.is_none() => {
                    return Err(ElfError::DynamicExecutable);
                }
                obj_elf::PT_LOAD => {
                    let virtual_address = program.p_vaddr(endian);
                    let physical_address = program.p_paddr(endian);
                    if virtual_address != physical_address {
                        return Err(ElfError::AddressMismatch);
                    }
                    let image_bias = if file_type == obj_elf::ET_DYN {
                        firmware_load_address.unwrap_or(0)
                    } else {
                        0
                    };
                    let loaded_address = virtual_address
                        .checked_add(image_bias)
                        .ok_or(ElfError::AddressOverflow)?;
                    let file_size = program.p_filesz(endian);
                    let memory_size = program.p_memsz(endian);
                    if file_size > memory_size {
                        return Err(ElfError::InvalidSegmentSize);
                    }
                    let end = loaded_address
                        .checked_add(memory_size)
                        .ok_or(ElfError::AddressOverflow)?;
                    if loaded_address < dram.base() || end > dram.end() {
                        return Err(ElfError::OutOfDram {
                            start: loaded_address,
                            end,
                            dram_start: dram.base(),
                            dram_end: dram.end(),
                        });
                    }
                    let file_data = program
                        .data(endian, image)
                        .map_err(|_| ElfError::InvalidSegmentData)?;
                    segments.push(ElfSegment {
                        address: loaded_address,
                        file_data,
                        memory_size,
                        flags: program.p_flags(endian),
                    });
                }
                _ => {}
            }
        }
        if segments.is_empty() {
            return Err(ElfError::NoLoadableSegments);
        }
        for (index, segment) in segments.iter().enumerate() {
            let end = segment.address + segment.memory_size;
            for other in &segments[index + 1..] {
                let other_end = other.address + other.memory_size;
                if segment.memory_size != 0
                    && other.memory_size != 0
                    && segment.address < other_end
                    && other.address < end
                {
                    return Err(ElfError::OverlappingSegments);
                }
            }
        }

        let entry_bias = if file_type == obj_elf::ET_DYN {
            firmware_load_address.unwrap_or(0)
        } else {
            0
        };
        let entry = header
            .e_entry(endian)
            .checked_add(entry_bias)
            .ok_or(ElfError::AddressOverflow)?;
        let entry_is_executable = segments.iter().any(|segment| {
            entry >= segment.address
                && entry - segment.address < segment.file_data.len() as u64
                && segment.flags.contains(obj_elf::PF_X)
        });
        if !entry_is_executable {
            return Err(ElfError::EntryNotExecutable);
        }

        Ok(Self { entry, segments })
    }

    /// Returns the architectural entry point.
    pub const fn entry(&self) -> u64 {
        self.entry
    }

    /// Returns every loadable segment's half-open memory range.
    pub fn segment_ranges(&self) -> impl Iterator<Item = (u64, u64)> + '_ {
        self.segments
            .iter()
            .map(|segment| (segment.address, segment.address + segment.memory_size))
    }

    /// Loads every validated segment and zeros its BSS tail.
    pub fn load<B: Bus>(&self, bus: &mut B) -> Result<(), ElfError> {
        for segment in &self.segments {
            bus.check_write(segment.address, segment.memory_size, MemoryType::Cacheable)
                .map_err(ElfError::Access)?;
        }
        for segment in &self.segments {
            bus.load_bytes(segment.address, segment.file_data, MemoryType::Cacheable)
                .map_err(ElfError::Access)?;
            let file_end = segment.address + segment.file_data.len() as u64;
            let memory_end = segment.address + segment.memory_size;
            let mut address = file_end;
            while address < memory_end {
                let remaining = memory_end - address;
                let chunk_bytes = remaining.min(ZERO_CHUNK_BYTES as u64) as usize;
                bus.load_bytes(address, &ZERO_CHUNK[..chunk_bytes], MemoryType::Cacheable)
                    .map_err(ElfError::Access)?;
                address += chunk_bytes as u64;
            }
        }
        Ok(())
    }
}

/// Why an ELF executable could not be parsed or loaded.
#[derive(Debug)]
pub enum ElfError {
    /// The input does not begin with the ELF magic bytes.
    NotElf,
    /// The ELF structure is malformed.
    Parse(object::read::Error),
    /// The executable is not 64-bit little-endian.
    UnsupportedFormat,
    /// The image is not a supported executable type.
    UnsupportedType,
    /// The executable is not for RISC-V.
    UnsupportedMachine,
    /// The executable requires a dynamic program interpreter.
    DynamicExecutable,
    /// A segment has different physical and virtual addresses.
    AddressMismatch,
    /// A segment's file size exceeds its memory size.
    InvalidSegmentSize,
    /// A segment's file data is outside the ELF image.
    InvalidSegmentData,
    /// A segment exceeds the host address space.
    AddressOverflow,
    /// A segment lies outside the configured DRAM range.
    OutOfDram {
        start: u64,
        end: u64,
        dram_start: u64,
        dram_end: u64,
    },
    /// No `PT_LOAD` segments were found.
    NoLoadableSegments,
    /// Two loadable segments overlap.
    OverlappingSegments,
    /// The ELF entry point is not in an executable segment.
    EntryNotExecutable,
    /// The destination bus rejected an access.
    Access(Trap),
}

impl fmt::Display for ElfError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotElf => formatter.write_str("input does not have an ELF header"),
            Self::Parse(error) => write!(formatter, "invalid ELF file: {error}"),
            Self::UnsupportedFormat => formatter.write_str("ELF must be 64-bit and little-endian"),
            Self::UnsupportedType => {
                formatter.write_str("ELF must be a static ET_EXEC or firmware PIE")
            }
            Self::UnsupportedMachine => formatter.write_str("ELF must target RISC-V"),
            Self::DynamicExecutable => {
                formatter.write_str("dynamically linked ELF executables are not supported")
            }
            Self::AddressMismatch => {
                formatter.write_str("ELF segment physical and virtual addresses must match")
            }
            Self::InvalidSegmentSize => {
                formatter.write_str("ELF segment file size exceeds its memory size")
            }
            Self::InvalidSegmentData => {
                formatter.write_str("ELF segment data lies outside the image")
            }
            Self::AddressOverflow => formatter.write_str("ELF segment address range overflows"),
            Self::OutOfDram {
                start,
                end,
                dram_start,
                dram_end,
            } => write!(
                formatter,
                "ELF segment {start:#x}..{end:#x} lies outside DRAM \
                 {dram_start:#x}..{dram_end:#x}"
            ),
            Self::NoLoadableSegments => formatter.write_str("ELF has no loadable segments"),
            Self::OverlappingSegments => formatter.write_str("ELF loadable segments overlap"),
            Self::EntryNotExecutable => {
                formatter.write_str("ELF entry point is not in an executable segment")
            }
            Self::Access(trap) => write!(formatter, "cannot load ELF segment: {trap:?}"),
        }
    }
}

impl std::error::Error for ElfError {}
