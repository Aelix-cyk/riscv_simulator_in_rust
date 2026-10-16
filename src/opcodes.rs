//! The instruction table: one row per implemented instruction.
//!
//! Matching uses the generated `MATCH_*`/`MASK_*` constants in [`crate::inst`],
//! which come from the riscv-opcodes repository. A word is decoded by finding the
//! first row with `word & mask == code`, so row order decides overlapping matches
//! and the tests below guard that ordering.
//!
//! The table is deliberately not indexed by opcode: `benches/decode.rs` measured
//! opcode-first grouping as equal or slightly slower at this size, because the
//! scan exits after a few comparisons and the cost sits elsewhere. Rows are ordered
//! hottest-first instead. Re-measure when the table passes roughly a hundred rows.

use crate::decode::{Instruction, Word};
use crate::inst;

/// One row of the instruction table.
struct Row {
    /// Bits that must be present in the word.
    code: u32,
    /// Bits that the encoding defines; the rest are ignored.
    mask: u32,
    /// Builds the decoded instruction from the word.
    build: fn(Word) -> Instruction,
}

/// Looks up the implemented instruction for `word`, if there is one.
pub(crate) fn lookup(word: Word) -> Option<Instruction> {
    TABLE
        .iter()
        .find(|row| word.raw() & row.mask == row.code)
        .map(|row| (row.build)(word))
}

/// Every implemented instruction, hottest first: `addi` and the other
/// register-immediate operations dominate real code, so they are scanned first.
const TABLE: &[Row] = &[
    // Register-immediate ALU operations.
    row(inst::MATCH_ADDI, inst::MASK_ADDI, addi),
    row(inst::MATCH_SLTI, inst::MASK_SLTI, slti),
    row(inst::MATCH_SLTIU, inst::MASK_SLTIU, sltiu),
    row(inst::MATCH_XORI, inst::MASK_XORI, xori),
    row(inst::MATCH_ORI, inst::MASK_ORI, ori),
    row(inst::MATCH_ANDI, inst::MASK_ANDI, andi),
    row(inst::MATCH_SLLI, inst::MASK_SLLI, slli),
    row(inst::MATCH_SRLI, inst::MASK_SRLI, srli),
    row(inst::MATCH_SRAI, inst::MASK_SRAI, srai),
    // Register-immediate ALU operations on 32-bit values.
    row(inst::MATCH_ADDIW, inst::MASK_ADDIW, addiw),
    row(inst::MATCH_SLLIW, inst::MASK_SLLIW, slliw),
    row(inst::MATCH_SRLIW, inst::MASK_SRLIW, srliw),
    row(inst::MATCH_SRAIW, inst::MASK_SRAIW, sraiw),
    // Register-register ALU operations.
    row(inst::MATCH_ADD, inst::MASK_ADD, add),
    row(inst::MATCH_SUB, inst::MASK_SUB, sub),
    row(inst::MATCH_SLL, inst::MASK_SLL, sll),
    row(inst::MATCH_SLT, inst::MASK_SLT, slt),
    row(inst::MATCH_SLTU, inst::MASK_SLTU, sltu),
    row(inst::MATCH_XOR, inst::MASK_XOR, xor),
    row(inst::MATCH_SRL, inst::MASK_SRL, srl),
    row(inst::MATCH_SRA, inst::MASK_SRA, sra),
    row(inst::MATCH_OR, inst::MASK_OR, or),
    row(inst::MATCH_AND, inst::MASK_AND, and),
    // Register-register ALU operations on 32-bit values.
    row(inst::MATCH_ADDW, inst::MASK_ADDW, addw),
    row(inst::MATCH_SUBW, inst::MASK_SUBW, subw),
    row(inst::MATCH_SLLW, inst::MASK_SLLW, sllw),
    row(inst::MATCH_SRLW, inst::MASK_SRLW, srlw),
    row(inst::MATCH_SRAW, inst::MASK_SRAW, sraw),
    // Control transfers.
    row(inst::MATCH_LUI, inst::MASK_LUI, lui),
    row(inst::MATCH_AUIPC, inst::MASK_AUIPC, auipc),
    row(inst::MATCH_JALR, inst::MASK_JALR, jalr),
    row(inst::MATCH_BEQ, inst::MASK_BEQ, beq),
    row(inst::MATCH_BNE, inst::MASK_BNE, bne),
    row(inst::MATCH_BLT, inst::MASK_BLT, blt),
    row(inst::MATCH_BGE, inst::MASK_BGE, bge),
    row(inst::MATCH_BLTU, inst::MASK_BLTU, bltu),
    row(inst::MATCH_BGEU, inst::MASK_BGEU, bgeu),
    row(inst::MATCH_JAL, inst::MASK_JAL, jal),
    // Loads.
    row(inst::MATCH_LW, inst::MASK_LW, lw),
    row(inst::MATCH_LD, inst::MASK_LD, ld),
    row(inst::MATCH_LB, inst::MASK_LB, lb),
    row(inst::MATCH_LH, inst::MASK_LH, lh),
    row(inst::MATCH_LWU, inst::MASK_LWU, lwu),
    row(inst::MATCH_LBU, inst::MASK_LBU, lbu),
    row(inst::MATCH_LHU, inst::MASK_LHU, lhu),
    // Stores.
    row(inst::MATCH_SW, inst::MASK_SW, sw),
    row(inst::MATCH_SD, inst::MASK_SD, sd),
    row(inst::MATCH_SB, inst::MASK_SB, sb),
    row(inst::MATCH_SH, inst::MASK_SH, sh),
];

