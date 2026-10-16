//! The trace renderer must spell instructions the way the toolchain does.

#[path = "support/assemble.rs"]
mod assemble;

use assemble::{Disassembled, branch_target};
use riscv_simulator_in_rust::bus::Bus;
use riscv_simulator_in_rust::decode::{Instruction, decode};
use riscv_simulator_in_rust::memory::{DRAM_BASE, Memory};

/// Fixtures whose instructions the renderer is checked against.
const FIXTURES: [&str; 6] = [
    "loop.S", "edges.S", "fields.S", "alu.S", "upper.S", "branch.S",
];

/// Legal mnemonics that this milestone does not decode yet.
const UNIMPLEMENTED: [&str; 2] = ["lw", "mul"];

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
        let text = instruction.render();
        let carries_target = text.contains(",pc + ") || text.contains(",pc - ");
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
