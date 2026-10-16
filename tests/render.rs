//! The trace renderer must spell instructions the way the toolchain does.

#[path = "support/assemble.rs"]
mod assemble;

use assemble::{Disassembled, branch_target};
use riscv_simulator_in_rust::bus::Bus;
use riscv_simulator_in_rust::decode::{Instruction, decode};
use riscv_simulator_in_rust::memory::{DRAM_BASE, Memory};

/// Fixtures whose instructions the renderer is checked against.
const FIXTURES: [&str; 4] = ["loop.S", "edges.S", "fields.S", "alu.S"];

/// Legal mnemonics that this milestone does not decode yet.
const UNIMPLEMENTED: [&str; 3] = ["bne", "lw", "mul"];

#[test]
fn renderer_matches_objdump_text() {
    for fixture in FIXTURES {
        check_fixture(fixture);
    }
}

/// Renders every instruction of a fixture and compares it with `objdump` text.
fn check_fixture(name: &str) {
    let program = assemble::assemble(name);
    let mut memory = Memory::with_size(program.binary.len());
    memory.load_bytes(DRAM_BASE, &program.binary).unwrap();

    for (index, line) in program.instructions.iter().enumerate() {
        let address = DRAM_BASE + index as u64 * 4;
        let Ok(instruction) = decode(memory.read_u32(address).unwrap()) else {
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
            instruction.render(),
            expected(line),
            "{name} at {address:#x}"
        );

        // Only control transfers carry a PC-relative target.
        let control_transfer = matches!(
            instruction,
            Instruction::Beq { .. } | Instruction::Jal { .. }
        );
        assert_eq!(
            control_transfer,
            instruction.render().contains("pc "),
            "{name} at {address:#x}"
        );
    }
}

/// Rewrites an `objdump` line with a PC-relative target, which is how the trace
/// prints control transfers.
fn expected(line: &Disassembled) -> String {
    let operands: Vec<&str> = line.operands.split(',').map(str::trim).collect();
    match line.mnemonic.as_str() {
        "beq" => format!("beq {},{},{}", operands[0], operands[1], relative(line)),
        "jal" => format!("jal {},{}", operands[0], relative(line)),
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