/// Builds a table row.
const fn row(code: u32, mask: u32, build: fn(Word) -> Instruction) -> Row {
    Row { code, mask, build }
}

fn addi(word: Word) -> Instruction {
    Instruction::Addi {
        rd: word.rd(),
        rs1: word.rs1(),
        imm: word.i_immediate(),
    }
}

fn slti(word: Word) -> Instruction {
    Instruction::Slti {
        rd: word.rd(),
        rs1: word.rs1(),
        imm: word.i_immediate(),
    }
}

fn sltiu(word: Word) -> Instruction {
    Instruction::Sltiu {
        rd: word.rd(),
        rs1: word.rs1(),
        imm: word.i_immediate(),
    }
}

fn xori(word: Word) -> Instruction {
    Instruction::Xori {
        rd: word.rd(),
        rs1: word.rs1(),
        imm: word.i_immediate(),
    }
}

fn ori(word: Word) -> Instruction {
    Instruction::Ori {
        rd: word.rd(),
        rs1: word.rs1(),
        imm: word.i_immediate(),
    }
}

fn andi(word: Word) -> Instruction {
    Instruction::Andi {
        rd: word.rd(),
        rs1: word.rs1(),
        imm: word.i_immediate(),
    }
}

fn slli(word: Word) -> Instruction {
    Instruction::Slli {
        rd: word.rd(),
        rs1: word.rs1(),
        shamt: word.shamt(),
    }
}

fn srli(word: Word) -> Instruction {
    Instruction::Srli {
        rd: word.rd(),
        rs1: word.rs1(),
        shamt: word.shamt(),
    }
}

fn srai(word: Word) -> Instruction {
    Instruction::Srai {
        rd: word.rd(),
        rs1: word.rs1(),
        shamt: word.shamt(),
    }
}

fn addiw(word: Word) -> Instruction {
    Instruction::Addiw {
        rd: word.rd(),
        rs1: word.rs1(),
        imm: word.i_immediate(),
    }
}

fn slliw(word: Word) -> Instruction {
    Instruction::Slliw {
        rd: word.rd(),
        rs1: word.rs1(),
        shamt: word.shamt_w(),
    }
}

fn srliw(word: Word) -> Instruction {
    Instruction::Srliw {
        rd: word.rd(),
        rs1: word.rs1(),
        shamt: word.shamt_w(),
    }
}

