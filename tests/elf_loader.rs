use riscv_simulator_in_rust::bus::{Bus, MemoryType};
use riscv_simulator_in_rust::elf::{ElfError, ElfImage};
use riscv_simulator_in_rust::memory::{DRAM_BASE, DramLayout, Memory};

const ELF_HEADER_BYTES: usize = 64;
const PROGRAM_HEADER_BYTES: usize = 56;
const FIRST_SEGMENT_OFFSET: usize = 0x1000;
const SECOND_SEGMENT_OFFSET: usize = 0x2000;
const TEST_FILE_BYTES: usize = 0x3004;
const TEST_DRAM_BYTES: u64 = 0x3000;
const TEST_DATA_BYTES: u64 = 4;
const TEST_MEMORY_BYTES: u64 = 8;
const ELF_64_CLASS: u8 = 2;
const ELF_LITTLE_ENDIAN: u8 = 1;
const ELF_CURRENT_VERSION: u8 = 1;
const ELF_EXECUTABLE: u16 = 2;
const ELF_RISCV_MACHINE: u16 = 243;
const PROGRAM_LOAD: u32 = 1;
const PROGRAM_INTERPRETER: u32 = 3;
const PROGRAM_READ: u32 = 1 << 2;
const PROGRAM_WRITE: u32 = 1 << 1;
const PROGRAM_EXECUTE: u32 = 1 << 0;
const PAGE_ALIGNMENT: u64 = 0x1000;

#[derive(Clone, Copy)]
struct Segment {
    kind: u32,
    flags: u32,
    offset: u64,
    virtual_address: u64,
    physical_address: u64,
    file_size: u64,
    memory_size: u64,
}

impl Segment {
    fn load(offset: usize, address: u64, file_size: u64, memory_size: u64, flags: u32) -> Self {
        Self {
            kind: PROGRAM_LOAD,
            flags,
            offset: offset as u64,
            virtual_address: address,
            physical_address: address,
            file_size,
            memory_size,
        }
    }
}

