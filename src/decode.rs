//! Instruction decoding.

use crate::cpu::abi_name;
use crate::inst;
use crate::opcodes;
use crate::trap::Trap;

/// A decoded instruction: one variant per mnemonic, with named fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Instruction {
    // Register-immediate ALU operations.
    /// `addi rd, rs1, imm`
    Addi { rd: u8, rs1: u8, imm: i64 },
    /// `slti rd, rs1, imm`
    Slti { rd: u8, rs1: u8, imm: i64 },
    /// `sltiu rd, rs1, imm`
    Sltiu { rd: u8, rs1: u8, imm: i64 },
    /// `xori rd, rs1, imm`
    Xori { rd: u8, rs1: u8, imm: i64 },
    /// `ori rd, rs1, imm`
    Ori { rd: u8, rs1: u8, imm: i64 },
    /// `andi rd, rs1, imm`
    Andi { rd: u8, rs1: u8, imm: i64 },
    /// `slli rd, rs1, shamt`
    Slli { rd: u8, rs1: u8, shamt: u8 },
    /// `srli rd, rs1, shamt`
    Srli { rd: u8, rs1: u8, shamt: u8 },
    /// `srai rd, rs1, shamt`
    Srai { rd: u8, rs1: u8, shamt: u8 },

    // Register-immediate ALU operations on 32-bit values.
    /// `addiw rd, rs1, imm`
    Addiw { rd: u8, rs1: u8, imm: i64 },
    /// `slliw rd, rs1, shamt`
    Slliw { rd: u8, rs1: u8, shamt: u8 },
    /// `srliw rd, rs1, shamt`
    Srliw { rd: u8, rs1: u8, shamt: u8 },
    /// `sraiw rd, rs1, shamt`
    Sraiw { rd: u8, rs1: u8, shamt: u8 },

    // Register-register ALU operations.
    /// `add rd, rs1, rs2`
    Add { rd: u8, rs1: u8, rs2: u8 },
    /// `sub rd, rs1, rs2`
    Sub { rd: u8, rs1: u8, rs2: u8 },
    /// `sll rd, rs1, rs2`
    Sll { rd: u8, rs1: u8, rs2: u8 },
    /// `slt rd, rs1, rs2`
    Slt { rd: u8, rs1: u8, rs2: u8 },
    /// `sltu rd, rs1, rs2`
    Sltu { rd: u8, rs1: u8, rs2: u8 },
    /// `xor rd, rs1, rs2`
    Xor { rd: u8, rs1: u8, rs2: u8 },
    /// `srl rd, rs1, rs2`
    Srl { rd: u8, rs1: u8, rs2: u8 },
    /// `sra rd, rs1, rs2`
    Sra { rd: u8, rs1: u8, rs2: u8 },
    /// `or rd, rs1, rs2`
    Or { rd: u8, rs1: u8, rs2: u8 },
    /// `and rd, rs1, rs2`
    And { rd: u8, rs1: u8, rs2: u8 },

    // Register-register ALU operations on 32-bit values.
    /// `addw rd, rs1, rs2`
    Addw { rd: u8, rs1: u8, rs2: u8 },
    /// `subw rd, rs1, rs2`
    Subw { rd: u8, rs1: u8, rs2: u8 },
    /// `sllw rd, rs1, rs2`
    Sllw { rd: u8, rs1: u8, rs2: u8 },
    /// `srlw rd, rs1, rs2`
    Srlw { rd: u8, rs1: u8, rs2: u8 },
    /// `sraw rd, rs1, rs2`
    Sraw { rd: u8, rs1: u8, rs2: u8 },

    // M-extension multiplication.
    /// `mul rd, rs1, rs2`
    Mul { rd: u8, rs1: u8, rs2: u8 },
    /// `mulh rd, rs1, rs2`
    Mulh { rd: u8, rs1: u8, rs2: u8 },
    /// `mulhsu rd, rs1, rs2`
    Mulhsu { rd: u8, rs1: u8, rs2: u8 },
    /// `mulhu rd, rs1, rs2`
    Mulhu { rd: u8, rs1: u8, rs2: u8 },
    /// `mulw rd, rs1, rs2`
    Mulw { rd: u8, rs1: u8, rs2: u8 },

    // M-extension division and remainder.
    /// `div rd, rs1, rs2`
    Div { rd: u8, rs1: u8, rs2: u8 },
    /// `divu rd, rs1, rs2`
    Divu { rd: u8, rs1: u8, rs2: u8 },
    /// `rem rd, rs1, rs2`
    Rem { rd: u8, rs1: u8, rs2: u8 },
    /// `remu rd, rs1, rs2`
    Remu { rd: u8, rs1: u8, rs2: u8 },
    /// `divw rd, rs1, rs2`
    Divw { rd: u8, rs1: u8, rs2: u8 },
    /// `divuw rd, rs1, rs2`
    Divuw { rd: u8, rs1: u8, rs2: u8 },
    /// `remw rd, rs1, rs2`
    Remw { rd: u8, rs1: u8, rs2: u8 },
    /// `remuw rd, rs1, rs2`
    Remuw { rd: u8, rs1: u8, rs2: u8 },

    // Loads.
    /// `lb rd, offset(rs1)`
    Lb { rd: u8, rs1: u8, offset: i64 },
    /// `lh rd, offset(rs1)`
    Lh { rd: u8, rs1: u8, offset: i64 },
    /// `lw rd, offset(rs1)`
    Lw { rd: u8, rs1: u8, offset: i64 },
    /// `ld rd, offset(rs1)`
    Ld { rd: u8, rs1: u8, offset: i64 },
    /// `lbu rd, offset(rs1)`
    Lbu { rd: u8, rs1: u8, offset: i64 },
    /// `lhu rd, offset(rs1)`
    Lhu { rd: u8, rs1: u8, offset: i64 },
    /// `lwu rd, offset(rs1)`
    Lwu { rd: u8, rs1: u8, offset: i64 },

    // Stores.
    /// `sb rs2, offset(rs1)`
    Sb { rs2: u8, rs1: u8, offset: i64 },
    /// `sh rs2, offset(rs1)`
    Sh { rs2: u8, rs1: u8, offset: i64 },
    /// `sw rs2, offset(rs1)`
    Sw { rs2: u8, rs1: u8, offset: i64 },
    /// `sd rs2, offset(rs1)`
    Sd { rs2: u8, rs1: u8, offset: i64 },

    // Ordering.
    /// `fence pred, succ` — the two four-bit access sets.
    Fence { pred: u8, succ: u8 },
    /// `fence.i` — instruction-fetch fence, from Zifencei.
    FenceI,

    // Zicsr.
    /// `csrrw rd, csr, rs1`
    Csrrw { rd: u8, csr: u16, rs1: u8 },
    /// `csrrs rd, csr, rs1`
    Csrrs { rd: u8, csr: u16, rs1: u8 },
    /// `csrrc rd, csr, rs1`
    Csrrc { rd: u8, csr: u16, rs1: u8 },
    /// `csrrwi rd, csr, zimm`
    Csrrwi { rd: u8, csr: u16, zimm: u8 },
    /// `csrrsi rd, csr, zimm`
    Csrrsi { rd: u8, csr: u16, zimm: u8 },
    /// `csrrci rd, csr, zimm`
    Csrrci { rd: u8, csr: u16, zimm: u8 },

    // Machine-mode system instructions.
    /// `ecall`
    Ecall,
    /// `ebreak`
    Ebreak,
    /// `mret`
    Mret,

    // Control transfers.
    /// `lui rd, imm` — `imm` is the value after the 12-bit shift.
    Lui { rd: u8, imm: i64 },
    /// `auipc rd, imm` — `imm` is the value after the 12-bit shift.
    Auipc { rd: u8, imm: i64 },
    /// `jalr rd, offset(rs1)`
    Jalr { rd: u8, rs1: u8, offset: i64 },
    /// `beq rs1, rs2, offset` — offset is relative to the instruction address.
    Beq { rs1: u8, rs2: u8, offset: i64 },
    /// `bne rs1, rs2, offset`
    Bne { rs1: u8, rs2: u8, offset: i64 },
    /// `blt rs1, rs2, offset`
    Blt { rs1: u8, rs2: u8, offset: i64 },
    /// `bge rs1, rs2, offset`
    Bge { rs1: u8, rs2: u8, offset: i64 },
    /// `bltu rs1, rs2, offset`
    Bltu { rs1: u8, rs2: u8, offset: i64 },
    /// `bgeu rs1, rs2, offset`
    Bgeu { rs1: u8, rs2: u8, offset: i64 },
    /// `jal rd, offset` — offset is relative to the instruction address.
    Jal { rd: u8, offset: i64 },
}

