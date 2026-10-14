//! The trace renderer must spell instructions the way the toolchain does.

#[path = "support/assemble.rs"]
mod assemble;

use assemble::{Disassembled, branch_target};
use riscv_simulator_in_rust::bus::Bus;
use riscv_simulator_in_rust::decode::{Instruction, decode};
use riscv_simulator_in_rust::memory::{DRAM_BASE, Memory};

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

#[test]
fn renderer_matches_objdump_text() {
    let program = assemble::assemble("loop.S");
    let mut memory = Memory::with_size(program.binary.len());
    memory.load_bytes(DRAM_BASE, &program.binary).unwrap();

    for (index, line) in program.instructions.iter().enumerate() {
        let address = DRAM_BASE + index as u64 * 4;
        let word = memory.read_u32(address).unwrap();
        let instruction = decode(word).unwrap_or_else(|trap| panic!("{address:#x}: {trap:?}"));

        assert_eq!(instruction.render(), expected(line), "at {address:#x}");
    }
}

#[test]
fn renderer_covers_every_instruction_it_decodes() {
    let program = assemble::assemble("fields.S");
    let mut memory = Memory::with_size(program.binary.len());
    memory.load_bytes(DRAM_BASE, &program.binary).unwrap();
    let mut rendered = 0;

    for (index, _) in program.instructions.iter().enumerate() {
        let address = DRAM_BASE + index as u64 * 4;
        let Ok(instruction) = decode(memory.read_u32(address).unwrap()) else {
            continue;
        };
        let text = instruction.render();
        let control_transfer = matches!(
            instruction,
            Instruction::Beq { .. } | Instruction::Jal { .. }
        );
        assert_eq!(
            control_transfer,
            text.contains("pc "),
            "at {address:#x}: {text}"
        );
        rendered += 1;
    }

    assert_eq!(rendered, 5, "fields.S decodes addi, add, beq and two jumps");
}