fn write_u16(bytes: &mut [u8], offset: usize, value: u16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn write_u32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn write_u64(bytes: &mut [u8], offset: usize, value: u64) {
    bytes[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

fn elf_with_segments(entry: u64, segments: &[Segment]) -> Vec<u8> {
    let mut bytes = vec![0; TEST_FILE_BYTES];
    bytes[..4].copy_from_slice(b"\x7fELF");
    bytes[4] = ELF_64_CLASS;
    bytes[5] = ELF_LITTLE_ENDIAN;
    bytes[6] = ELF_CURRENT_VERSION;
    write_u16(&mut bytes, 16, ELF_EXECUTABLE);
    write_u16(&mut bytes, 18, ELF_RISCV_MACHINE);
    write_u32(&mut bytes, 20, u32::from(ELF_CURRENT_VERSION));
    write_u64(&mut bytes, 24, entry);
    write_u64(&mut bytes, 32, ELF_HEADER_BYTES as u64);
    write_u16(&mut bytes, 52, ELF_HEADER_BYTES as u16);
    write_u16(&mut bytes, 54, PROGRAM_HEADER_BYTES as u16);
    write_u16(&mut bytes, 56, segments.len() as u16);

    for (index, segment) in segments.iter().enumerate() {
        let offset = ELF_HEADER_BYTES + index * PROGRAM_HEADER_BYTES;
        write_u32(&mut bytes, offset, segment.kind);
        write_u32(&mut bytes, offset + 4, segment.flags);
        write_u64(&mut bytes, offset + 8, segment.offset);
        write_u64(&mut bytes, offset + 16, segment.virtual_address);
        write_u64(&mut bytes, offset + 24, segment.physical_address);
        write_u64(&mut bytes, offset + 32, segment.file_size);
        write_u64(&mut bytes, offset + 40, segment.memory_size);
        write_u64(&mut bytes, offset + 48, PAGE_ALIGNMENT);
    }
    bytes
}

fn valid_elf() -> Vec<u8> {
    let mut bytes = elf_with_segments(
        DRAM_BASE,
        &[Segment::load(
            FIRST_SEGMENT_OFFSET,
            DRAM_BASE,
            TEST_DATA_BYTES,
            TEST_MEMORY_BYTES,
            PROGRAM_READ | PROGRAM_EXECUTE,
        )],
    );
    bytes[FIRST_SEGMENT_OFFSET..FIRST_SEGMENT_OFFSET + TEST_DATA_BYTES as usize]
        .copy_from_slice(&[0x13, 0x05, 0x10, 0x00]);
    bytes
}

fn test_layout() -> DramLayout {
    DramLayout::new(DRAM_BASE, TEST_DRAM_BYTES).unwrap()
}

#[test]
fn loads_file_data_and_zeroes_bss() {
    let bytes = valid_elf();
    let image = ElfImage::parse(&bytes, test_layout()).unwrap();
    let mut memory = Memory::with_size(TEST_DRAM_BYTES as usize);
    memory
        .write_u8(DRAM_BASE + TEST_DATA_BYTES, 0xaa, MemoryType::Cacheable)
        .unwrap();

    image.load(&mut memory).unwrap();

    assert_eq!(image.entry(), DRAM_BASE);
    assert_eq!(
        memory.read_u32(DRAM_BASE, MemoryType::Cacheable).unwrap(),
        0x0010_0513
    );
    for address in DRAM_BASE + TEST_DATA_BYTES..DRAM_BASE + TEST_MEMORY_BYTES {
        assert_eq!(memory.read_u8(address, MemoryType::Cacheable).unwrap(), 0);
    }
}

#[test]
fn rejects_wrong_identity_and_entry_permissions() {
    let mut bytes = valid_elf();
    bytes[4] = 1;
    assert!(matches!(
        ElfImage::parse(&bytes, test_layout()),
        Err(ElfError::Parse(_))
    ));

    let bytes = elf_with_segments(
        DRAM_BASE,
        &[Segment::load(
            FIRST_SEGMENT_OFFSET,
            DRAM_BASE,
            TEST_DATA_BYTES,
            TEST_MEMORY_BYTES,
            PROGRAM_READ,
        )],
    );
    assert!(matches!(
        ElfImage::parse(&bytes, test_layout()),
        Err(ElfError::EntryNotExecutable)
    ));

    let bytes = elf_with_segments(
        DRAM_BASE + TEST_DATA_BYTES,
        &[Segment::load(
            FIRST_SEGMENT_OFFSET,
            DRAM_BASE,
            TEST_DATA_BYTES,
            TEST_MEMORY_BYTES,
            PROGRAM_READ | PROGRAM_EXECUTE,
        )],
    );
    assert!(matches!(
        ElfImage::parse(&bytes, test_layout()),
        Err(ElfError::EntryNotExecutable)
    ));
}

#[test]
fn rejects_dynamic_and_mismatched_segments() {
    let dynamic = elf_with_segments(
        DRAM_BASE,
        &[
            Segment::load(
                FIRST_SEGMENT_OFFSET,
                DRAM_BASE,
                TEST_DATA_BYTES,
                TEST_MEMORY_BYTES,
                PROGRAM_READ | PROGRAM_EXECUTE,
            ),
            Segment {
                kind: PROGRAM_INTERPRETER,
                flags: PROGRAM_READ,
                offset: SECOND_SEGMENT_OFFSET as u64,
                virtual_address: 0,
                physical_address: 0,
                file_size: 0,
                memory_size: 0,
            },
        ],
    );
    assert!(matches!(
        ElfImage::parse(&dynamic, test_layout()),
        Err(ElfError::DynamicExecutable)
    ));

    let mut mismatched = Segment::load(
        FIRST_SEGMENT_OFFSET,
        DRAM_BASE,
        TEST_DATA_BYTES,
        TEST_MEMORY_BYTES,
        PROGRAM_READ | PROGRAM_EXECUTE,
    );
    mismatched.physical_address += PAGE_ALIGNMENT;
    let bytes = elf_with_segments(DRAM_BASE, &[mismatched]);
    assert!(matches!(
        ElfImage::parse(&bytes, test_layout()),
        Err(ElfError::AddressMismatch)
    ));
}

#[test]
fn rejects_bad_ranges_overlaps_and_truncated_data() {
    let outside = elf_with_segments(
        DRAM_BASE + TEST_DRAM_BYTES,
        &[Segment::load(
            FIRST_SEGMENT_OFFSET,
            DRAM_BASE + TEST_DRAM_BYTES,
            TEST_DATA_BYTES,
            TEST_MEMORY_BYTES,
            PROGRAM_READ | PROGRAM_EXECUTE,
        )],
    );
    assert!(matches!(
        ElfImage::parse(&outside, test_layout()),
        Err(ElfError::OutOfDram)
    ));

    let overlapping = elf_with_segments(
        DRAM_BASE,
        &[
            Segment::load(
                FIRST_SEGMENT_OFFSET,
                DRAM_BASE,
                TEST_DATA_BYTES,
                TEST_MEMORY_BYTES,
                PROGRAM_READ | PROGRAM_EXECUTE,
            ),
            Segment::load(
                SECOND_SEGMENT_OFFSET,
                DRAM_BASE + TEST_DATA_BYTES,
                TEST_DATA_BYTES,
                TEST_MEMORY_BYTES,
                PROGRAM_READ | PROGRAM_WRITE,
            ),
        ],
    );
    assert!(matches!(
        ElfImage::parse(&overlapping, test_layout()),
        Err(ElfError::OverlappingSegments)
    ));

    let mut truncated = Segment::load(
        FIRST_SEGMENT_OFFSET,
        DRAM_BASE,
        TEST_DATA_BYTES,
        TEST_MEMORY_BYTES,
        PROGRAM_READ | PROGRAM_EXECUTE,
    );
    truncated.file_size = (TEST_FILE_BYTES - FIRST_SEGMENT_OFFSET + 1) as u64;
    truncated.memory_size = truncated.file_size;
    let bytes = elf_with_segments(DRAM_BASE, &[truncated]);
    assert!(matches!(
        ElfImage::parse(&bytes, test_layout()),
        Err(ElfError::InvalidSegmentData)
    ));
}