impl Instruction {
    /// Mnemonic spelled the way `objdump -M no-aliases` spells it.
    pub fn mnemonic(&self) -> &'static str {
        match *self {
            Instruction::Addi { .. } => "addi",
            Instruction::Slti { .. } => "slti",
            Instruction::Sltiu { .. } => "sltiu",
            Instruction::Xori { .. } => "xori",
            Instruction::Ori { .. } => "ori",
            Instruction::Andi { .. } => "andi",
            Instruction::Slli { .. } => "slli",
            Instruction::Srli { .. } => "srli",
            Instruction::Srai { .. } => "srai",
            Instruction::Addiw { .. } => "addiw",
            Instruction::Slliw { .. } => "slliw",
            Instruction::Srliw { .. } => "srliw",
            Instruction::Sraiw { .. } => "sraiw",
            Instruction::Add { .. } => "add",
            Instruction::Sub { .. } => "sub",
            Instruction::Sll { .. } => "sll",
            Instruction::Slt { .. } => "slt",
            Instruction::Sltu { .. } => "sltu",
            Instruction::Xor { .. } => "xor",
            Instruction::Srl { .. } => "srl",
            Instruction::Sra { .. } => "sra",
            Instruction::Or { .. } => "or",
            Instruction::And { .. } => "and",
            Instruction::Addw { .. } => "addw",
            Instruction::Subw { .. } => "subw",
            Instruction::Sllw { .. } => "sllw",
            Instruction::Srlw { .. } => "srlw",
            Instruction::Sraw { .. } => "sraw",
            Instruction::Mul { .. } => "mul",
            Instruction::Mulh { .. } => "mulh",
            Instruction::Mulhsu { .. } => "mulhsu",
            Instruction::Mulhu { .. } => "mulhu",
            Instruction::Mulw { .. } => "mulw",
            Instruction::Div { .. } => "div",
            Instruction::Divu { .. } => "divu",
            Instruction::Rem { .. } => "rem",
            Instruction::Remu { .. } => "remu",
            Instruction::Divw { .. } => "divw",
            Instruction::Divuw { .. } => "divuw",
            Instruction::Remw { .. } => "remw",
            Instruction::Remuw { .. } => "remuw",
            Instruction::Lb { .. } => "lb",
            Instruction::Lh { .. } => "lh",
            Instruction::Lw { .. } => "lw",
            Instruction::Ld { .. } => "ld",
            Instruction::Lbu { .. } => "lbu",
            Instruction::Lhu { .. } => "lhu",
            Instruction::Lwu { .. } => "lwu",
            Instruction::Sb { .. } => "sb",
            Instruction::Sh { .. } => "sh",
            Instruction::Sw { .. } => "sw",
            Instruction::Sd { .. } => "sd",
            Instruction::Fence { .. } => "fence",
            Instruction::FenceI => "fence.i",
            Instruction::Csrrw { .. } => "csrrw",
            Instruction::Csrrs { .. } => "csrrs",
            Instruction::Csrrc { .. } => "csrrc",
            Instruction::Csrrwi { .. } => "csrrwi",
            Instruction::Csrrsi { .. } => "csrrsi",
            Instruction::Csrrci { .. } => "csrrci",
            Instruction::Ecall => "ecall",
            Instruction::Ebreak => "ebreak",
            Instruction::Mret => "mret",
            Instruction::Lui { .. } => "lui",
            Instruction::Auipc { .. } => "auipc",
            Instruction::Jalr { .. } => "jalr",
            Instruction::Beq { .. } => "beq",
            Instruction::Bne { .. } => "bne",
            Instruction::Blt { .. } => "blt",
            Instruction::Bge { .. } => "bge",
            Instruction::Bltu { .. } => "bltu",
            Instruction::Bgeu { .. } => "bgeu",
            Instruction::Jal { .. } => "jal",
        }
    }

    /// Renders the instruction the way the trace prints it: ABI register names,
    /// decimal immediates, and a PC-relative target for control transfers.
    pub fn render(&self) -> String {
        match *self {
            Instruction::Addi { rd, rs1, imm } => imm_op("addi", rd, rs1, imm),
            Instruction::Slti { rd, rs1, imm } => imm_op("slti", rd, rs1, imm),
            Instruction::Sltiu { rd, rs1, imm } => imm_op("sltiu", rd, rs1, imm),
            Instruction::Xori { rd, rs1, imm } => imm_op("xori", rd, rs1, imm),
            Instruction::Ori { rd, rs1, imm } => imm_op("ori", rd, rs1, imm),
            Instruction::Andi { rd, rs1, imm } => imm_op("andi", rd, rs1, imm),
            Instruction::Slli { rd, rs1, shamt } => shamt_op("slli", rd, rs1, shamt),
            Instruction::Srli { rd, rs1, shamt } => shamt_op("srli", rd, rs1, shamt),
            Instruction::Srai { rd, rs1, shamt } => shamt_op("srai", rd, rs1, shamt),
            Instruction::Addiw { rd, rs1, imm } => imm_op("addiw", rd, rs1, imm),
            Instruction::Slliw { rd, rs1, shamt } => shamt_op("slliw", rd, rs1, shamt),
            Instruction::Srliw { rd, rs1, shamt } => shamt_op("srliw", rd, rs1, shamt),
            Instruction::Sraiw { rd, rs1, shamt } => shamt_op("sraiw", rd, rs1, shamt),
            Instruction::Add { rd, rs1, rs2 } => reg_op("add", rd, rs1, rs2),
            Instruction::Sub { rd, rs1, rs2 } => reg_op("sub", rd, rs1, rs2),
            Instruction::Sll { rd, rs1, rs2 } => reg_op("sll", rd, rs1, rs2),
            Instruction::Slt { rd, rs1, rs2 } => reg_op("slt", rd, rs1, rs2),
            Instruction::Sltu { rd, rs1, rs2 } => reg_op("sltu", rd, rs1, rs2),
            Instruction::Xor { rd, rs1, rs2 } => reg_op("xor", rd, rs1, rs2),
            Instruction::Srl { rd, rs1, rs2 } => reg_op("srl", rd, rs1, rs2),
            Instruction::Sra { rd, rs1, rs2 } => reg_op("sra", rd, rs1, rs2),
            Instruction::Or { rd, rs1, rs2 } => reg_op("or", rd, rs1, rs2),
            Instruction::And { rd, rs1, rs2 } => reg_op("and", rd, rs1, rs2),
            Instruction::Addw { rd, rs1, rs2 } => reg_op("addw", rd, rs1, rs2),
            Instruction::Subw { rd, rs1, rs2 } => reg_op("subw", rd, rs1, rs2),
            Instruction::Sllw { rd, rs1, rs2 } => reg_op("sllw", rd, rs1, rs2),
            Instruction::Srlw { rd, rs1, rs2 } => reg_op("srlw", rd, rs1, rs2),
            Instruction::Sraw { rd, rs1, rs2 } => reg_op("sraw", rd, rs1, rs2),
            Instruction::Mul { rd, rs1, rs2 } => reg_op("mul", rd, rs1, rs2),
            Instruction::Mulh { rd, rs1, rs2 } => reg_op("mulh", rd, rs1, rs2),
            Instruction::Mulhsu { rd, rs1, rs2 } => reg_op("mulhsu", rd, rs1, rs2),
            Instruction::Mulhu { rd, rs1, rs2 } => reg_op("mulhu", rd, rs1, rs2),
            Instruction::Mulw { rd, rs1, rs2 } => reg_op("mulw", rd, rs1, rs2),
            Instruction::Div { rd, rs1, rs2 } => reg_op("div", rd, rs1, rs2),
            Instruction::Divu { rd, rs1, rs2 } => reg_op("divu", rd, rs1, rs2),
            Instruction::Rem { rd, rs1, rs2 } => reg_op("rem", rd, rs1, rs2),
            Instruction::Remu { rd, rs1, rs2 } => reg_op("remu", rd, rs1, rs2),
            Instruction::Divw { rd, rs1, rs2 } => reg_op("divw", rd, rs1, rs2),
            Instruction::Divuw { rd, rs1, rs2 } => reg_op("divuw", rd, rs1, rs2),
            Instruction::Remw { rd, rs1, rs2 } => reg_op("remw", rd, rs1, rs2),
            Instruction::Remuw { rd, rs1, rs2 } => reg_op("remuw", rd, rs1, rs2),
            Instruction::Lb { rd, rs1, offset } => load_op("lb", rd, rs1, offset),
            Instruction::Lh { rd, rs1, offset } => load_op("lh", rd, rs1, offset),
            Instruction::Lw { rd, rs1, offset } => load_op("lw", rd, rs1, offset),
            Instruction::Ld { rd, rs1, offset } => load_op("ld", rd, rs1, offset),
            Instruction::Lbu { rd, rs1, offset } => load_op("lbu", rd, rs1, offset),
            Instruction::Lhu { rd, rs1, offset } => load_op("lhu", rd, rs1, offset),
            Instruction::Lwu { rd, rs1, offset } => load_op("lwu", rd, rs1, offset),
            Instruction::Sb { rs2, rs1, offset } => store_op("sb", rs2, rs1, offset),
            Instruction::Sh { rs2, rs1, offset } => store_op("sh", rs2, rs1, offset),
            Instruction::Sw { rs2, rs1, offset } => store_op("sw", rs2, rs1, offset),
            Instruction::Sd { rs2, rs1, offset } => store_op("sd", rs2, rs1, offset),
            Instruction::Fence { pred, succ } => {
                format!("fence {},{}", fence_set(pred), fence_set(succ))
            }
            Instruction::FenceI => "fence.i".to_owned(),
            Instruction::Csrrw { rd, csr, rs1 } => csr_reg_op("csrrw", rd, csr, rs1),
            Instruction::Csrrs { rd, csr, rs1 } => csr_reg_op("csrrs", rd, csr, rs1),
            Instruction::Csrrc { rd, csr, rs1 } => csr_reg_op("csrrc", rd, csr, rs1),
            Instruction::Csrrwi { rd, csr, zimm } => csr_imm_op("csrrwi", rd, csr, zimm),
            Instruction::Csrrsi { rd, csr, zimm } => csr_imm_op("csrrsi", rd, csr, zimm),
            Instruction::Csrrci { rd, csr, zimm } => csr_imm_op("csrrci", rd, csr, zimm),
            Instruction::Ecall => "ecall".to_owned(),
            Instruction::Ebreak => "ebreak".to_owned(),
            Instruction::Mret => "mret".to_owned(),
            Instruction::Lui { rd, imm } => format!("lui {},{:#x}", abi_name(rd), u_field(imm)),
            Instruction::Auipc { rd, imm } => {
                format!("auipc {},{:#x}", abi_name(rd), u_field(imm))
            }
            Instruction::Jalr { rd, rs1, offset } => {
                format!("jalr {},{}({})", abi_name(rd), offset, abi_name(rs1))
            }
            Instruction::Beq { rs1, rs2, offset } => branch_op("beq", rs1, rs2, offset),
            Instruction::Bne { rs1, rs2, offset } => branch_op("bne", rs1, rs2, offset),
            Instruction::Blt { rs1, rs2, offset } => branch_op("blt", rs1, rs2, offset),
            Instruction::Bge { rs1, rs2, offset } => branch_op("bge", rs1, rs2, offset),
            Instruction::Bltu { rs1, rs2, offset } => branch_op("bltu", rs1, rs2, offset),
            Instruction::Bgeu { rs1, rs2, offset } => branch_op("bgeu", rs1, rs2, offset),
            Instruction::Jal { rd, offset } => {
                format!("jal {},{}", abi_name(rd), relative(offset))
            }
        }
    }
}

