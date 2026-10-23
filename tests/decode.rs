//! The decoder must agree with the toolchain that produced the fixture bytes.

#[path = "support/assemble.rs"]
mod assemble;

use riscv_simulator_in_rust::bus::Bus;
use riscv_simulator_in_rust::decode::decode;
use riscv_simulator_in_rust::memory::{DRAM_BASE, Memory};
use riscv_simulator_in_rust::trap::Trap;

/// Fixtures whose instruction stream the decoder is checked against.
const FIXTURES: [&str; 13] = [
    "loop.S", "edges.S", "fields.S", "alu.S", "upper.S", "branch.S", "memory.S", "fence.S",
    "trap.S", "mul.S", "div.S", "lrsc.S", "amo.S",
];

/// Zca fixtures assembled with compressed instructions enabled.
const COMPRESSED_FIXTURES: [&str; 2] = ["compressed.S", "compressed_boundaries.S"];

/// Legal mnemonics that this milestone does not decode yet.
const UNIMPLEMENTED: [&str; 0] = [];

#[test]
fn decoder_mnemonics_match_objdump() {
    for fixture in FIXTURES {
        check_fixture(fixture, assemble::assemble);
    }
    for fixture in COMPRESSED_FIXTURES {
        check_fixture(fixture, assemble::assemble_compressed);
    }
    check_fixture("f.S", assemble::assemble_float);
}

/// Decodes every instruction of a fixture and compares mnemonics with `objdump`.
fn check_fixture(name: &str, assembler: fn(&str) -> assemble::Program) {
    let program = assembler(name);
    let mut memory = Memory::with_size(program.binary.len());
    memory.load_bytes(DRAM_BASE, &program.binary).unwrap();

    for line in &program.instructions {
        let address = line.address;
        let word = assemble::instruction_word(&mut memory, address);
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
            canonical_mnemonic(&line.mnemonic),
            "{name} at {address:#x}"
        );
    }
}

/// Maps a compressed mnemonic to the base instruction it expands into.
fn canonical_mnemonic(mnemonic: &str) -> &str {
    match mnemonic {
        "c.addi" | "c.addi4spn" | "c.addi16sp" | "c.li" | "c.nop" => "addi",
        "c.lui" => "lui",
        "c.addiw" => "addiw",
        "c.andi" => "andi",
        "c.slli" => "slli",
        "c.srli" => "srli",
        "c.srai" => "srai",
        "c.sub" => "sub",
        "c.xor" => "xor",
        "c.or" => "or",
        "c.and" => "and",
        "c.subw" => "subw",
        "c.addw" => "addw",
        "c.lw" | "c.lwsp" => "lw",
        "c.ld" | "c.ldsp" => "ld",
        "c.sw" | "c.swsp" => "sw",
        "c.sd" | "c.sdsp" => "sd",
        "c.mv" | "c.add" => "add",
        "c.j" => "jal",
        "c.jr" | "c.jalr" => "jalr",
        "c.beqz" => "beq",
        "c.bnez" => "bne",
        "c.ebreak" => "ebreak",
        other => other.strip_prefix("c.").unwrap_or(other),
    }
}
