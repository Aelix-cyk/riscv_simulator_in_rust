//! Instruction decoding for the instructions the walking skeleton executes.

use crate::cpu::abi_name;
use crate::trap::Trap;

/// Opcodes of the instructions understood so far.
const OP_IMM: u32 = 0b001_0011;
const OP: u32 = 0b011_0011;
const BRANCH: u32 = 0b110_0011;
const JAL: u32 = 0b110_1111;

/// `funct3` values that select the understood operations.
const ADDI_FUNCT3: u32 = 0b000;
const ADD_FUNCT3: u32 = 0b000;
const BEQ_FUNCT3: u32 = 0b000;

/// `funct7` values that select the understood operations.
const ADD_FUNCT7: u32 = 0b000_0000;

/// A decoded instruction. Fields keep encoding widths rather than `usize`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Instruction {
    /// `addi rd, rs1, imm`
    Addi { rd: u8, rs1: u8, imm: i64 },
    /// `add rd, rs1, rs2`
    Add { rd: u8, rs1: u8, rs2: u8 },
    /// `beq rs1, rs2, offset` — offset is relative to the instruction address.
    Beq { rs1: u8, rs2: u8, offset: i64 },
    /// `jal rd, offset` — offset is relative to the instruction address.
    Jal { rd: u8, offset: i64 },
}

impl Instruction {
    /// Mnemonic spelled the way `objdump -M no-aliases` spells it.
    pub fn mnemonic(&self) -> &'static str {
        match self {
            Instruction::Addi { .. } => "addi",
            Instruction::Add { .. } => "add",
            Instruction::Beq { .. } => "beq",
            Instruction::Jal { .. } => "jal",
        }
    }

    /// Renders the instruction the way the trace prints it: ABI register names,
    /// decimal immediates, and a PC-relative target for control transfers.
    pub fn render(&self) -> String {
        match *self {
            Instruction::Addi { rd, rs1, imm } => {
                format!("addi {},{},{imm}", abi_name(rd), abi_name(rs1))
            }
            Instruction::Add { rd, rs1, rs2 } => {
                format!("add {},{},{}", abi_name(rd), abi_name(rs1), abi_name(rs2))
            }
            Instruction::Beq { rs1, rs2, offset } => format!(
                "beq {},{},{}",
                abi_name(rs1),
                abi_name(rs2),
                relative(offset)
            ),
            Instruction::Jal { rd, offset } => {
                format!("jal {},{}", abi_name(rd), relative(offset))
            }
        }
    }
}

/// Formats a PC-relative offset the way the trace prints it.
fn relative(offset: i64) -> String {
    if offset < 0 {
        format!("pc - {:#x}", offset.unsigned_abs())
    } else {
        format!("pc + {offset:#x}")
    }
}

/// Decodes a 32-bit instruction word.
pub fn decode(word: u32) -> Result<Instruction, Trap> {
    let word = Word(word);

    match word.opcode() {
        OP_IMM => decode_op_imm(word),
        OP => decode_op(word),
        BRANCH => decode_branch(word),
        JAL => decode_jal(word),
        _ => Err(Trap::IllegalInstruction),
    }
}

/// `addi rd, rs1, imm`
fn decode_op_imm(word: Word) -> Result<Instruction, Trap> {
    if word.funct3() != ADDI_FUNCT3 {
        return Err(Trap::IllegalInstruction);
    }
    Ok(Instruction::Addi {
        rd: word.rd(),
        rs1: word.rs1(),
        imm: word.i_immediate(),
    })
}

/// `add rd, rs1, rs2`; `sub` and the remaining funct7 variants come later.
fn decode_op(word: Word) -> Result<Instruction, Trap> {
    if word.funct3() != ADD_FUNCT3 || word.funct7() != ADD_FUNCT7 {
        return Err(Trap::IllegalInstruction);
    }
    Ok(Instruction::Add {
        rd: word.rd(),
        rs1: word.rs1(),
        rs2: word.rs2(),
    })
}

/// `beq rs1, rs2, offset`
fn decode_branch(word: Word) -> Result<Instruction, Trap> {
    if word.funct3() != BEQ_FUNCT3 {
        return Err(Trap::IllegalInstruction);
    }
    Ok(Instruction::Beq {
        rs1: word.rs1(),
        rs2: word.rs2(),
        offset: word.b_immediate(),
    })
}

/// `jal rd, offset`. J-type has no `funct3` field: bits 14:12 are `imm[19:12]`.
fn decode_jal(word: Word) -> Result<Instruction, Trap> {
    Ok(Instruction::Jal {
        rd: word.rd(),
        offset: word.j_immediate(),
    })
}

/// A 32-bit instruction word whose accessors are named as the spec names its
/// fields, so each decode arm reads like the encoding table it implements.
#[derive(Clone, Copy)]
struct Word(u32);

impl Word {
    /// Bits `[shift + width - 1 : shift]`.
    fn bits(self, shift: u32, width: u32) -> u32 {
        (self.0 >> shift) & ((1 << width) - 1)
    }

    /// Opcode, bits 6:0.
    fn opcode(self) -> u32 {
        self.bits(0, 7)
    }

    /// Destination register, bits 11:7.
    fn rd(self) -> u8 {
        self.bits(7, 5) as u8
    }

    /// Operation selector, bits 14:12.
    fn funct3(self) -> u32 {
        self.bits(12, 3)
    }

    /// First source register, bits 19:15.
    fn rs1(self) -> u8 {
        self.bits(15, 5) as u8
    }

    /// Second source register, bits 24:20.
    fn rs2(self) -> u8 {
        self.bits(20, 5) as u8
    }

    /// Operation selector, bits 31:25.
    fn funct7(self) -> u32 {
        self.bits(25, 7)
    }

    /// I-type immediate: `imm[11:0]`, bits 31:20.
    fn i_immediate(self) -> i64 {
        sign_extend(self.bits(20, 12), 12)
    }

    /// B-type offset: `imm[12|10:5]` at bits 31:25, `imm[4:1|11]` at bits 11:7.
    fn b_immediate(self) -> i64 {
        let value = (self.bits(31, 1) << 12)
            | (self.bits(7, 1) << 11)
            | (self.bits(25, 6) << 5)
            | (self.bits(8, 4) << 1);
        sign_extend(value, 13)
    }

    /// J-type offset: `imm[20|10:1|11|19:12]`, bits 31:12.
    fn j_immediate(self) -> i64 {
        let value = (self.bits(31, 1) << 20)
            | (self.bits(12, 8) << 12)
            | (self.bits(20, 1) << 11)
            | (self.bits(21, 10) << 1);
        sign_extend(value, 21)
    }
}

/// Sign-extends the low `width` bits of `value` to 64 bits.
fn sign_extend(value: u32, width: u32) -> i64 {
    let shift = 64 - width;
    (i64::from(value) << shift) >> shift
}