/// `mnemonic rd,rs1,imm`
fn imm_op(mnemonic: &str, rd: u8, rs1: u8, imm: i64) -> String {
    format!("{mnemonic} {},{},{imm}", abi_name(rd), abi_name(rs1))
}

/// `mnemonic rd,rs1,shamt`
fn shamt_op(mnemonic: &str, rd: u8, rs1: u8, shamt: u8) -> String {
    format!("{mnemonic} {},{},{shamt:#x}", abi_name(rd), abi_name(rs1))
}

/// `mnemonic rd,rs1,rs2`
fn reg_op(mnemonic: &str, rd: u8, rs1: u8, rs2: u8) -> String {
    format!(
        "{mnemonic} {},{},{}",
        abi_name(rd),
        abi_name(rs1),
        abi_name(rs2)
    )
}

/// `mnemonic rs1,rs2,pc ± offset`
fn branch_op(mnemonic: &str, rs1: u8, rs2: u8, offset: i64) -> String {
    format!(
        "{mnemonic} {},{},{}",
        abi_name(rs1),
        abi_name(rs2),
        relative(offset)
    )
}

/// `mnemonic rd,offset(rs1)`
fn load_op(mnemonic: &str, rd: u8, rs1: u8, offset: i64) -> String {
    format!("{mnemonic} {},{offset}({})", abi_name(rd), abi_name(rs1))
}