fn sraiw(word: Word) -> Instruction {
    Instruction::Sraiw {
        rd: word.rd(),
        rs1: word.rs1(),
        shamt: word.shamt_w(),
    }
}

fn add(word: Word) -> Instruction {
    Instruction::Add {
        rd: word.rd(),
        rs1: word.rs1(),
        rs2: word.rs2(),
    }
}

fn sub(word: Word) -> Instruction {
    Instruction::Sub {
        rd: word.rd(),
        rs1: word.rs1(),
        rs2: word.rs2(),
    }
}

fn sll(word: Word) -> Instruction {
    Instruction::Sll {
        rd: word.rd(),
        rs1: word.rs1(),
        rs2: word.rs2(),
    }
}

fn slt(word: Word) -> Instruction {
    Instruction::Slt {
        rd: word.rd(),
        rs1: word.rs1(),
        rs2: word.rs2(),
    }
}

fn sltu(word: Word) -> Instruction {
    Instruction::Sltu {
        rd: word.rd(),
        rs1: word.rs1(),
        rs2: word.rs2(),
    }
}

fn xor(word: Word) -> Instruction {
    Instruction::Xor {
        rd: word.rd(),
        rs1: word.rs1(),
        rs2: word.rs2(),
    }
}

fn srl(word: Word) -> Instruction {
    Instruction::Srl {
        rd: word.rd(),
        rs1: word.rs1(),
        rs2: word.rs2(),
    }
}

fn sra(word: Word) -> Instruction {
    Instruction::Sra {
        rd: word.rd(),
        rs1: word.rs1(),
        rs2: word.rs2(),
    }
}

fn or(word: Word) -> Instruction {
    Instruction::Or {
        rd: word.rd(),
        rs1: word.rs1(),
        rs2: word.rs2(),
    }
}

fn and(word: Word) -> Instruction {
    Instruction::And {
        rd: word.rd(),
        rs1: word.rs1(),
        rs2: word.rs2(),
    }
}

fn addw(word: Word) -> Instruction {
    Instruction::Addw {
        rd: word.rd(),
        rs1: word.rs1(),
        rs2: word.rs2(),
    }
}

fn subw(word: Word) -> Instruction {
    Instruction::Subw {
        rd: word.rd(),
        rs1: word.rs1(),
        rs2: word.rs2(),
    }
}

fn sllw(word: Word) -> Instruction {
    Instruction::Sllw {
        rd: word.rd(),
        rs1: word.rs1(),
        rs2: word.rs2(),
    }
}

fn srlw(word: Word) -> Instruction {
    Instruction::Srlw {
        rd: word.rd(),
        rs1: word.rs1(),
        rs2: word.rs2(),
    }
}

fn sraw(word: Word) -> Instruction {
    Instruction::Sraw {
        rd: word.rd(),
        rs1: word.rs1(),
        rs2: word.rs2(),
    }
}

fn beq(word: Word) -> Instruction {
    Instruction::Beq {
        rs1: word.rs1(),
        rs2: word.rs2(),
        offset: word.b_immediate(),
    }
}

fn lui(word: Word) -> Instruction {
    Instruction::Lui {
        rd: word.rd(),
        imm: word.u_immediate(),
    }
}

fn auipc(word: Word) -> Instruction {
    Instruction::Auipc {
        rd: word.rd(),
        imm: word.u_immediate(),
    }
}

fn jalr(word: Word) -> Instruction {
    Instruction::Jalr {
        rd: word.rd(),
        rs1: word.rs1(),
        offset: word.i_immediate(),
    }
}

fn jal(word: Word) -> Instruction {
    Instruction::Jal {
        rd: word.rd(),
        offset: word.j_immediate(),
    }
}

fn bne(word: Word) -> Instruction {
    Instruction::Bne {
        rs1: word.rs1(),
        rs2: word.rs2(),
        offset: word.b_immediate(),
    }
}

