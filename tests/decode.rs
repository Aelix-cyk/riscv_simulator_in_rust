//! The decoder must agree with the toolchain that produced the fixture bytes.

#[path = "support/assemble.rs"]
mod assemble;

use riscv_simulator_in_rust::bus::Bus;
use riscv_simulator_in_rust::decode::decode;
use riscv_simulator_in_rust::memory::{DRAM_BASE, Memory};
use riscv_simulator_in_rust::trap::Trap;

/// Fixtures whose instruction stream the decoder is checked against.
const FIXTURES: [&str; 8] = [
    "loop.S", "edges.S", "fields.S", "alu.S", "upper.S", "branch.S", "memory.S", "fence.S",
];

/// Legal mnemonics that this milestone does not decode yet.
const UNIMPLEMENTED: [&str; 1] = ["mul"];

#[test]
fn decoder_mnemonics_match_objdump() {
    for fixture in FIXTURES {
        check_fixture(fixture);
    }
}

/// Decodes every instruction of a fixture and compares mnemonics with `objdump`.
fn check_fixture(name: &str) {
    let program = assemble::assemble(name);
    // The image holds .text plus any .data; it must at least cover the
    // instruction stream the disassembly lists.
    assert!(
        program.binary.len() >= program.instructions.len() * 4,
        "{name}: the image is smaller than its instruction stream"
    );

    let mut memory = Memory::with_size(program.binary.len());
    memory.load_bytes(DRAM_BASE, &program.binary).unwrap();

    for (index, line) in program.instructions.iter().enumerate() {
        let address = DRAM_BASE + index as u64 * 4;
        assert_eq!(
            address, line.address,
            "{name}: instructions are not contiguous"
        );
        let word = memory.read_u32(address).unwrap();
        let decoded = decode(word);

        if line.mnemonic == ".word" || UNIMPLEMENTED.contains(&line.mnemonic.as_str()) {
            assert_eq!(
                decoded,
                Err(Trap::IllegalInstruction),
                "{name}: {} at {address:#x} should not decode yet",
                line.mnemonic
            );
            continue;
        }

        let instruction =
            decoded.unwrap_or_else(|trap| panic!("{name}: {address:#x} {word:#010x}: {trap:?}"));
        assert_eq!(
            instruction.mnemonic(),
            line.mnemonic,
            "{name} at {address:#x}"
        );
    }
}