/// `mnemonic rs2,offset(rs1)`
fn store_op(mnemonic: &str, rs2: u8, rs1: u8, offset: i64) -> String {
    format!("{mnemonic} {},{offset}({})", abi_name(rs2), abi_name(rs1))
}

/// `mnemonic rd,csr,rs1`
fn csr_reg_op(mnemonic: &str, rd: u8, csr: u16, rs1: u8) -> String {
    format!(
        "{mnemonic} {},{},{}",
        abi_name(rd),
        csr_name(csr),
        abi_name(rs1)
    )
}

/// `mnemonic rd,csr,zimm`
fn csr_imm_op(mnemonic: &str, rd: u8, csr: u16, zimm: u8) -> String {
    format!("{mnemonic} {},{},{zimm}", abi_name(rd), csr_name(csr))
}

/// Spell the CSRs implemented by this milestone as `objdump` does.
fn csr_name(csr: u16) -> String {
    match csr {
        inst::CSR_MSTATUS => "mstatus".to_owned(),
        inst::CSR_MISA => "misa".to_owned(),
        inst::CSR_MTVEC => "mtvec".to_owned(),
        inst::CSR_MSCRATCH => "mscratch".to_owned(),
        inst::CSR_MEPC => "mepc".to_owned(),
        inst::CSR_MCAUSE => "mcause".to_owned(),
        inst::CSR_MTVAL => "mtval".to_owned(),
        _ => format!("{csr:#x}"),
    }
}