fn blt(word: Word) -> Instruction {
    Instruction::Blt {
        rs1: word.rs1(),
        rs2: word.rs2(),
        offset: word.b_immediate(),
    }
}

fn bge(word: Word) -> Instruction {
    Instruction::Bge {
        rs1: word.rs1(),
        rs2: word.rs2(),
        offset: word.b_immediate(),
    }
}

fn bltu(word: Word) -> Instruction {
    Instruction::Bltu {
        rs1: word.rs1(),
        rs2: word.rs2(),
        offset: word.b_immediate(),
    }
}

fn bgeu(word: Word) -> Instruction {
    Instruction::Bgeu {
        rs1: word.rs1(),
        rs2: word.rs2(),
        offset: word.b_immediate(),
    }
}

fn lb(word: Word) -> Instruction {
    Instruction::Lb {
        rd: word.rd(),
        rs1: word.rs1(),
        offset: word.i_immediate(),
    }
}

fn lh(word: Word) -> Instruction {
    Instruction::Lh {
        rd: word.rd(),
        rs1: word.rs1(),
        offset: word.i_immediate(),
    }
}

fn lw(word: Word) -> Instruction {
    Instruction::Lw {
        rd: word.rd(),
        rs1: word.rs1(),
        offset: word.i_immediate(),
    }
}

fn ld(word: Word) -> Instruction {
    Instruction::Ld {
        rd: word.rd(),
        rs1: word.rs1(),
        offset: word.i_immediate(),
    }
}

fn lbu(word: Word) -> Instruction {
    Instruction::Lbu {
        rd: word.rd(),
        rs1: word.rs1(),
        offset: word.i_immediate(),
    }
}

fn lhu(word: Word) -> Instruction {
    Instruction::Lhu {
        rd: word.rd(),
        rs1: word.rs1(),
        offset: word.i_immediate(),
    }
}

fn lwu(word: Word) -> Instruction {
    Instruction::Lwu {
        rd: word.rd(),
        rs1: word.rs1(),
        offset: word.i_immediate(),
    }
}

fn sb(word: Word) -> Instruction {
    Instruction::Sb {
        rs2: word.rs2(),
        rs1: word.rs1(),
        offset: word.s_immediate(),
    }
}

fn sh(word: Word) -> Instruction {
    Instruction::Sh {
        rs2: word.rs2(),
        rs1: word.rs1(),
        offset: word.s_immediate(),
    }
}

fn sw(word: Word) -> Instruction {
    Instruction::Sw {
        rs2: word.rs2(),
        rs1: word.rs1(),
        offset: word.s_immediate(),
    }
}

fn sd(word: Word) -> Instruction {
    Instruction::Sd {
        rs2: word.rs2(),
        rs1: word.rs1(),
        offset: word.s_immediate(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decode::decode;
    use crate::trap::Trap;

    #[test]
    fn every_row_matches_its_own_code() {
        for row in TABLE {
            let word = Word::new(row.code);
            assert_eq!(
                lookup(word),
                Some((row.build)(word)),
                "{:#010x} does not decode to its own row",
                row.code
            );
        }
    }

    #[test]
    fn no_row_shadows_an_earlier_one() {
        for row in TABLE {
            let ignored = !row.mask;
            if ignored == 0 {
                continue;
            }
            // Set the lowest bit the mask ignores: the row must still win.
            let witness = Word::new(row.code | ignored & ignored.wrapping_neg());
            assert_eq!(
                lookup(witness),
                Some((row.build)(witness)),
                "{:#010x} is shadowed when an ignored bit is set",
                row.code
            );
        }
    }

    #[test]
    fn unimplemented_instructions_do_not_decode() {
        // Legal encodings that this milestone does not run: the M extension, and
        // the system instructions that wait for the trap milestone.
        for code in [inst::MATCH_MUL, inst::MATCH_FENCE, inst::MATCH_ECALL] {
            assert_eq!(decode(code), Err(Trap::IllegalInstruction), "{code:#010x}");
        }
    }
}
