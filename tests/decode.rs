//! The decoder must agree with the toolchain that produced the fixture bytes.

#[path = "support/assemble.rs"]
mod assemble;

use riscv_simulator_in_rust::bus::Bus;
use riscv_simulator_in_rust::decode::decode;
use riscv_simulator_in_rust::memory::{DRAM_BASE, Memory};
use riscv_simulator_in_rust::trap::Trap;

#[test]
fn decoder_agrees_with_objdump_on_every_loop_instruction() {
    let program = assemble::assemble("loop.S");
    assert_eq!(
        program.binary.len(),
        program.instructions.len() * 4,
        "binary has bytes beyond the instruction stream; check for a section gap"
    );

    let mut memory = Memory::with_size(program.binary.len());
    memory.load_bytes(DRAM_BASE, &program.binary).unwrap();

    for (index, expected) in program.instructions.iter().enumerate() {
        let address = DRAM_BASE + index as u64 * 4;
        assert_eq!(address, expected.address);
        let word = memory.read_u32(address).unwrap();
        let instruction = decode(word)
            .unwrap_or_else(|trap| panic!("{address:#x}: {word:#010x} failed: {trap:?}"));
        assert_eq!(instruction.mnemonic(), expected.mnemonic, "at {address:#x}");
    }
}

#[test]
fn decoder_sign_extends_immediates_and_rejects_illegal_words() {
    let program = assemble::assemble("edges.S");
    let mut memory = Memory::with_size(program.binary.len());
    memory.load_bytes(DRAM_BASE, &program.binary).unwrap();

    let mut immediates = Vec::new();
    for (index, expected) in program.instructions.iter().enumerate() {
        let address = DRAM_BASE + index as u64 * 4;
        let word = memory.read_u32(address).unwrap();
        let decoded = decode(word);

        if expected.mnemonic == ".word" {
            assert_eq!(decoded, Err(Trap::IllegalInstruction), "at {address:#x}");
            continue;
        }

        let instruction = decoded.unwrap_or_else(|trap| panic!("{address:#x}: {trap:?}"));
        assert_eq!(instruction.mnemonic(), expected.mnemonic, "at {address:#x}");

        match instruction {
            riscv_simulator_in_rust::decode::Instruction::Addi { imm, .. } => {
                immediates.push(imm);
            }
            riscv_simulator_in_rust::decode::Instruction::Beq { offset, .. }
            | riscv_simulator_in_rust::decode::Instruction::Jal { offset, .. } => {
                let target = assemble::branch_target(&expected.operands);
                assert_eq!(
                    address.wrapping_add_signed(offset),
                    target,
                    "at {address:#x}"
                );
            }
            riscv_simulator_in_rust::decode::Instruction::Add { .. } => {}
        }
    }

    assert_eq!(immediates, vec![-1, -2048]);
}

/// Reads a register number from an ABI name or an `x`-name, e.g. `a0` or `x10`.
fn register(token: &str) -> u8 {
    let token = token.trim();
    if let Some(number) = token.strip_prefix('x') {
        return number.parse().expect("register number");
    }
    riscv_simulator_in_rust::cpu::ABI_NAMES
        .iter()
        .position(|name| *name == token)
        .expect("ABI register name") as u8
}

/// Splits `objdump` operand text, e.g. `x5,x6,7`.
fn operands(line: &assemble::Disassembled) -> Vec<&str> {
    line.operands.split(',').map(str::trim).collect()
}

/// Builds memory holding the given fixture's binary.
fn fixture_memory(name: &str) -> (assemble::Program, Memory) {
    let program = assemble::assemble(name);
    let mut memory = Memory::with_size(program.binary.len());
    memory.load_bytes(DRAM_BASE, &program.binary).unwrap();
    (program, memory)
}

#[test]
fn decoder_reports_exact_register_fields() {
    use riscv_simulator_in_rust::decode::Instruction;

    let (program, mut memory) = fixture_memory("fields.S");
    let mut checked = 0;

    for (index, line) in program.instructions.iter().enumerate() {
        let address = DRAM_BASE + index as u64 * 4;
        let Ok(instruction) = decode(memory.read_u32(address).unwrap()) else {
            continue; // unsupported encodings are covered by the next test
        };
        let operands = operands(line);

        match instruction {
            Instruction::Addi { rd, rs1, imm } => {
                assert_eq!(line.mnemonic, "addi");
                assert_eq!((rd, rs1), (register(operands[0]), register(operands[1])));
                assert_eq!(imm, operands[2].parse().unwrap());
            }
            Instruction::Add { rd, rs1, rs2 } => {
                assert_eq!(line.mnemonic, "add");
                assert_eq!(
                    (rd, rs1, rs2),
                    (
                        register(operands[0]),
                        register(operands[1]),
                        register(operands[2])
                    )
                );
            }
            Instruction::Beq { rs1, rs2, offset } => {
                assert_eq!(line.mnemonic, "beq");
                assert_eq!((rs1, rs2), (register(operands[0]), register(operands[1])));
                assert_eq!(
                    address.wrapping_add_signed(offset),
                    assemble::branch_target(&line.operands)
                );
            }
            Instruction::Jal { rd, offset } => {
                assert_eq!(line.mnemonic, "jal");
                assert_eq!(rd, register(operands[0]));
                assert_eq!(
                    address.wrapping_add_signed(offset),
                    assemble::branch_target(&line.operands)
                );
            }
        }
        checked += 1;
    }

    assert_eq!(checked, 5, "addi, add, beq and both jumps should decode");
}

#[test]
fn unsupported_encodings_of_known_opcodes_are_illegal() {
    let (program, mut memory) = fixture_memory("fields.S");
    let mut rejected = 0;

    for (index, line) in program.instructions.iter().enumerate() {
        if !matches!(line.mnemonic.as_str(), "sub" | "xor" | "andi" | "bne") {
            continue;
        }
        let address = DRAM_BASE + index as u64 * 4;
        assert_eq!(
            decode(memory.read_u32(address).unwrap()),
            Err(Trap::IllegalInstruction),
            "{} at {address:#x}",
            line.mnemonic
        );
        rejected += 1;
    }

    assert_eq!(rejected, 4, "sub, xor, andi and bne should be rejected");
}