/// Renders a four-bit fence access set the way `objdump` does, in `i`, `o`, `r`,
/// `w` order. An empty set cannot come from assembly; `objdump` calls it
/// `unknown`.
fn fence_set(bits: u8) -> String {
    let mut text = String::new();
    for (mask, letter) in [(0b1000, 'i'), (0b0100, 'o'), (0b0010, 'r'), (0b0001, 'w')] {
        if bits & mask != 0 {
            text.push(letter);
        }
    }
    if text.is_empty() {
        text.push_str("unknown");
    }
    text
}

/// Formats a PC-relative offset the way the trace prints it.
fn relative(offset: i64) -> String {
    if offset < 0 {
        format!("pc - {:#x}", offset.unsigned_abs())
    } else {
        format!("pc + {offset:#x}")
    }
}

/// The 20-bit U-type field as `objdump` prints it: the immediate before its
/// 12-bit shift.
fn u_field(imm: i64) -> u32 {
    ((imm >> 12) as u32) & 0xfffff
}

/// Decodes a 32-bit instruction word.
pub fn decode(word: u32) -> Result<Instruction, Trap> {
    opcodes::lookup(Word::new(word)).ok_or(Trap::IllegalInstruction)
}

/// A 32-bit instruction word whose accessors are named as the spec names its
/// fields, so each builder reads like the encoding table it implements.
#[derive(Clone, Copy)]
pub(crate) struct Word(u32);

