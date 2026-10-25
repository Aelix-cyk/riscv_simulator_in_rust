//! The trace renderer must spell instructions the way the toolchain does.

#[path = "support/assemble.rs"]
mod assemble;

use assemble::{Disassembled, branch_target};
use riscv_simulator_in_rust::bus::Bus;
use riscv_simulator_in_rust::decode::{Instruction, decode, render};
use riscv_simulator_in_rust::memory::{DRAM_BASE, Memory};

/// Fixtures whose instructions the renderer is checked against.
const FIXTURES: [&str; 13] = [
    "loop.S", "edges.S", "fields.S", "alu.S", "upper.S", "branch.S", "memory.S", "fence.S",
    "trap.S", "mul.S", "div.S", "lrsc.S", "amo.S",
];

/// Zca fixtures assembled with compressed instructions enabled.
const COMPRESSED_FIXTURES: [&str; 2] = ["compressed.S", "compressed_boundaries.S"];

/// RV64B fixtures.
const BITMANIP_FIXTURES: [&str; 2] = ["b.S", "b_bits.S"];

/// Legal mnemonics that this milestone does not decode yet.
const UNIMPLEMENTED: [&str; 0] = [];

#[test]
fn renderer_matches_objdump_text() {
    for fixture in FIXTURES {
        check_fixture(fixture, assemble::assemble);
    }
    for fixture in COMPRESSED_FIXTURES {
        check_fixture(fixture, assemble::assemble_compressed);
    }
    check_fixture("f.S", assemble::assemble_float);
    check_fixture("d.S", assemble::assemble_double);
    check_fixture("compressed_d.S", assemble::assemble_double);
    for fixture in BITMANIP_FIXTURES {
        check_fixture(fixture, assemble::assemble_bitmanip);
    }
}

/// Renders every instruction of a fixture and compares it with `objdump` text.
fn check_fixture(name: &str, assembler: fn(&str) -> assemble::Program) {
    let program = assembler(name);
    let mut memory = Memory::with_size(program.binary.len());
    memory.load_bytes(DRAM_BASE, &program.binary).unwrap();

    for line in &program.instructions {
        let address = line.address;
        let word = assemble::instruction_word(&mut memory, address);
        let Ok(instruction) = decode(word) else {
            let expected_to_fail =
                line.mnemonic == ".word" || UNIMPLEMENTED.contains(&line.mnemonic.as_str());
            assert!(
                expected_to_fail,
                "{name}: {} at {address:#x}",
                line.mnemonic
            );
            continue;
        };

        assert_eq!(
            render(word).unwrap(),
            expected(line),
            "{name} at {address:#x}"
        );

        // Only the PC-relative jumps and branches carry a `pc` target; `jalr`
        // takes a register-based one.
        let pc_relative = matches!(
            instruction,
            Instruction::Beq { .. }
                | Instruction::Bne { .. }
                | Instruction::Blt { .. }
                | Instruction::Bge { .. }
                | Instruction::Bltu { .. }
                | Instruction::Bgeu { .. }
                | Instruction::Jal { .. }
        );
        // `auipc t1,0x1` also contains "pc", so look for the target form.
        let text = render(word).unwrap();
        let carries_target = text.contains("pc + ") || text.contains("pc - ");
        assert_eq!(
            pc_relative, carries_target,
            "{name} at {address:#x}: {text}"
        );
    }
}

/// Rewrites an `objdump` line with a PC-relative target, which is how the trace
/// prints control transfers.
fn expected(line: &Disassembled) -> String {
    let operands: Vec<&str> = line.operands.split(',').map(str::trim).collect();
    match line.mnemonic.as_str() {
        "beq" | "bne" | "blt" | "bge" | "bltu" | "bgeu" => format!(
            "{} {},{},{}",
            line.mnemonic,
            operands[0],
            operands[1],
            relative(line)
        ),
        "c.beqz" | "c.bnez" => {
            format!("{} {},{}", line.mnemonic, operands[0], relative(line))
        }
        "jal" => format!("jal {},{}", operands[0], relative(line)),
        "c.j" => format!("c.j {}", relative(line)),
        mnemonic if line.operands.is_empty() => mnemonic.to_owned(),
        mnemonic => format!("{} {}", mnemonic, line.operands),
    }
}

/// The `pc + 0x8` / `pc - 0xc` form of a line's target.
fn relative(line: &Disassembled) -> String {
    let offset = branch_target(&line.operands) as i64 - line.address as i64;
    if offset < 0 {
        format!("pc - {:#x}", offset.unsigned_abs())
    } else {
        format!("pc + {offset:#x}")
    }
}