impl Word {
    /// Wraps a raw instruction word.
    pub(crate) fn new(word: u32) -> Self {
        Self(word)
    }

    /// The raw instruction word, for table matching.
    pub(crate) fn raw(self) -> u32 {
        self.0
    }

    /// Bits `[shift + width - 1 : shift]`.
    fn bits(self, shift: u32, width: u32) -> u32 {
        (self.0 >> shift) & ((1 << width) - 1)
    }

    /// Destination register, bits 11:7.
    pub(crate) fn rd(self) -> u8 {
        self.bits(7, 5) as u8
    }

    /// First source register, bits 19:15.
    pub(crate) fn rs1(self) -> u8 {
        self.bits(15, 5) as u8
    }

    /// Second source register, bits 24:20.
    pub(crate) fn rs2(self) -> u8 {
        self.bits(20, 5) as u8
    }

    /// CSR address, bits 31:20.
    pub(crate) fn csr(self) -> u16 {
        self.bits(20, 12) as u16
    }

    /// Five-bit unsigned immediate for the immediate CSR operations.
    pub(crate) fn zimm(self) -> u8 {
        self.rs1()
    }

    /// I-type immediate: `imm[11:0]`, bits 31:20.
    pub(crate) fn i_immediate(self) -> i64 {
        sign_extend(self.bits(20, 12), 12)
    }

    /// U-type immediate: `imm[31:12]` shifted into place.
    pub(crate) fn u_immediate(self) -> i64 {
        sign_extend(self.bits(12, 20), 20) << 12
    }

    /// S-type immediate: `imm[11:5]` at bits 31:25, `imm[4:0]` at bits 11:7.
    pub(crate) fn s_immediate(self) -> i64 {
        let value = (self.bits(25, 7) << 5) | self.bits(7, 5);
        sign_extend(value, 12)
    }

    /// Fence predecessor set, bits 27:24.
    pub(crate) fn pred(self) -> u8 {
        self.bits(24, 4) as u8
    }

    /// Fence successor set, bits 23:20.
    pub(crate) fn succ(self) -> u8 {
        self.bits(20, 4) as u8
    }

    /// Shift amount of the 64-bit shifts: `imm[5:0]`.
    pub(crate) fn shamt(self) -> u8 {
        self.bits(20, 6) as u8
    }

    /// Shift amount of the W-forms: `imm[4:0]`.
    pub(crate) fn shamt_w(self) -> u8 {
        self.bits(20, 5) as u8
    }

    /// B-type offset: `imm[12|10:5]` at bits 31:25, `imm[4:1|11]` at bits 11:7.
    pub(crate) fn b_immediate(self) -> i64 {
        let value = (self.bits(31, 1) << 12)
            | (self.bits(7, 1) << 11)
            | (self.bits(25, 6) << 5)
            | (self.bits(8, 4) << 1);
        sign_extend(value, 13)
    }

    /// J-type offset: `imm[20|10:1|11|19:12]`, bits 31:12.
    pub(crate) fn j_immediate(self) -> i64 {
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
