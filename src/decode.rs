//! Instruction decoding.

use std::borrow::Cow;

use crate::cpu::{RA, SP, abi_name, fpr_abi_name};
use crate::inst;
use crate::opcodes;
use crate::trap::Trap;

const COMPRESSED_INSTRUCTION_BYTES: u64 = 2;
const STANDARD_INSTRUCTION_BYTES: u64 = 4;
const XLEN: u32 = 64;
const WORD_BITS: u32 = 32;
const REGISTER_BITS: u32 = 5;
const PRIME_REGISTER_BITS: u32 = 3;
const SINGLE_BIT_WIDTH: u32 = 1;
const PRIME_REGISTER_BASE: u8 = 8;
const MINOR_OPCODE_BITS: u32 = 2;
const MINOR_OPCODE_MASK: u16 = (1 << MINOR_OPCODE_BITS) - 1;
const STANDARD_MINOR_OPCODE: u16 = 0b11;
const ZERO_REGISTER: u8 = 0;
const ZERO_IMMEDIATE: i64 = 0;
const ZERO_OFFSET: i64 = 0;

const COMPRESSED_RD_SHIFT: u32 = 7;
const COMPRESSED_RS2_SHIFT: u32 = 2;
const PRIME_RD_SHIFT: u32 = 2;
const PRIME_RS1_SHIFT: u32 = 7;
const COMPRESSED_FUNCT3_SHIFT: u32 = 13;
const COMPRESSED_FUNCT3_BITS: u32 = 3;
const COMPRESSED_ALU_FUNCT2_SHIFT: u32 = 10;
const COMPRESSED_ALU_FUNCT2_BITS: u32 = 2;
const COMPRESSED_ALU_RS2_SHIFT: u32 = 5;
const COMPRESSED_ALU_RS2_BITS: u32 = 2;
const COMPRESSED_ALU_WIDTH_FLAG_SHIFT: u32 = 12;
const COMPRESSED_ALU_WIDTH_FLAG_BITS: u32 = 1;
const COMPRESSED_MISC_WIDTH_FLAG_SHIFT: u32 = 12;
const COMPRESSED_MISC_WIDTH_FLAG_BITS: u32 = 1;

const C0_ADDI4SPN: u8 = 0b000;
const C0_FLD: u8 = 0b001;
const C0_LW: u8 = 0b010;
const C0_LD: u8 = 0b011;
const C0_FSD: u8 = 0b101;
const C0_SW: u8 = 0b110;
const C0_SD: u8 = 0b111;
const C1_ADDI: u8 = 0b000;
const C1_ADDIW: u8 = 0b001;
const C1_LI: u8 = 0b010;
const C1_LUI_OR_ADDI16SP: u8 = 0b011;
const C1_ALU: u8 = 0b100;
const C1_J: u8 = 0b101;
const C1_BEQZ: u8 = 0b110;
const C1_BNEZ: u8 = 0b111;
const C2_SLLI: u8 = 0b000;
const C2_FLDSP: u8 = 0b001;
const C2_LWSP: u8 = 0b010;
const C2_LDSP: u8 = 0b011;
const C2_MISC: u8 = 0b100;
const C2_FSDSP: u8 = 0b101;
const C2_SWSP: u8 = 0b110;
const C2_SDSP: u8 = 0b111;

const C_ALU_SRLI: u16 = 0b00;
const C_ALU_SRAI: u16 = 0b01;
const C_ALU_ANDI: u16 = 0b10;
const C_ALU_REGISTER: u16 = 0b11;
const C_ALU_REGISTER_BASE: u16 = 0;
const C_ALU_REGISTER_WORD: u16 = 1;
const C_ALU_SUB: u16 = 0b00;
const C_ALU_XOR: u16 = 0b01;
const C_ALU_OR: u16 = 0b10;
const C_ALU_AND: u16 = 0b11;
const C_ALU_SUBW: u16 = 0b00;
const C_ALU_ADDW: u16 = 0b01;

const C_MISC_JR_OR_MV: u16 = 0;
const C_MISC_EBREAK_OR_JALR: u16 = 1;
const C_MISC_EMPTY_REGISTER: u8 = 0;

const IMMEDIATE_LSB_SHIFT: u32 = 0;
const U_IMMEDIATE_SHIFT: u32 = 12;
const CI_IMMEDIATE_BITS: u32 = 6;
const ADDI16SP_IMMEDIATE_BITS: u32 = 10;
const JUMP_IMMEDIATE_BITS: u32 = 12;
const BRANCH_IMMEDIATE_BITS: u32 = 9;

const STANDARD_RD_SHIFT: u32 = 7;
const STANDARD_RS1_SHIFT: u32 = 15;
const STANDARD_RS2_SHIFT: u32 = 20;
const STANDARD_AQ_SHIFT: u32 = 26;
const STANDARD_RL_SHIFT: u32 = 25;
const STANDARD_CSR_SHIFT: u32 = 20;
const STANDARD_CSR_BITS: u32 = 12;
const STANDARD_RS3_SHIFT: u32 = 27;
const STANDARD_ROUNDING_SHIFT: u32 = 12;
const STANDARD_ROUNDING_BITS: u32 = 3;
const ROUNDING_NEAREST_EVEN: u8 = 0;
const ROUNDING_TOWARD_ZERO: u8 = 1;
const ROUNDING_DOWN: u8 = 2;
const ROUNDING_UP: u8 = 3;
const ROUNDING_NEAREST_MAX_MAGNITUDE: u8 = 4;
const ROUNDING_DYNAMIC: u8 = 7;
const STANDARD_FENCE_PRED_SHIFT: u32 = 24;
const STANDARD_FENCE_SUCC_SHIFT: u32 = 20;
const STANDARD_FENCE_BITS: u32 = 4;
const STANDARD_SHIFT_AMOUNT_SHIFT: u32 = 20;
const STANDARD_SHIFT_AMOUNT_BITS: u32 = 6;
const WORD_SHIFT_AMOUNT_BITS: u32 = 5;
const U_IMMEDIATE_BITS: u32 = 20;
const I_IMMEDIATE_BITS: u32 = 12;
const S_IMMEDIATE_BITS: u32 = 12;
const B_IMMEDIATE_BITS: u32 = 13;
const J_IMMEDIATE_BITS: u32 = 21;
const U_IMMEDIATE_FIELD_BITS: u32 = 20;
const U_FIELD_MASK: u32 = (1 << U_IMMEDIATE_FIELD_BITS) - 1;

/// Moves a contiguous field from an instruction word into an immediate.
#[derive(Clone, Copy)]
struct Field {
    source_shift: u32,
    width: u32,
    target_shift: u32,
}

const fn field(source_shift: u32, width: u32, target_shift: u32) -> Field {
    Field {
        source_shift,
        width,
        target_shift,
    }
}

const CI_IMMEDIATE_FIELDS: &[Field] = &[field(12, 1, 5), field(2, 5, 0)];
const ADDI4SPN_IMMEDIATE_FIELDS: &[Field] = &[
    field(7, 4, 6),
    field(11, 2, 4),
    field(5, 1, 3),
    field(6, 1, 2),
];
const ADDI16SP_IMMEDIATE_FIELDS: &[Field] = &[
    field(12, 1, 9),
    field(3, 2, 7),
    field(5, 1, 6),
    field(2, 1, 5),
    field(6, 1, 4),
];
const LW_IMMEDIATE_FIELDS: &[Field] = &[field(5, 1, 6), field(10, 3, 3), field(6, 1, 2)];
const LD_IMMEDIATE_FIELDS: &[Field] = &[field(5, 2, 6), field(10, 3, 3)];
const J_IMMEDIATE_FIELDS: &[Field] = &[
    field(12, 1, 11),
    field(8, 1, 10),
    field(9, 2, 8),
    field(6, 1, 7),
    field(7, 1, 6),
    field(2, 1, 5),
    field(11, 1, 4),
    field(3, 3, 1),
];
const B_IMMEDIATE_FIELDS: &[Field] = &[
    field(12, 1, 8),
    field(5, 2, 6),
    field(2, 1, 5),
    field(10, 2, 3),
    field(3, 2, 1),
];
const LWSP_IMMEDIATE_FIELDS: &[Field] = &[field(2, 2, 6), field(12, 1, 5), field(4, 3, 2)];
const LDSP_IMMEDIATE_FIELDS: &[Field] = &[field(2, 3, 6), field(12, 1, 5), field(5, 2, 3)];
const SWSP_IMMEDIATE_FIELDS: &[Field] = &[field(7, 2, 6), field(9, 4, 2)];
const SDSP_IMMEDIATE_FIELDS: &[Field] = &[field(7, 3, 6), field(10, 3, 3)];
const U_IMMEDIATE_FIELDS: &[Field] = &[field(
    U_IMMEDIATE_SHIFT,
    U_IMMEDIATE_FIELD_BITS,
    IMMEDIATE_LSB_SHIFT,
)];
const I_IMMEDIATE_FIELDS: &[Field] = &[field(
    STANDARD_CSR_SHIFT,
    I_IMMEDIATE_BITS,
    IMMEDIATE_LSB_SHIFT,
)];
const S_IMMEDIATE_FIELDS: &[Field] = &[field(25, 7, 5), field(7, 5, 0)];
const STANDARD_B_IMMEDIATE_FIELDS: &[Field] = &[
    field(31, 1, 12),
    field(7, 1, 11),
    field(25, 6, 5),
    field(8, 4, 1),
];
const STANDARD_J_IMMEDIATE_FIELDS: &[Field] = &[
    field(31, 1, 20),
    field(12, 8, 12),
    field(20, 1, 11),
    field(21, 10, 1),
];

const fn mask_u16(width: u32) -> u16 {
    (1 << width) - 1
}

const fn mask_u32(width: u32) -> u32 {
    (1 << width) - 1
}

fn encode_immediate(base: u32, value: i64, fields: &[Field]) -> u32 {
    let value = value as u32;
    fields.iter().fold(base, |word, field| {
        let field_value = (value >> field.target_shift) & mask_u32(field.width);
        word | (field_value << field.source_shift)
    })
}

fn encode_i(base: u32, rd: u8, rs1: u8, immediate: i64) -> u32 {
    encode_immediate(base, immediate, I_IMMEDIATE_FIELDS)
        | (u32::from(rd) << STANDARD_RD_SHIFT)
        | (u32::from(rs1) << STANDARD_RS1_SHIFT)
}

fn encode_r(base: u32, rd: u8, rs1: u8, rs2: u8) -> u32 {
    base | (u32::from(rd) << STANDARD_RD_SHIFT)
        | (u32::from(rs1) << STANDARD_RS1_SHIFT)
        | (u32::from(rs2) << STANDARD_RS2_SHIFT)
}

fn encode_u(base: u32, rd: u8, immediate: i64) -> u32 {
    encode_immediate(base, immediate >> U_IMMEDIATE_SHIFT, U_IMMEDIATE_FIELDS)
        | (u32::from(rd) << STANDARD_RD_SHIFT)
}

fn store_register_fields(rs1: u8, rs2: u8) -> u32 {
    (u32::from(rs1) << STANDARD_RS1_SHIFT) | (u32::from(rs2) << STANDARD_RS2_SHIFT)
}

fn encode_nop() -> u32 {
    encode_i(
        inst::MATCH_ADDI,
        ZERO_REGISTER,
        ZERO_REGISTER,
        ZERO_IMMEDIATE,
    )
}

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

    // Zba address-generation operations.
    /// `add.uw rd, rs1, rs2`
    AddUw { rd: u8, rs1: u8, rs2: u8 },
    /// `sh1add rd, rs1, rs2`
    Sh1Add { rd: u8, rs1: u8, rs2: u8 },
    /// `sh1add.uw rd, rs1, rs2`
    Sh1AddUw { rd: u8, rs1: u8, rs2: u8 },
    /// `sh2add rd, rs1, rs2`
    Sh2Add { rd: u8, rs1: u8, rs2: u8 },
    /// `sh2add.uw rd, rs1, rs2`
    Sh2AddUw { rd: u8, rs1: u8, rs2: u8 },
    /// `sh3add rd, rs1, rs2`
    Sh3Add { rd: u8, rs1: u8, rs2: u8 },
    /// `sh3add.uw rd, rs1, rs2`
    Sh3AddUw { rd: u8, rs1: u8, rs2: u8 },
    /// `slli.uw rd, rs1, shamt`
    SlliUw { rd: u8, rs1: u8, shamt: u8 },

    // Zbb logical, count, comparison, extension, and rotation operations.
    /// `andn rd, rs1, rs2`
    Andn { rd: u8, rs1: u8, rs2: u8 },
    /// `orn rd, rs1, rs2`
    Orn { rd: u8, rs1: u8, rs2: u8 },
    /// `xnor rd, rs1, rs2`
    Xnor { rd: u8, rs1: u8, rs2: u8 },
    /// `clz rd, rs1`
    Clz { rd: u8, rs1: u8 },
    /// `clzw rd, rs1`
    Clzw { rd: u8, rs1: u8 },
    /// `ctz rd, rs1`
    Ctz { rd: u8, rs1: u8 },
    /// `ctzw rd, rs1`
    Ctzw { rd: u8, rs1: u8 },
    /// `cpop rd, rs1`
    Cpop { rd: u8, rs1: u8 },
    /// `cpopw rd, rs1`
    Cpopw { rd: u8, rs1: u8 },
    /// `max rd, rs1, rs2`
    Max { rd: u8, rs1: u8, rs2: u8 },
    /// `maxu rd, rs1, rs2`
    Maxu { rd: u8, rs1: u8, rs2: u8 },
    /// `min rd, rs1, rs2`
    Min { rd: u8, rs1: u8, rs2: u8 },
    /// `minu rd, rs1, rs2`
    Minu { rd: u8, rs1: u8, rs2: u8 },
    /// `sext.b rd, rs1`
    SextB { rd: u8, rs1: u8 },
    /// `sext.h rd, rs1`
    SextH { rd: u8, rs1: u8 },
    /// `zext.h rd, rs1`
    ZextH { rd: u8, rs1: u8 },
    /// `rol rd, rs1, rs2`
    Rol { rd: u8, rs1: u8, rs2: u8 },
    /// `rolw rd, rs1, rs2`
    Rolw { rd: u8, rs1: u8, rs2: u8 },
    /// `ror rd, rs1, rs2`
    Ror { rd: u8, rs1: u8, rs2: u8 },
    /// `rori rd, rs1, shamt`
    Rori { rd: u8, rs1: u8, shamt: u8 },
    /// `roriw rd, rs1, shamt`
    Roriw { rd: u8, rs1: u8, shamt: u8 },
    /// `rorw rd, rs1, rs2`
    Rorw { rd: u8, rs1: u8, rs2: u8 },
    /// `orc.b rd, rs1`
    OrcB { rd: u8, rs1: u8 },
    /// `rev8 rd, rs1`
    Rev8 { rd: u8, rs1: u8 },

    // Zbs single-bit operations.
    /// `bset rd, rs1, rs2`
    Bset { rd: u8, rs1: u8, rs2: u8 },
    /// `bseti rd, rs1, shamt`
    Bseti { rd: u8, rs1: u8, shamt: u8 },
    /// `bclr rd, rs1, rs2`
    Bclr { rd: u8, rs1: u8, rs2: u8 },
    /// `bclri rd, rs1, shamt`
    Bclri { rd: u8, rs1: u8, shamt: u8 },
    /// `binv rd, rs1, rs2`
    Binv { rd: u8, rs1: u8, rs2: u8 },
    /// `binvi rd, rs1, shamt`
    Binvi { rd: u8, rs1: u8, shamt: u8 },
    /// `bext rd, rs1, rs2`
    Bext { rd: u8, rs1: u8, rs2: u8 },
    /// `bexti rd, rs1, shamt`
    Bexti { rd: u8, rs1: u8, shamt: u8 },

    // F-extension memory and arithmetic operations.
    /// `flh rd, offset(rs1)`
    Flh { rd: u8, rs1: u8, offset: i64 },
    /// `fsh rs2, offset(rs1)`
    Fsh { rs2: u8, rs1: u8, offset: i64 },
    /// `flw rd, offset(rs1)`
    Flw { rd: u8, rs1: u8, offset: i64 },
    /// `fsw rs2, offset(rs1)`
    Fsw { rs2: u8, rs1: u8, offset: i64 },
    /// `fld rd, offset(rs1)`
    Fld { rd: u8, rs1: u8, offset: i64 },
    /// `fsd rs2, offset(rs1)`
    Fsd { rs2: u8, rs1: u8, offset: i64 },
    /// A binary F operation.
    FpBinary {
        format: FpFormat,
        op: FpBinaryOp,
        rd: u8,
        rs1: u8,
        rs2: u8,
        rounding: u8,
    },
    /// `fsqrt.s rd,rs1`
    FpSqrt {
        format: FpFormat,
        rd: u8,
        rs1: u8,
        rounding: u8,
    },
    /// A fused F multiply-add-family operation.
    FpFused {
        format: FpFormat,
        op: FpFusedOp,
        rd: u8,
        rs1: u8,
        rs2: u8,
        rs3: u8,
        rounding: u8,
    },
    /// A floating-point comparison producing an integer result.
    FpCompare {
        format: FpFormat,
        op: FpCompareOp,
        rd: u8,
        rs1: u8,
        rs2: u8,
    },
    /// `fclass.s rd,rs1`
    FpClass { format: FpFormat, rd: u8, rs1: u8 },
    /// A float-to-integer conversion.
    FpToInt {
        format: FpFormat,
        rd: u8,
        rs1: u8,
        width: u32,
        unsigned: bool,
        rounding: u8,
    },
    /// An integer-to-float conversion.
    IntToFp {
        format: FpFormat,
        rd: u8,
        rs1: u8,
        width: u32,
        unsigned: bool,
        rounding: u8,
    },
    /// Conversion between single and double precision.
    FpConvert {
        from: FpFormat,
        to: FpFormat,
        rd: u8,
        rs1: u8,
        rounding: u8,
    },
    /// `fmv.x.w rd,rs1`
    FpMoveToInt { format: FpFormat, rd: u8, rs1: u8 },
    /// `fmv.w.x rd,rs1`
    FpMoveFromInt { format: FpFormat, rd: u8, rs1: u8 },

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

    // A-extension load-reserved/store-conditional.
    /// `lr.w rd, (rs1)`
    LrW { rd: u8, rs1: u8, aq: bool, rl: bool },
    /// `lr.d rd, (rs1)`
    LrD { rd: u8, rs1: u8, aq: bool, rl: bool },
    /// `sc.w rd, rs2, (rs1)`
    ScW {
        rd: u8,
        rs1: u8,
        rs2: u8,
        aq: bool,
        rl: bool,
    },
    /// `sc.d rd, rs2, (rs1)`
    ScD {
        rd: u8,
        rs1: u8,
        rs2: u8,
        aq: bool,
        rl: bool,
    },

    // A-extension atomic memory operations.
    /// `amoswap.w rd, rs2, (rs1)`
    AmoSwapW {
        rd: u8,
        rs1: u8,
        rs2: u8,
        aq: bool,
        rl: bool,
    },
    /// `amoadd.w rd, rs2, (rs1)`
    AmoAddW {
        rd: u8,
        rs1: u8,
        rs2: u8,
        aq: bool,
        rl: bool,
    },
    /// `amoxor.w rd, rs2, (rs1)`
    AmoXorW {
        rd: u8,
        rs1: u8,
        rs2: u8,
        aq: bool,
        rl: bool,
    },
    /// `amoor.w rd, rs2, (rs1)`
    AmoOrW {
        rd: u8,
        rs1: u8,
        rs2: u8,
        aq: bool,
        rl: bool,
    },
    /// `amoand.w rd, rs2, (rs1)`
    AmoAndW {
        rd: u8,
        rs1: u8,
        rs2: u8,
        aq: bool,
        rl: bool,
    },
    /// `amomin.w rd, rs2, (rs1)`
    AmoMinW {
        rd: u8,
        rs1: u8,
        rs2: u8,
        aq: bool,
        rl: bool,
    },
    /// `amomax.w rd, rs2, (rs1)`
    AmoMaxW {
        rd: u8,
        rs1: u8,
        rs2: u8,
        aq: bool,
        rl: bool,
    },
    /// `amominu.w rd, rs2, (rs1)`
    AmoMinuW {
        rd: u8,
        rs1: u8,
        rs2: u8,
        aq: bool,
        rl: bool,
    },
    /// `amomaxu.w rd, rs2, (rs1)`
    AmoMaxuW {
        rd: u8,
        rs1: u8,
        rs2: u8,
        aq: bool,
        rl: bool,
    },
    /// `amoswap.d rd, rs2, (rs1)`
    AmoSwapD {
        rd: u8,
        rs1: u8,
        rs2: u8,
        aq: bool,
        rl: bool,
    },
    /// `amoadd.d rd, rs2, (rs1)`
    AmoAddD {
        rd: u8,
        rs1: u8,
        rs2: u8,
        aq: bool,
        rl: bool,
    },
    /// `amoxor.d rd, rs2, (rs1)`
    AmoXorD {
        rd: u8,
        rs1: u8,
        rs2: u8,
        aq: bool,
        rl: bool,
    },
    /// `amoor.d rd, rs2, (rs1)`
    AmoOrD {
        rd: u8,
        rs1: u8,
        rs2: u8,
        aq: bool,
        rl: bool,
    },
    /// `amoand.d rd, rs2, (rs1)`
    AmoAndD {
        rd: u8,
        rs1: u8,
        rs2: u8,
        aq: bool,
        rl: bool,
    },
    /// `amomin.d rd, rs2, (rs1)`
    AmoMinD {
        rd: u8,
        rs1: u8,
        rs2: u8,
        aq: bool,
        rl: bool,
    },
    /// `amomax.d rd, rs2, (rs1)`
    AmoMaxD {
        rd: u8,
        rs1: u8,
        rs2: u8,
        aq: bool,
        rl: bool,
    },
    /// `amominu.d rd, rs2, (rs1)`
    AmoMinuD {
        rd: u8,
        rs1: u8,
        rs2: u8,
        aq: bool,
        rl: bool,
    },
    /// `amomaxu.d rd, rs2, (rs1)`
    AmoMaxuD {
        rd: u8,
        rs1: u8,
        rs2: u8,
        aq: bool,
        rl: bool,
    },

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

    // Zicbom, Zicboz, and Zicbop cache-block operations.
    /// `cbo.clean (rs1)`
    CboClean { rs1: u8 },
    /// `cbo.flush (rs1)`
    CboFlush { rs1: u8 },
    /// `cbo.inval (rs1)`
    CboInval { rs1: u8 },
    /// `cbo.zero (rs1)`
    CboZero { rs1: u8 },
    /// `prefetch.i offset(rs1)`
    PrefetchI { rs1: u8, offset: i64 },
    /// `prefetch.r offset(rs1)`
    PrefetchR { rs1: u8, offset: i64 },
    /// `prefetch.w offset(rs1)`
    PrefetchW { rs1: u8, offset: i64 },

    // Zihintpause.
    /// `pause`
    Pause,

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

    // Privileged system instructions.
    /// `ecall`
    Ecall,
    /// `ebreak`
    Ebreak,
    /// `mret`
    Mret,
    /// `sret`
    Sret,
    /// `wfi`
    Wfi,
    /// `sinval.vma rs1,rs2`
    SinvalVma { rs1: u8, rs2: u8 },
    /// `sfence.w.inval`
    SfenceWInval,
    /// `sfence.inval.ir`
    SfenceInvalIr,
    /// `sfence.vma rs1,rs2`
    SfenceVma { rs1: u8, rs2: u8 },

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

/// Floating-point operand format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FpFormat {
    Half,
    Single,
    Double,
}

/// Binary operations encoded by the F opcode space.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FpBinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    SignInject,
    SignInjectNegated,
    SignInjectXor,
    Min,
    Max,
}

/// Fused multiply-add-family operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FpFusedOp {
    Madd,
    Msub,
    Nmsub,
    Nmadd,
}

/// Floating-point comparison operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FpCompareOp {
    Equal,
    Less,
    LessEqual,
}

/// A decoded instruction plus the encoding that produced it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DecodedInstruction {
    instruction: Instruction,
    encoding: Encoding,
}

/// Original instruction length and bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Encoding {
    Compressed(CompressedInstruction),
    Standard(u32),
}

impl DecodedInstruction {
    /// Wraps an already-decoded standard instruction.
    pub(crate) fn standard(instruction: Instruction, word: u32) -> Self {
        Self {
            instruction,
            encoding: Encoding::Standard(word),
        }
    }

    /// The base instruction whose execution semantics this encoding uses.
    pub(crate) fn instruction(self) -> Instruction {
        self.instruction
    }

    /// Instruction length in bytes.
    pub(crate) fn length(self) -> u64 {
        match self.encoding {
            Encoding::Compressed(_) => COMPRESSED_INSTRUCTION_BYTES,
            Encoding::Standard(_) => STANDARD_INSTRUCTION_BYTES,
        }
    }

    /// Original encoding, zero-extended for a compressed instruction.
    pub(crate) fn raw(self) -> u32 {
        match self.encoding {
            Encoding::Compressed(instruction) => u32::from(instruction.word()),
            Encoding::Standard(word) => word,
        }
    }

    /// Whether this is a standard 32-bit `jal x0, 0`, the CLI halt convention.
    pub(crate) fn is_standard_jal_self_loop(self) -> bool {
        matches!(self.encoding, Encoding::Standard(_))
            && self.instruction
                == (Instruction::Jal {
                    rd: ZERO_REGISTER,
                    offset: ZERO_OFFSET,
                })
    }

    /// Renders the original compressed mnemonic and operands for a trace.
    pub(crate) fn render(self) -> String {
        match self.encoding {
            Encoding::Compressed(instruction) => instruction.render(),
            Encoding::Standard(_) => self.instruction.render(),
        }
    }
}

/// The integer compressed instructions in Zca, with their decoded operands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CompressedInstruction {
    Addi4spn {
        rd: u8,
        imm: i64,
        word: u16,
    },
    Lw {
        rd: u8,
        rs1: u8,
        offset: i64,
        word: u16,
    },
    Ld {
        rd: u8,
        rs1: u8,
        offset: i64,
        word: u16,
    },
    Fld {
        rd: u8,
        rs1: u8,
        offset: i64,
        word: u16,
    },
    Sw {
        rs2: u8,
        rs1: u8,
        offset: i64,
        word: u16,
    },
    Sd {
        rs2: u8,
        rs1: u8,
        offset: i64,
        word: u16,
    },
    Fsd {
        rs2: u8,
        rs1: u8,
        offset: i64,
        word: u16,
    },
    Addi {
        rd: u8,
        imm: i64,
        word: u16,
    },
    Nop {
        imm: i64,
        word: u16,
    },
    Addiw {
        rd: u8,
        imm: i64,
        word: u16,
    },
    Li {
        rd: u8,
        imm: i64,
        word: u16,
    },
    Lui {
        rd: u8,
        imm: i64,
        word: u16,
    },
    Addi16sp {
        imm: i64,
        word: u16,
    },
    Srli {
        rd: u8,
        shamt: u8,
        word: u16,
    },
    Srai {
        rd: u8,
        shamt: u8,
        word: u16,
    },
    Andi {
        rd: u8,
        imm: i64,
        word: u16,
    },
    Sub {
        rd: u8,
        rs2: u8,
        word: u16,
    },
    Xor {
        rd: u8,
        rs2: u8,
        word: u16,
    },
    Or {
        rd: u8,
        rs2: u8,
        word: u16,
    },
    And {
        rd: u8,
        rs2: u8,
        word: u16,
    },
    Subw {
        rd: u8,
        rs2: u8,
        word: u16,
    },
    Addw {
        rd: u8,
        rs2: u8,
        word: u16,
    },
    J {
        offset: i64,
        word: u16,
    },
    Beqz {
        rs1: u8,
        offset: i64,
        word: u16,
    },
    Bnez {
        rs1: u8,
        offset: i64,
        word: u16,
    },
    Slli {
        rd: u8,
        shamt: u8,
        word: u16,
    },
    Lwsp {
        rd: u8,
        offset: i64,
        word: u16,
    },
    Ldsp {
        rd: u8,
        offset: i64,
        word: u16,
    },
    Fldsp {
        rd: u8,
        offset: i64,
        word: u16,
    },
    Jr {
        rs1: u8,
        word: u16,
    },
    Mv {
        rd: u8,
        rs2: u8,
        word: u16,
    },
    Ebreak {
        word: u16,
    },
    Jalr {
        rs1: u8,
        word: u16,
    },
    Add {
        rd: u8,
        rs2: u8,
        word: u16,
    },
    Swsp {
        rs2: u8,
        offset: i64,
        word: u16,
    },
    Sdsp {
        rs2: u8,
        offset: i64,
        word: u16,
    },
    Fsdsp {
        rs2: u8,
        offset: i64,
        word: u16,
    },
}

impl CompressedInstruction {
    fn word(self) -> u16 {
        match self {
            Self::Addi4spn { word, .. }
            | Self::Lw { word, .. }
            | Self::Ld { word, .. }
            | Self::Fld { word, .. }
            | Self::Sw { word, .. }
            | Self::Sd { word, .. }
            | Self::Fsd { word, .. }
            | Self::Addi { word, .. }
            | Self::Nop { word, .. }
            | Self::Addiw { word, .. }
            | Self::Li { word, .. }
            | Self::Lui { word, .. }
            | Self::Addi16sp { word, .. }
            | Self::Srli { word, .. }
            | Self::Srai { word, .. }
            | Self::Andi { word, .. }
            | Self::Sub { word, .. }
            | Self::Xor { word, .. }
            | Self::Or { word, .. }
            | Self::And { word, .. }
            | Self::Subw { word, .. }
            | Self::Addw { word, .. }
            | Self::J { word, .. }
            | Self::Beqz { word, .. }
            | Self::Bnez { word, .. }
            | Self::Slli { word, .. }
            | Self::Lwsp { word, .. }
            | Self::Ldsp { word, .. }
            | Self::Fldsp { word, .. }
            | Self::Jr { word, .. }
            | Self::Mv { word, .. }
            | Self::Ebreak { word }
            | Self::Jalr { word, .. }
            | Self::Add { word, .. }
            | Self::Swsp { word, .. }
            | Self::Sdsp { word, .. }
            | Self::Fsdsp { word, .. } => word,
        }
    }

    fn decode(word: u16) -> Result<Self, Trap> {
        let quadrant = word & MINOR_OPCODE_MASK;
        let funct3 =
            CompressedWord::new(word).bits(COMPRESSED_FUNCT3_SHIFT, COMPRESSED_FUNCT3_BITS) as u8;

        match (quadrant, funct3) {
            (0b00, C0_ADDI4SPN) => {
                let imm = CompressedWord::new(word).addi4spn_immediate();
                if imm == ZERO_IMMEDIATE {
                    return Err(Trap::IllegalInstruction);
                }
                Ok(Self::Addi4spn {
                    rd: CompressedWord::new(word).rd_prime(),
                    imm,
                    word,
                })
            }
            (0b00, C0_FLD) => {
                let word_fields = CompressedWord::new(word);
                Ok(Self::Fld {
                    rd: word_fields.rd_prime(),
                    rs1: word_fields.rs1_prime(),
                    offset: word_fields.ld_offset(),
                    word,
                })
            }
            (0b00, C0_LW) => {
                let word_fields = CompressedWord::new(word);
                Ok(Self::Lw {
                    rd: word_fields.rd_prime(),
                    rs1: word_fields.rs1_prime(),
                    offset: word_fields.lw_offset(),
                    word,
                })
            }
            (0b00, C0_LD) => {
                let word_fields = CompressedWord::new(word);
                Ok(Self::Ld {
                    rd: word_fields.rd_prime(),
                    rs1: word_fields.rs1_prime(),
                    offset: word_fields.ld_offset(),
                    word,
                })
            }
            (0b00, C0_SW) => {
                let word_fields = CompressedWord::new(word);
                Ok(Self::Sw {
                    rs2: word_fields.rd_prime(),
                    rs1: word_fields.rs1_prime(),
                    offset: word_fields.lw_offset(),
                    word,
                })
            }
            (0b00, C0_SD) => {
                let word_fields = CompressedWord::new(word);
                Ok(Self::Sd {
                    rs2: word_fields.rd_prime(),
                    rs1: word_fields.rs1_prime(),
                    offset: word_fields.ld_offset(),
                    word,
                })
            }
            (0b00, C0_FSD) => {
                let word_fields = CompressedWord::new(word);
                Ok(Self::Fsd {
                    rs2: word_fields.rd_prime(),
                    rs1: word_fields.rs1_prime(),
                    offset: word_fields.ld_offset(),
                    word,
                })
            }
            (0b01, C1_ADDI) => {
                let word_fields = CompressedWord::new(word);
                let rd = word_fields.rd();
                let imm = word_fields.ci_immediate();
                if rd == ZERO_REGISTER {
                    Ok(Self::Nop { imm, word })
                } else {
                    Ok(Self::Addi { rd, imm, word })
                }
            }
            (0b01, C1_ADDIW) => {
                let word_fields = CompressedWord::new(word);
                let rd = word_fields.rd();
                if rd == ZERO_REGISTER {
                    return Err(Trap::IllegalInstruction);
                }
                Ok(Self::Addiw {
                    rd,
                    imm: word_fields.ci_immediate(),
                    word,
                })
            }
            (0b01, C1_LI) => {
                let word_fields = CompressedWord::new(word);
                Ok(Self::Li {
                    rd: word_fields.rd(),
                    imm: word_fields.ci_immediate(),
                    word,
                })
            }
            (0b01, C1_LUI_OR_ADDI16SP) => {
                let word_fields = CompressedWord::new(word);
                let rd = word_fields.rd();
                if rd == SP {
                    let imm = word_fields.addi16sp_immediate();
                    if imm == ZERO_IMMEDIATE {
                        return Err(Trap::IllegalInstruction);
                    }
                    Ok(Self::Addi16sp { imm, word })
                } else {
                    let imm = word_fields.lui_immediate();
                    if imm == ZERO_IMMEDIATE {
                        return Err(Trap::IllegalInstruction);
                    }
                    Ok(Self::Lui { rd, imm, word })
                }
            }
            (0b01, C1_ALU) => {
                let word_fields = CompressedWord::new(word);
                let rd = word_fields.rs1_prime();
                let register_width = word_fields.bits(
                    COMPRESSED_ALU_WIDTH_FLAG_SHIFT,
                    COMPRESSED_ALU_WIDTH_FLAG_BITS,
                );
                match word_fields.bits(COMPRESSED_ALU_FUNCT2_SHIFT, COMPRESSED_ALU_FUNCT2_BITS) {
                    C_ALU_SRLI => Ok(Self::Srli {
                        rd,
                        shamt: word_fields.shamt(),
                        word,
                    }),
                    C_ALU_SRAI => Ok(Self::Srai {
                        rd,
                        shamt: word_fields.shamt(),
                        word,
                    }),
                    C_ALU_ANDI => Ok(Self::Andi {
                        rd,
                        imm: word_fields.ci_immediate(),
                        word,
                    }),
                    C_ALU_REGISTER if register_width == C_ALU_REGISTER_BASE => {
                        let rs2 = word_fields.rd_prime();
                        match word_fields.bits(COMPRESSED_ALU_RS2_SHIFT, COMPRESSED_ALU_RS2_BITS) {
                            C_ALU_SUB => Ok(Self::Sub { rd, rs2, word }),
                            C_ALU_XOR => Ok(Self::Xor { rd, rs2, word }),
                            C_ALU_OR => Ok(Self::Or { rd, rs2, word }),
                            C_ALU_AND => Ok(Self::And { rd, rs2, word }),
                            _ => unreachable!(),
                        }
                    }
                    C_ALU_REGISTER if register_width == C_ALU_REGISTER_WORD => {
                        let rs2 = word_fields.rd_prime();
                        match word_fields.bits(COMPRESSED_ALU_RS2_SHIFT, COMPRESSED_ALU_RS2_BITS) {
                            C_ALU_SUBW => Ok(Self::Subw { rd, rs2, word }),
                            C_ALU_ADDW => Ok(Self::Addw { rd, rs2, word }),
                            _ => Err(Trap::IllegalInstruction),
                        }
                    }
                    _ => unreachable!(),
                }
            }
            (0b01, C1_J) => Ok(Self::J {
                offset: CompressedWord::new(word).jump_offset(),
                word,
            }),
            (0b01, C1_BEQZ) => {
                let word_fields = CompressedWord::new(word);
                Ok(Self::Beqz {
                    rs1: word_fields.rs1_prime(),
                    offset: word_fields.branch_offset(),
                    word,
                })
            }
            (0b01, C1_BNEZ) => {
                let word_fields = CompressedWord::new(word);
                Ok(Self::Bnez {
                    rs1: word_fields.rs1_prime(),
                    offset: word_fields.branch_offset(),
                    word,
                })
            }
            (0b10, C2_SLLI) => {
                let word_fields = CompressedWord::new(word);
                Ok(Self::Slli {
                    rd: word_fields.rd(),
                    shamt: word_fields.shamt(),
                    word,
                })
            }
            (0b10, C2_FLDSP) => {
                let word_fields = CompressedWord::new(word);
                Ok(Self::Fldsp {
                    rd: word_fields.rd(),
                    offset: word_fields.ldsp_offset(),
                    word,
                })
            }
            (0b10, C2_LWSP) => {
                let word_fields = CompressedWord::new(word);
                let rd = word_fields.rd();
                if rd == ZERO_REGISTER {
                    return Err(Trap::IllegalInstruction);
                }
                Ok(Self::Lwsp {
                    rd,
                    offset: word_fields.lwsp_offset(),
                    word,
                })
            }
            (0b10, C2_LDSP) => {
                let word_fields = CompressedWord::new(word);
                let rd = word_fields.rd();
                if rd == ZERO_REGISTER {
                    return Err(Trap::IllegalInstruction);
                }
                Ok(Self::Ldsp {
                    rd,
                    offset: word_fields.ldsp_offset(),
                    word,
                })
            }
            (0b10, C2_MISC) => {
                let word_fields = CompressedWord::new(word);
                let rd = word_fields.rd();
                let rs2 = word_fields.rs2();
                match (
                    word_fields.bits(
                        COMPRESSED_MISC_WIDTH_FLAG_SHIFT,
                        COMPRESSED_MISC_WIDTH_FLAG_BITS,
                    ),
                    rd,
                    rs2,
                ) {
                    (C_MISC_JR_OR_MV, C_MISC_EMPTY_REGISTER, C_MISC_EMPTY_REGISTER) => {
                        Err(Trap::IllegalInstruction)
                    }
                    (C_MISC_JR_OR_MV, _, C_MISC_EMPTY_REGISTER) => Ok(Self::Jr { rs1: rd, word }),
                    (C_MISC_JR_OR_MV, _, _) => Ok(Self::Mv { rd, rs2, word }),
                    (C_MISC_EBREAK_OR_JALR, C_MISC_EMPTY_REGISTER, C_MISC_EMPTY_REGISTER) => {
                        Ok(Self::Ebreak { word })
                    }
                    (C_MISC_EBREAK_OR_JALR, _, C_MISC_EMPTY_REGISTER) => {
                        Ok(Self::Jalr { rs1: rd, word })
                    }
                    (C_MISC_EBREAK_OR_JALR, _, _) => Ok(Self::Add { rd, rs2, word }),
                    _ => unreachable!(),
                }
            }
            (0b10, C2_SWSP) => {
                let word_fields = CompressedWord::new(word);
                Ok(Self::Swsp {
                    rs2: word_fields.rs2(),
                    offset: word_fields.swsp_offset(),
                    word,
                })
            }
            (0b10, C2_SDSP) => {
                let word_fields = CompressedWord::new(word);
                Ok(Self::Sdsp {
                    rs2: word_fields.rs2(),
                    offset: word_fields.sdsp_offset(),
                    word,
                })
            }
            (0b10, C2_FSDSP) => {
                let word_fields = CompressedWord::new(word);
                Ok(Self::Fsdsp {
                    rs2: word_fields.rs2(),
                    offset: word_fields.sdsp_offset(),
                    word,
                })
            }
            _ => Err(Trap::IllegalInstruction),
        }
    }

    /// Expands this compressed form into its canonical standard encoding.
    fn decompress(self) -> u32 {
        match self {
            Self::Addi4spn { rd, imm, .. } => encode_i(inst::MATCH_ADDI, rd, SP, imm),
            Self::Lw {
                rd, rs1, offset, ..
            } => encode_i(inst::MATCH_LW, rd, rs1, offset),
            Self::Ld {
                rd, rs1, offset, ..
            } => encode_i(inst::MATCH_LD, rd, rs1, offset),
            Self::Fld {
                rd, rs1, offset, ..
            } => encode_i(inst::MATCH_FLD, rd, rs1, offset),
            Self::Sw {
                rs2, rs1, offset, ..
            } => {
                encode_immediate(inst::MATCH_SW, offset, S_IMMEDIATE_FIELDS)
                    | store_register_fields(rs1, rs2)
            }
            Self::Sd {
                rs2, rs1, offset, ..
            } => {
                encode_immediate(inst::MATCH_SD, offset, S_IMMEDIATE_FIELDS)
                    | store_register_fields(rs1, rs2)
            }
            Self::Fsd {
                rs2, rs1, offset, ..
            } => {
                encode_immediate(inst::MATCH_FSD, offset, S_IMMEDIATE_FIELDS)
                    | store_register_fields(rs1, rs2)
            }
            Self::Addi { rd, imm, .. } => encode_i(inst::MATCH_ADDI, rd, rd, imm),
            Self::Nop { .. } => encode_nop(),
            Self::Addiw { rd, imm, .. } => encode_i(inst::MATCH_ADDIW, rd, rd, imm),
            Self::Li { rd, imm, .. } => encode_i(inst::MATCH_ADDI, rd, ZERO_REGISTER, imm),
            Self::Lui { rd, imm, .. } => encode_u(inst::MATCH_LUI, rd, imm),
            Self::Addi16sp { imm, .. } => encode_i(inst::MATCH_ADDI, SP, SP, imm),
            Self::Srli { rd, shamt, .. } => encode_i(inst::MATCH_SRLI, rd, rd, i64::from(shamt)),
            Self::Srai { rd, shamt, .. } => encode_i(inst::MATCH_SRAI, rd, rd, i64::from(shamt)),
            Self::Andi { rd, imm, .. } => encode_i(inst::MATCH_ANDI, rd, rd, imm),
            Self::Sub { rd, rs2, .. } => encode_r(inst::MATCH_SUB, rd, rd, rs2),
            Self::Xor { rd, rs2, .. } => encode_r(inst::MATCH_XOR, rd, rd, rs2),
            Self::Or { rd, rs2, .. } => encode_r(inst::MATCH_OR, rd, rd, rs2),
            Self::And { rd, rs2, .. } => encode_r(inst::MATCH_AND, rd, rd, rs2),
            Self::Subw { rd, rs2, .. } => encode_r(inst::MATCH_SUBW, rd, rd, rs2),
            Self::Addw { rd, rs2, .. } => encode_r(inst::MATCH_ADDW, rd, rd, rs2),
            Self::J { offset, .. } => {
                encode_immediate(inst::MATCH_JAL, offset, STANDARD_J_IMMEDIATE_FIELDS)
            }
            Self::Beqz { rs1, offset, .. } => {
                encode_immediate(inst::MATCH_BEQ, offset, STANDARD_B_IMMEDIATE_FIELDS)
                    | (u32::from(rs1) << STANDARD_RS1_SHIFT)
            }
            Self::Bnez { rs1, offset, .. } => {
                encode_immediate(inst::MATCH_BNE, offset, STANDARD_B_IMMEDIATE_FIELDS)
                    | (u32::from(rs1) << STANDARD_RS1_SHIFT)
            }
            Self::Slli { rd, shamt, .. } => encode_i(inst::MATCH_SLLI, rd, rd, i64::from(shamt)),
            Self::Lwsp { rd, offset, .. } => encode_i(inst::MATCH_LW, rd, SP, offset),
            Self::Ldsp { rd, offset, .. } => encode_i(inst::MATCH_LD, rd, SP, offset),
            Self::Fldsp { rd, offset, .. } => encode_i(inst::MATCH_FLD, rd, SP, offset),
            Self::Jr { rs1, .. } => encode_i(inst::MATCH_JALR, ZERO_REGISTER, rs1, ZERO_OFFSET),
            Self::Mv { rd, rs2, .. } => encode_r(inst::MATCH_ADD, rd, ZERO_REGISTER, rs2),
            Self::Ebreak { .. } => inst::MATCH_EBREAK,
            Self::Jalr { rs1, .. } => encode_i(inst::MATCH_JALR, RA, rs1, ZERO_OFFSET),
            Self::Add { rd, rs2, .. } => encode_r(inst::MATCH_ADD, rd, rd, rs2),
            Self::Swsp { rs2, offset, .. } => {
                encode_immediate(inst::MATCH_SW, offset, S_IMMEDIATE_FIELDS)
                    | (u32::from(SP) << STANDARD_RS1_SHIFT)
                    | (u32::from(rs2) << STANDARD_RS2_SHIFT)
            }
            Self::Sdsp { rs2, offset, .. } => {
                encode_immediate(inst::MATCH_SD, offset, S_IMMEDIATE_FIELDS)
                    | (u32::from(SP) << STANDARD_RS1_SHIFT)
                    | (u32::from(rs2) << STANDARD_RS2_SHIFT)
            }
            Self::Fsdsp { rs2, offset, .. } => {
                encode_immediate(inst::MATCH_FSD, offset, S_IMMEDIATE_FIELDS)
                    | (u32::from(SP) << STANDARD_RS1_SHIFT)
                    | (u32::from(rs2) << STANDARD_RS2_SHIFT)
            }
        }
    }

    fn render(self) -> String {
        match self {
            Self::Addi4spn { rd, imm, .. } => {
                format!("c.addi4spn {},{},{imm}", abi_name(rd), abi_name(SP))
            }
            Self::Lw {
                rd, rs1, offset, ..
            } => {
                format!("c.lw {},{offset}({})", abi_name(rd), abi_name(rs1))
            }
            Self::Ld {
                rd, rs1, offset, ..
            } => {
                format!("c.ld {},{offset}({})", abi_name(rd), abi_name(rs1))
            }
            Self::Fld {
                rd, rs1, offset, ..
            } => {
                format!("c.fld {},{offset}({})", fpr_abi_name(rd), abi_name(rs1))
            }
            Self::Sw {
                rs2, rs1, offset, ..
            } => format!("c.sw {},{offset}({})", abi_name(rs2), abi_name(rs1)),
            Self::Sd {
                rs2, rs1, offset, ..
            } => format!("c.sd {},{offset}({})", abi_name(rs2), abi_name(rs1)),
            Self::Fsd {
                rs2, rs1, offset, ..
            } => format!("c.fsd {},{offset}({})", fpr_abi_name(rs2), abi_name(rs1)),
            Self::Addi { rd, imm, .. } => format!("c.addi {},{imm}", abi_name(rd)),
            Self::Nop { imm, .. } => format!("c.addi zero,{imm}"),
            Self::Addiw { rd, imm, .. } => format!("c.addiw {},{imm}", abi_name(rd)),
            Self::Li { rd, imm, .. } => format!("c.li {},{imm}", abi_name(rd)),
            Self::Lui { rd, imm, .. } => {
                format!("c.lui {},{:#x}", abi_name(rd), u_field(imm))
            }
            Self::Addi16sp { imm, .. } => {
                format!("c.addi16sp {},{imm}", abi_name(SP))
            }
            Self::Srli { rd, shamt, .. } => format!("c.srli {},{shamt:#x}", abi_name(rd)),
            Self::Srai { rd, shamt, .. } => format!("c.srai {},{shamt:#x}", abi_name(rd)),
            Self::Andi { rd, imm, .. } => format!("c.andi {},{imm}", abi_name(rd)),
            Self::Sub { rd, rs2, .. } => format!("c.sub {},{}", abi_name(rd), abi_name(rs2)),
            Self::Xor { rd, rs2, .. } => format!("c.xor {},{}", abi_name(rd), abi_name(rs2)),
            Self::Or { rd, rs2, .. } => format!("c.or {},{}", abi_name(rd), abi_name(rs2)),
            Self::And { rd, rs2, .. } => format!("c.and {},{}", abi_name(rd), abi_name(rs2)),
            Self::Subw { rd, rs2, .. } => format!("c.subw {},{}", abi_name(rd), abi_name(rs2)),
            Self::Addw { rd, rs2, .. } => format!("c.addw {},{}", abi_name(rd), abi_name(rs2)),
            Self::J { offset, .. } => format!("c.j {}", relative(offset)),
            Self::Beqz { rs1, offset, .. } => {
                format!("c.beqz {},{}", abi_name(rs1), relative(offset))
            }
            Self::Bnez { rs1, offset, .. } => {
                format!("c.bnez {},{}", abi_name(rs1), relative(offset))
            }
            Self::Slli { rd, shamt, .. } => format!("c.slli {},{shamt:#x}", abi_name(rd)),
            Self::Lwsp { rd, offset, .. } => format!("c.lwsp {},{offset}(sp)", abi_name(rd)),
            Self::Ldsp { rd, offset, .. } => format!("c.ldsp {},{offset}(sp)", abi_name(rd)),
            Self::Fldsp { rd, offset, .. } => {
                format!("c.fldsp {},{offset}(sp)", fpr_abi_name(rd))
            }
            Self::Jr { rs1, .. } => format!("c.jr {}", abi_name(rs1)),
            Self::Mv { rd, rs2, .. } => format!("c.mv {},{}", abi_name(rd), abi_name(rs2)),
            Self::Ebreak { .. } => "c.ebreak".to_owned(),
            Self::Jalr { rs1, .. } => format!("c.jalr {}", abi_name(rs1)),
            Self::Add { rd, rs2, .. } => format!("c.add {},{}", abi_name(rd), abi_name(rs2)),
            Self::Swsp { rs2, offset, .. } => {
                format!("c.swsp {},{offset}(sp)", abi_name(rs2))
            }
            Self::Sdsp { rs2, offset, .. } => {
                format!("c.sdsp {},{offset}(sp)", abi_name(rs2))
            }
            Self::Fsdsp { rs2, offset, .. } => {
                format!("c.fsdsp {},{offset}(sp)", fpr_abi_name(rs2))
            }
        }
    }
}

/// A 16-bit compressed instruction word.
#[derive(Clone, Copy)]
struct CompressedWord(u16);

impl CompressedWord {
    const fn new(word: u16) -> Self {
        Self(word)
    }

    fn bits(self, shift: u32, width: u32) -> u16 {
        (self.0 >> shift) & mask_u16(width)
    }

    fn gather(self, fields: &[Field]) -> u16 {
        fields.iter().fold(0, |value, field| {
            value | (self.bits(field.source_shift, field.width) << field.target_shift)
        })
    }

    fn rd(self) -> u8 {
        self.bits(COMPRESSED_RD_SHIFT, REGISTER_BITS) as u8
    }

    fn rs2(self) -> u8 {
        self.bits(COMPRESSED_RS2_SHIFT, REGISTER_BITS) as u8
    }

    fn rd_prime(self) -> u8 {
        PRIME_REGISTER_BASE + self.bits(PRIME_RD_SHIFT, PRIME_REGISTER_BITS) as u8
    }

    fn rs1_prime(self) -> u8 {
        PRIME_REGISTER_BASE + self.bits(PRIME_RS1_SHIFT, PRIME_REGISTER_BITS) as u8
    }

    fn signed_immediate(self, value: u16, width: u32) -> i64 {
        sign_extend(u32::from(value), width)
    }

    fn ci_immediate(self) -> i64 {
        self.signed_immediate(self.gather(CI_IMMEDIATE_FIELDS), CI_IMMEDIATE_BITS)
    }

    fn shamt(self) -> u8 {
        self.gather(CI_IMMEDIATE_FIELDS) as u8
    }

    fn addi4spn_immediate(self) -> i64 {
        i64::from(self.gather(ADDI4SPN_IMMEDIATE_FIELDS))
    }

    fn lui_immediate(self) -> i64 {
        self.ci_immediate() << U_IMMEDIATE_SHIFT
    }

    fn addi16sp_immediate(self) -> i64 {
        self.signed_immediate(
            self.gather(ADDI16SP_IMMEDIATE_FIELDS),
            ADDI16SP_IMMEDIATE_BITS,
        )
    }

    fn lw_offset(self) -> i64 {
        i64::from(self.gather(LW_IMMEDIATE_FIELDS))
    }

    fn ld_offset(self) -> i64 {
        i64::from(self.gather(LD_IMMEDIATE_FIELDS))
    }

    fn jump_offset(self) -> i64 {
        self.signed_immediate(self.gather(J_IMMEDIATE_FIELDS), JUMP_IMMEDIATE_BITS)
    }

    fn branch_offset(self) -> i64 {
        self.signed_immediate(self.gather(B_IMMEDIATE_FIELDS), BRANCH_IMMEDIATE_BITS)
    }

    fn lwsp_offset(self) -> i64 {
        i64::from(self.gather(LWSP_IMMEDIATE_FIELDS))
    }

    fn ldsp_offset(self) -> i64 {
        i64::from(self.gather(LDSP_IMMEDIATE_FIELDS))
    }

    fn swsp_offset(self) -> i64 {
        i64::from(self.gather(SWSP_IMMEDIATE_FIELDS))
    }

    fn sdsp_offset(self) -> i64 {
        i64::from(self.gather(SDSP_IMMEDIATE_FIELDS))
    }
}

impl Instruction {
    /// Mnemonic spelled the way `objdump -M no-aliases` spells it.
    pub fn mnemonic(&self) -> Cow<'static, str> {
        let base = self.base_mnemonic();
        match self.atomic_ordering() {
            Some((aq, rl)) => Cow::Owned(format!("{base}{}", atomic_suffix(aq, rl))),
            None => Cow::Borrowed(base),
        }
    }

    fn atomic_ordering(&self) -> Option<(bool, bool)> {
        match *self {
            Instruction::LrW { aq, rl, .. }
            | Instruction::LrD { aq, rl, .. }
            | Instruction::ScW { aq, rl, .. }
            | Instruction::ScD { aq, rl, .. }
            | Instruction::AmoSwapW { aq, rl, .. }
            | Instruction::AmoAddW { aq, rl, .. }
            | Instruction::AmoXorW { aq, rl, .. }
            | Instruction::AmoOrW { aq, rl, .. }
            | Instruction::AmoAndW { aq, rl, .. }
            | Instruction::AmoMinW { aq, rl, .. }
            | Instruction::AmoMaxW { aq, rl, .. }
            | Instruction::AmoMinuW { aq, rl, .. }
            | Instruction::AmoMaxuW { aq, rl, .. }
            | Instruction::AmoSwapD { aq, rl, .. }
            | Instruction::AmoAddD { aq, rl, .. }
            | Instruction::AmoXorD { aq, rl, .. }
            | Instruction::AmoOrD { aq, rl, .. }
            | Instruction::AmoAndD { aq, rl, .. }
            | Instruction::AmoMinD { aq, rl, .. }
            | Instruction::AmoMaxD { aq, rl, .. }
            | Instruction::AmoMinuD { aq, rl, .. }
            | Instruction::AmoMaxuD { aq, rl, .. } => Some((aq, rl)),
            _ => None,
        }
    }

    fn base_mnemonic(&self) -> &'static str {
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
            Instruction::AddUw { .. } => "add.uw",
            Instruction::Sh1Add { .. } => "sh1add",
            Instruction::Sh1AddUw { .. } => "sh1add.uw",
            Instruction::Sh2Add { .. } => "sh2add",
            Instruction::Sh2AddUw { .. } => "sh2add.uw",
            Instruction::Sh3Add { .. } => "sh3add",
            Instruction::Sh3AddUw { .. } => "sh3add.uw",
            Instruction::SlliUw { .. } => "slli.uw",
            Instruction::Andn { .. } => "andn",
            Instruction::Orn { .. } => "orn",
            Instruction::Xnor { .. } => "xnor",
            Instruction::Clz { .. } => "clz",
            Instruction::Clzw { .. } => "clzw",
            Instruction::Ctz { .. } => "ctz",
            Instruction::Ctzw { .. } => "ctzw",
            Instruction::Cpop { .. } => "cpop",
            Instruction::Cpopw { .. } => "cpopw",
            Instruction::Max { .. } => "max",
            Instruction::Maxu { .. } => "maxu",
            Instruction::Min { .. } => "min",
            Instruction::Minu { .. } => "minu",
            Instruction::SextB { .. } => "sext.b",
            Instruction::SextH { .. } => "sext.h",
            Instruction::ZextH { .. } => "zext.h",
            Instruction::Rol { .. } => "rol",
            Instruction::Rolw { .. } => "rolw",
            Instruction::Ror { .. } => "ror",
            Instruction::Rori { .. } => "rori",
            Instruction::Roriw { .. } => "roriw",
            Instruction::Rorw { .. } => "rorw",
            Instruction::OrcB { .. } => "orc.b",
            Instruction::Rev8 { .. } => "rev8",
            Instruction::Bset { .. } => "bset",
            Instruction::Bseti { .. } => "bseti",
            Instruction::Bclr { .. } => "bclr",
            Instruction::Bclri { .. } => "bclri",
            Instruction::Binv { .. } => "binv",
            Instruction::Binvi { .. } => "binvi",
            Instruction::Bext { .. } => "bext",
            Instruction::Bexti { .. } => "bexti",
            Instruction::Flh { .. } => "flh",
            Instruction::Fsh { .. } => "fsh",
            Instruction::Flw { .. } => "flw",
            Instruction::Fsw { .. } => "fsw",
            Instruction::Fld { .. } => "fld",
            Instruction::Fsd { .. } => "fsd",
            Instruction::FpBinary { op, format, .. } => match (op, format) {
                (FpBinaryOp::Add, FpFormat::Half) => "fadd.h",
                (FpBinaryOp::Sub, FpFormat::Half) => "fsub.h",
                (FpBinaryOp::Mul, FpFormat::Half) => "fmul.h",
                (FpBinaryOp::Div, FpFormat::Half) => "fdiv.h",
                (FpBinaryOp::SignInject, FpFormat::Half) => "fsgnj.h",
                (FpBinaryOp::SignInjectNegated, FpFormat::Half) => "fsgnjn.h",
                (FpBinaryOp::SignInjectXor, FpFormat::Half) => "fsgnjx.h",
                (FpBinaryOp::Min, FpFormat::Half) => "fmin.h",
                (FpBinaryOp::Max, FpFormat::Half) => "fmax.h",
                (FpBinaryOp::Add, FpFormat::Single) => "fadd.s",
                (FpBinaryOp::Sub, FpFormat::Single) => "fsub.s",
                (FpBinaryOp::Mul, FpFormat::Single) => "fmul.s",
                (FpBinaryOp::Div, FpFormat::Single) => "fdiv.s",
                (FpBinaryOp::SignInject, FpFormat::Single) => "fsgnj.s",
                (FpBinaryOp::SignInjectNegated, FpFormat::Single) => "fsgnjn.s",
                (FpBinaryOp::SignInjectXor, FpFormat::Single) => "fsgnjx.s",
                (FpBinaryOp::Min, FpFormat::Single) => "fmin.s",
                (FpBinaryOp::Max, FpFormat::Single) => "fmax.s",
                (FpBinaryOp::Add, FpFormat::Double) => "fadd.d",
                (FpBinaryOp::Sub, FpFormat::Double) => "fsub.d",
                (FpBinaryOp::Mul, FpFormat::Double) => "fmul.d",
                (FpBinaryOp::Div, FpFormat::Double) => "fdiv.d",
                (FpBinaryOp::SignInject, FpFormat::Double) => "fsgnj.d",
                (FpBinaryOp::SignInjectNegated, FpFormat::Double) => "fsgnjn.d",
                (FpBinaryOp::SignInjectXor, FpFormat::Double) => "fsgnjx.d",
                (FpBinaryOp::Min, FpFormat::Double) => "fmin.d",
                (FpBinaryOp::Max, FpFormat::Double) => "fmax.d",
            },
            Instruction::FpSqrt { format, .. } => match format {
                FpFormat::Half => "fsqrt.h",
                FpFormat::Single => "fsqrt.s",
                FpFormat::Double => "fsqrt.d",
            },
            Instruction::FpFused { op, format, .. } => match (op, format) {
                (FpFusedOp::Madd, FpFormat::Half) => "fmadd.h",
                (FpFusedOp::Msub, FpFormat::Half) => "fmsub.h",
                (FpFusedOp::Nmsub, FpFormat::Half) => "fnmsub.h",
                (FpFusedOp::Nmadd, FpFormat::Half) => "fnmadd.h",
                (FpFusedOp::Madd, FpFormat::Single) => "fmadd.s",
                (FpFusedOp::Msub, FpFormat::Single) => "fmsub.s",
                (FpFusedOp::Nmsub, FpFormat::Single) => "fnmsub.s",
                (FpFusedOp::Nmadd, FpFormat::Single) => "fnmadd.s",
                (FpFusedOp::Madd, FpFormat::Double) => "fmadd.d",
                (FpFusedOp::Msub, FpFormat::Double) => "fmsub.d",
                (FpFusedOp::Nmsub, FpFormat::Double) => "fnmsub.d",
                (FpFusedOp::Nmadd, FpFormat::Double) => "fnmadd.d",
            },
            Instruction::FpCompare { op, format, .. } => match (op, format) {
                (FpCompareOp::Equal, FpFormat::Half) => "feq.h",
                (FpCompareOp::Less, FpFormat::Half) => "flt.h",
                (FpCompareOp::LessEqual, FpFormat::Half) => "fle.h",
                (FpCompareOp::Equal, FpFormat::Single) => "feq.s",
                (FpCompareOp::Less, FpFormat::Single) => "flt.s",
                (FpCompareOp::LessEqual, FpFormat::Single) => "fle.s",
                (FpCompareOp::Equal, FpFormat::Double) => "feq.d",
                (FpCompareOp::Less, FpFormat::Double) => "flt.d",
                (FpCompareOp::LessEqual, FpFormat::Double) => "fle.d",
            },
            Instruction::FpClass { format, .. } => match format {
                FpFormat::Half => "fclass.h",
                FpFormat::Single => "fclass.s",
                FpFormat::Double => "fclass.d",
            },
            Instruction::FpToInt {
                format,
                width,
                unsigned,
                ..
            } => to_int_mnemonic(format, width, unsigned),
            Instruction::IntToFp {
                format,
                width,
                unsigned,
                ..
            } => from_int_mnemonic(format, width, unsigned),
            Instruction::FpConvert { from, to, .. } => match (from, to) {
                (FpFormat::Half, FpFormat::Single) => "fcvt.s.h",
                (FpFormat::Half, FpFormat::Double) => "fcvt.d.h",
                (FpFormat::Single, FpFormat::Half) => "fcvt.h.s",
                (FpFormat::Double, FpFormat::Half) => "fcvt.h.d",
                (FpFormat::Double, FpFormat::Single) => "fcvt.s.d",
                (FpFormat::Single, FpFormat::Double) => "fcvt.d.s",
                _ => unreachable!("format conversion changes precision"),
            },
            Instruction::FpMoveToInt { format, .. } => match format {
                FpFormat::Half => "fmv.x.h",
                FpFormat::Single => "fmv.x.w",
                FpFormat::Double => "fmv.x.d",
            },
            Instruction::FpMoveFromInt { format, .. } => match format {
                FpFormat::Half => "fmv.h.x",
                FpFormat::Single => "fmv.w.x",
                FpFormat::Double => "fmv.d.x",
            },
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
            Instruction::LrW { .. } => "lr.w",
            Instruction::LrD { .. } => "lr.d",
            Instruction::ScW { .. } => "sc.w",
            Instruction::ScD { .. } => "sc.d",
            Instruction::AmoSwapW { .. } => "amoswap.w",
            Instruction::AmoAddW { .. } => "amoadd.w",
            Instruction::AmoXorW { .. } => "amoxor.w",
            Instruction::AmoOrW { .. } => "amoor.w",
            Instruction::AmoAndW { .. } => "amoand.w",
            Instruction::AmoMinW { .. } => "amomin.w",
            Instruction::AmoMaxW { .. } => "amomax.w",
            Instruction::AmoMinuW { .. } => "amominu.w",
            Instruction::AmoMaxuW { .. } => "amomaxu.w",
            Instruction::AmoSwapD { .. } => "amoswap.d",
            Instruction::AmoAddD { .. } => "amoadd.d",
            Instruction::AmoXorD { .. } => "amoxor.d",
            Instruction::AmoOrD { .. } => "amoor.d",
            Instruction::AmoAndD { .. } => "amoand.d",
            Instruction::AmoMinD { .. } => "amomin.d",
            Instruction::AmoMaxD { .. } => "amomax.d",
            Instruction::AmoMinuD { .. } => "amominu.d",
            Instruction::AmoMaxuD { .. } => "amomaxu.d",
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
            Instruction::CboClean { .. } => "cbo.clean",
            Instruction::CboFlush { .. } => "cbo.flush",
            Instruction::CboInval { .. } => "cbo.inval",
            Instruction::CboZero { .. } => "cbo.zero",
            Instruction::PrefetchI { .. } => "prefetch.i",
            Instruction::PrefetchR { .. } => "prefetch.r",
            Instruction::PrefetchW { .. } => "prefetch.w",
            Instruction::Pause => "pause",
            Instruction::Csrrw { .. } => "csrrw",
            Instruction::Csrrs { .. } => "csrrs",
            Instruction::Csrrc { .. } => "csrrc",
            Instruction::Csrrwi { .. } => "csrrwi",
            Instruction::Csrrsi { .. } => "csrrsi",
            Instruction::Csrrci { .. } => "csrrci",
            Instruction::Ecall => "ecall",
            Instruction::Ebreak => "ebreak",
            Instruction::Mret => "mret",
            Instruction::Sret => "sret",
            Instruction::Wfi => "wfi",
            Instruction::SinvalVma { .. } => "sinval.vma",
            Instruction::SfenceWInval => "sfence.w.inval",
            Instruction::SfenceInvalIr => "sfence.inval.ir",
            Instruction::SfenceVma { .. } => "sfence.vma",
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
            Instruction::AddUw { rd, rs1, rs2 } => reg_op("add.uw", rd, rs1, rs2),
            Instruction::Sh1Add { rd, rs1, rs2 } => reg_op("sh1add", rd, rs1, rs2),
            Instruction::Sh1AddUw { rd, rs1, rs2 } => reg_op("sh1add.uw", rd, rs1, rs2),
            Instruction::Sh2Add { rd, rs1, rs2 } => reg_op("sh2add", rd, rs1, rs2),
            Instruction::Sh2AddUw { rd, rs1, rs2 } => reg_op("sh2add.uw", rd, rs1, rs2),
            Instruction::Sh3Add { rd, rs1, rs2 } => reg_op("sh3add", rd, rs1, rs2),
            Instruction::Sh3AddUw { rd, rs1, rs2 } => reg_op("sh3add.uw", rd, rs1, rs2),
            Instruction::SlliUw { rd, rs1, shamt } => shamt_op("slli.uw", rd, rs1, shamt),
            Instruction::Andn { rd, rs1, rs2 } => reg_op("andn", rd, rs1, rs2),
            Instruction::Orn { rd, rs1, rs2 } => reg_op("orn", rd, rs1, rs2),
            Instruction::Xnor { rd, rs1, rs2 } => reg_op("xnor", rd, rs1, rs2),
            Instruction::Clz { rd, rs1 } => unary_op("clz", rd, rs1),
            Instruction::Clzw { rd, rs1 } => unary_op("clzw", rd, rs1),
            Instruction::Ctz { rd, rs1 } => unary_op("ctz", rd, rs1),
            Instruction::Ctzw { rd, rs1 } => unary_op("ctzw", rd, rs1),
            Instruction::Cpop { rd, rs1 } => unary_op("cpop", rd, rs1),
            Instruction::Cpopw { rd, rs1 } => unary_op("cpopw", rd, rs1),
            Instruction::Max { rd, rs1, rs2 } => reg_op("max", rd, rs1, rs2),
            Instruction::Maxu { rd, rs1, rs2 } => reg_op("maxu", rd, rs1, rs2),
            Instruction::Min { rd, rs1, rs2 } => reg_op("min", rd, rs1, rs2),
            Instruction::Minu { rd, rs1, rs2 } => reg_op("minu", rd, rs1, rs2),
            Instruction::SextB { rd, rs1 } => unary_op("sext.b", rd, rs1),
            Instruction::SextH { rd, rs1 } => unary_op("sext.h", rd, rs1),
            Instruction::ZextH { rd, rs1 } => unary_op("zext.h", rd, rs1),
            Instruction::Rol { rd, rs1, rs2 } => reg_op("rol", rd, rs1, rs2),
            Instruction::Rolw { rd, rs1, rs2 } => reg_op("rolw", rd, rs1, rs2),
            Instruction::Ror { rd, rs1, rs2 } => reg_op("ror", rd, rs1, rs2),
            Instruction::Rori { rd, rs1, shamt } => shamt_op("rori", rd, rs1, shamt),
            Instruction::Roriw { rd, rs1, shamt } => shamt_op("roriw", rd, rs1, shamt),
            Instruction::Rorw { rd, rs1, rs2 } => reg_op("rorw", rd, rs1, rs2),
            Instruction::OrcB { rd, rs1 } => unary_op("orc.b", rd, rs1),
            Instruction::Rev8 { rd, rs1 } => unary_op("rev8", rd, rs1),
            Instruction::Bset { rd, rs1, rs2 } => reg_op("bset", rd, rs1, rs2),
            Instruction::Bseti { rd, rs1, shamt } => shamt_op("bseti", rd, rs1, shamt),
            Instruction::Bclr { rd, rs1, rs2 } => reg_op("bclr", rd, rs1, rs2),
            Instruction::Bclri { rd, rs1, shamt } => shamt_op("bclri", rd, rs1, shamt),
            Instruction::Binv { rd, rs1, rs2 } => reg_op("binv", rd, rs1, rs2),
            Instruction::Binvi { rd, rs1, shamt } => shamt_op("binvi", rd, rs1, shamt),
            Instruction::Bext { rd, rs1, rs2 } => reg_op("bext", rd, rs1, rs2),
            Instruction::Bexti { rd, rs1, shamt } => shamt_op("bexti", rd, rs1, shamt),
            Instruction::Flh { rd, rs1, offset } => fp_load_op("flh", rd, rs1, offset),
            Instruction::Fsh { rs2, rs1, offset } => fp_store_op("fsh", rs2, rs1, offset),
            Instruction::Flw { rd, rs1, offset } => fp_load_op("flw", rd, rs1, offset),
            Instruction::Fsw { rs2, rs1, offset } => fp_store_op("fsw", rs2, rs1, offset),
            Instruction::Fld { rd, rs1, offset } => fp_load_op("fld", rd, rs1, offset),
            Instruction::Fsd { rs2, rs1, offset } => fp_store_op("fsd", rs2, rs1, offset),
            Instruction::FpBinary {
                format,
                op,
                rd,
                rs1,
                rs2,
                rounding,
            } => format!(
                "{} {},{},{}{}",
                fp_binary_mnemonic(op, format),
                fpr_abi_name(rd),
                fpr_abi_name(rs1),
                fpr_abi_name(rs2),
                fp_binary_rounding_suffix(op, rounding)
            ),
            Instruction::FpSqrt {
                format,
                rd,
                rs1,
                rounding,
            } => {
                format!(
                    "fsqrt{} {},{}{}",
                    format_suffix(format),
                    fpr_abi_name(rd),
                    fpr_abi_name(rs1),
                    rounding_suffix(rounding)
                )
            }
            Instruction::FpFused {
                format,
                op,
                rd,
                rs1,
                rs2,
                rs3,
                rounding,
            } => format!(
                "{} {},{},{},{}{}",
                fp_fused_mnemonic(op, format),
                fpr_abi_name(rd),
                fpr_abi_name(rs1),
                fpr_abi_name(rs2),
                fpr_abi_name(rs3),
                rounding_suffix(rounding)
            ),
            Instruction::FpCompare {
                format,
                op,
                rd,
                rs1,
                rs2,
            } => format!(
                "{} {},{},{}",
                fp_compare_mnemonic(op, format),
                abi_name(rd),
                fpr_abi_name(rs1),
                fpr_abi_name(rs2),
            ),
            Instruction::FpClass { format, rd, rs1 } => {
                format!(
                    "fclass{} {},{}",
                    format_suffix(format),
                    abi_name(rd),
                    fpr_abi_name(rs1)
                )
            }
            Instruction::FpToInt {
                format,
                rd,
                rs1,
                width,
                unsigned,
                rounding,
            } => format!(
                "{} {},{}{}",
                to_int_mnemonic(format, width, unsigned),
                abi_name(rd),
                fpr_abi_name(rs1),
                rounding_suffix(rounding)
            ),
            Instruction::IntToFp {
                format,
                rd,
                rs1,
                width,
                unsigned,
                rounding,
            } => format!(
                "{} {},{}{}",
                from_int_mnemonic(format, width, unsigned),
                fpr_abi_name(rd),
                abi_name(rs1),
                if format == FpFormat::Double && width == WORD_BITS {
                    ""
                } else {
                    rounding_suffix(rounding)
                }
            ),
            Instruction::FpConvert {
                from,
                to,
                rd,
                rs1,
                rounding,
            } => format!(
                "fcvt{}.{} {},{}{}",
                format_suffix(to),
                format_suffix(from).trim_start_matches('.'),
                fpr_abi_name(rd),
                fpr_abi_name(rs1),
                if is_widening_fp_conversion(from, to) {
                    ""
                } else {
                    rounding_suffix(rounding)
                }
            ),
            Instruction::FpMoveToInt { format, rd, rs1 } => {
                format!(
                    "fmv.x.{} {},{}",
                    move_suffix(format),
                    abi_name(rd),
                    fpr_abi_name(rs1)
                )
            }
            Instruction::FpMoveFromInt { format, rd, rs1 } => {
                format!(
                    "fmv.{}.x {},{}",
                    move_suffix(format),
                    fpr_abi_name(rd),
                    abi_name(rs1)
                )
            }
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
            Instruction::LrW { rd, rs1, aq, rl } => {
                format!(
                    "lr.w{} {},({})",
                    atomic_suffix(aq, rl),
                    abi_name(rd),
                    abi_name(rs1)
                )
            }
            Instruction::LrD { rd, rs1, aq, rl } => {
                format!(
                    "lr.d{} {},({})",
                    atomic_suffix(aq, rl),
                    abi_name(rd),
                    abi_name(rs1)
                )
            }
            Instruction::ScW {
                rd,
                rs1,
                rs2,
                aq,
                rl,
            } => format!(
                "sc.w{} {},{},({})",
                atomic_suffix(aq, rl),
                abi_name(rd),
                abi_name(rs2),
                abi_name(rs1)
            ),
            Instruction::ScD {
                rd,
                rs1,
                rs2,
                aq,
                rl,
            } => format!(
                "sc.d{} {},{},({})",
                atomic_suffix(aq, rl),
                abi_name(rd),
                abi_name(rs2),
                abi_name(rs1)
            ),
            Instruction::AmoSwapW {
                rd,
                rs1,
                rs2,
                aq,
                rl,
            } => amo_op("amoswap.w", rd, rs1, rs2, aq, rl),
            Instruction::AmoAddW {
                rd,
                rs1,
                rs2,
                aq,
                rl,
            } => amo_op("amoadd.w", rd, rs1, rs2, aq, rl),
            Instruction::AmoXorW {
                rd,
                rs1,
                rs2,
                aq,
                rl,
            } => amo_op("amoxor.w", rd, rs1, rs2, aq, rl),
            Instruction::AmoOrW {
                rd,
                rs1,
                rs2,
                aq,
                rl,
            } => amo_op("amoor.w", rd, rs1, rs2, aq, rl),
            Instruction::AmoAndW {
                rd,
                rs1,
                rs2,
                aq,
                rl,
            } => amo_op("amoand.w", rd, rs1, rs2, aq, rl),
            Instruction::AmoMinW {
                rd,
                rs1,
                rs2,
                aq,
                rl,
            } => amo_op("amomin.w", rd, rs1, rs2, aq, rl),
            Instruction::AmoMaxW {
                rd,
                rs1,
                rs2,
                aq,
                rl,
            } => amo_op("amomax.w", rd, rs1, rs2, aq, rl),
            Instruction::AmoMinuW {
                rd,
                rs1,
                rs2,
                aq,
                rl,
            } => amo_op("amominu.w", rd, rs1, rs2, aq, rl),
            Instruction::AmoMaxuW {
                rd,
                rs1,
                rs2,
                aq,
                rl,
            } => amo_op("amomaxu.w", rd, rs1, rs2, aq, rl),
            Instruction::AmoSwapD {
                rd,
                rs1,
                rs2,
                aq,
                rl,
            } => amo_op("amoswap.d", rd, rs1, rs2, aq, rl),
            Instruction::AmoAddD {
                rd,
                rs1,
                rs2,
                aq,
                rl,
            } => amo_op("amoadd.d", rd, rs1, rs2, aq, rl),
            Instruction::AmoXorD {
                rd,
                rs1,
                rs2,
                aq,
                rl,
            } => amo_op("amoxor.d", rd, rs1, rs2, aq, rl),
            Instruction::AmoOrD {
                rd,
                rs1,
                rs2,
                aq,
                rl,
            } => amo_op("amoor.d", rd, rs1, rs2, aq, rl),
            Instruction::AmoAndD {
                rd,
                rs1,
                rs2,
                aq,
                rl,
            } => amo_op("amoand.d", rd, rs1, rs2, aq, rl),
            Instruction::AmoMinD {
                rd,
                rs1,
                rs2,
                aq,
                rl,
            } => amo_op("amomin.d", rd, rs1, rs2, aq, rl),
            Instruction::AmoMaxD {
                rd,
                rs1,
                rs2,
                aq,
                rl,
            } => amo_op("amomax.d", rd, rs1, rs2, aq, rl),
            Instruction::AmoMinuD {
                rd,
                rs1,
                rs2,
                aq,
                rl,
            } => amo_op("amominu.d", rd, rs1, rs2, aq, rl),
            Instruction::AmoMaxuD {
                rd,
                rs1,
                rs2,
                aq,
                rl,
            } => amo_op("amomaxu.d", rd, rs1, rs2, aq, rl),
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
            Instruction::CboClean { rs1 } => cache_block_op("cbo.clean", rs1),
            Instruction::CboFlush { rs1 } => cache_block_op("cbo.flush", rs1),
            Instruction::CboInval { rs1 } => cache_block_op("cbo.inval", rs1),
            Instruction::CboZero { rs1 } => cache_block_op("cbo.zero", rs1),
            Instruction::PrefetchI { rs1, offset } => {
                format!("prefetch.i {offset}({})", abi_name(rs1))
            }
            Instruction::PrefetchR { rs1, offset } => {
                format!("prefetch.r {offset}({})", abi_name(rs1))
            }
            Instruction::PrefetchW { rs1, offset } => {
                format!("prefetch.w {offset}({})", abi_name(rs1))
            }
            Instruction::Pause => "pause".to_owned(),
            Instruction::Csrrw { rd, csr, rs1 } => csr_reg_op("csrrw", rd, csr, rs1),
            Instruction::Csrrs { rd, csr, rs1 } => csr_reg_op("csrrs", rd, csr, rs1),
            Instruction::Csrrc { rd, csr, rs1 } => csr_reg_op("csrrc", rd, csr, rs1),
            Instruction::Csrrwi { rd, csr, zimm } => csr_imm_op("csrrwi", rd, csr, zimm),
            Instruction::Csrrsi { rd, csr, zimm } => csr_imm_op("csrrsi", rd, csr, zimm),
            Instruction::Csrrci { rd, csr, zimm } => csr_imm_op("csrrci", rd, csr, zimm),
            Instruction::Ecall => "ecall".to_owned(),
            Instruction::Ebreak => "ebreak".to_owned(),
            Instruction::Mret => "mret".to_owned(),
            Instruction::Sret => "sret".to_owned(),
            Instruction::Wfi => "wfi".to_owned(),
            Instruction::SinvalVma { rs1, rs2 } => {
                format!("sinval.vma {},{}", abi_name(rs1), abi_name(rs2))
            }
            Instruction::SfenceWInval => "sfence.w.inval".to_owned(),
            Instruction::SfenceInvalIr => "sfence.inval.ir".to_owned(),
            Instruction::SfenceVma { rs1, rs2 } => {
                format!("sfence.vma {},{}", abi_name(rs1), abi_name(rs2))
            }
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

/// `mnemonic rd,rs1`
fn unary_op(mnemonic: &str, rd: u8, rs1: u8) -> String {
    format!("{mnemonic} {},{}", abi_name(rd), abi_name(rs1))
}

fn cache_block_op(mnemonic: &str, rs1: u8) -> String {
    format!("{mnemonic} ({})", abi_name(rs1))
}

fn fp_binary_mnemonic(op: FpBinaryOp, format: FpFormat) -> &'static str {
    match (op, format) {
        (FpBinaryOp::Add, FpFormat::Half) => "fadd.h",
        (FpBinaryOp::Sub, FpFormat::Half) => "fsub.h",
        (FpBinaryOp::Mul, FpFormat::Half) => "fmul.h",
        (FpBinaryOp::Div, FpFormat::Half) => "fdiv.h",
        (FpBinaryOp::SignInject, FpFormat::Half) => "fsgnj.h",
        (FpBinaryOp::SignInjectNegated, FpFormat::Half) => "fsgnjn.h",
        (FpBinaryOp::SignInjectXor, FpFormat::Half) => "fsgnjx.h",
        (FpBinaryOp::Min, FpFormat::Half) => "fmin.h",
        (FpBinaryOp::Max, FpFormat::Half) => "fmax.h",
        (FpBinaryOp::Add, FpFormat::Single) => "fadd.s",
        (FpBinaryOp::Sub, FpFormat::Single) => "fsub.s",
        (FpBinaryOp::Mul, FpFormat::Single) => "fmul.s",
        (FpBinaryOp::Div, FpFormat::Single) => "fdiv.s",
        (FpBinaryOp::SignInject, FpFormat::Single) => "fsgnj.s",
        (FpBinaryOp::SignInjectNegated, FpFormat::Single) => "fsgnjn.s",
        (FpBinaryOp::SignInjectXor, FpFormat::Single) => "fsgnjx.s",
        (FpBinaryOp::Min, FpFormat::Single) => "fmin.s",
        (FpBinaryOp::Max, FpFormat::Single) => "fmax.s",
        (FpBinaryOp::Add, FpFormat::Double) => "fadd.d",
        (FpBinaryOp::Sub, FpFormat::Double) => "fsub.d",
        (FpBinaryOp::Mul, FpFormat::Double) => "fmul.d",
        (FpBinaryOp::Div, FpFormat::Double) => "fdiv.d",
        (FpBinaryOp::SignInject, FpFormat::Double) => "fsgnj.d",
        (FpBinaryOp::SignInjectNegated, FpFormat::Double) => "fsgnjn.d",
        (FpBinaryOp::SignInjectXor, FpFormat::Double) => "fsgnjx.d",
        (FpBinaryOp::Min, FpFormat::Double) => "fmin.d",
        (FpBinaryOp::Max, FpFormat::Double) => "fmax.d",
    }
}

fn fp_binary_rounding_suffix(op: FpBinaryOp, rounding: u8) -> &'static str {
    if matches!(
        op,
        FpBinaryOp::Add | FpBinaryOp::Sub | FpBinaryOp::Mul | FpBinaryOp::Div
    ) {
        rounding_suffix(rounding)
    } else {
        ""
    }
}

/// Suffix printed by `objdump` for a floating-point rounding mode.
fn rounding_suffix(rounding: u8) -> &'static str {
    match rounding {
        ROUNDING_NEAREST_EVEN => ",rne",
        ROUNDING_TOWARD_ZERO => ",rtz",
        ROUNDING_DOWN => ",rdn",
        ROUNDING_UP => ",rup",
        ROUNDING_NEAREST_MAX_MAGNITUDE => ",rmm",
        ROUNDING_DYNAMIC => "",
        _ => "",
    }
}

fn fp_fused_mnemonic(op: FpFusedOp, format: FpFormat) -> &'static str {
    match (op, format) {
        (FpFusedOp::Madd, FpFormat::Half) => "fmadd.h",
        (FpFusedOp::Msub, FpFormat::Half) => "fmsub.h",
        (FpFusedOp::Nmsub, FpFormat::Half) => "fnmsub.h",
        (FpFusedOp::Nmadd, FpFormat::Half) => "fnmadd.h",
        (FpFusedOp::Madd, FpFormat::Single) => "fmadd.s",
        (FpFusedOp::Msub, FpFormat::Single) => "fmsub.s",
        (FpFusedOp::Nmsub, FpFormat::Single) => "fnmsub.s",
        (FpFusedOp::Nmadd, FpFormat::Single) => "fnmadd.s",
        (FpFusedOp::Madd, FpFormat::Double) => "fmadd.d",
        (FpFusedOp::Msub, FpFormat::Double) => "fmsub.d",
        (FpFusedOp::Nmsub, FpFormat::Double) => "fnmsub.d",
        (FpFusedOp::Nmadd, FpFormat::Double) => "fnmadd.d",
    }
}

fn fp_compare_mnemonic(op: FpCompareOp, format: FpFormat) -> &'static str {
    match (op, format) {
        (FpCompareOp::Equal, FpFormat::Half) => "feq.h",
        (FpCompareOp::Less, FpFormat::Half) => "flt.h",
        (FpCompareOp::LessEqual, FpFormat::Half) => "fle.h",
        (FpCompareOp::Equal, FpFormat::Single) => "feq.s",
        (FpCompareOp::Less, FpFormat::Single) => "flt.s",
        (FpCompareOp::LessEqual, FpFormat::Single) => "fle.s",
        (FpCompareOp::Equal, FpFormat::Double) => "feq.d",
        (FpCompareOp::Less, FpFormat::Double) => "flt.d",
        (FpCompareOp::LessEqual, FpFormat::Double) => "fle.d",
    }
}

fn to_int_mnemonic(format: FpFormat, width: u32, unsigned: bool) -> &'static str {
    match (format, width, unsigned) {
        (FpFormat::Half, WORD_BITS, false) => "fcvt.w.h",
        (FpFormat::Half, WORD_BITS, true) => "fcvt.wu.h",
        (FpFormat::Half, XLEN, false) => "fcvt.l.h",
        (FpFormat::Half, XLEN, true) => "fcvt.lu.h",
        (FpFormat::Single, WORD_BITS, false) => "fcvt.w.s",
        (FpFormat::Single, WORD_BITS, true) => "fcvt.wu.s",
        (FpFormat::Single, XLEN, false) => "fcvt.l.s",
        (FpFormat::Single, XLEN, true) => "fcvt.lu.s",
        (FpFormat::Double, WORD_BITS, false) => "fcvt.w.d",
        (FpFormat::Double, WORD_BITS, true) => "fcvt.wu.d",
        (FpFormat::Double, XLEN, false) => "fcvt.l.d",
        (FpFormat::Double, XLEN, true) => "fcvt.lu.d",
        _ => unreachable!("F only converts 32- and 64-bit integers"),
    }
}

fn from_int_mnemonic(format: FpFormat, width: u32, unsigned: bool) -> &'static str {
    match (format, width, unsigned) {
        (FpFormat::Half, WORD_BITS, false) => "fcvt.h.w",
        (FpFormat::Half, WORD_BITS, true) => "fcvt.h.wu",
        (FpFormat::Half, XLEN, false) => "fcvt.h.l",
        (FpFormat::Half, XLEN, true) => "fcvt.h.lu",
        (FpFormat::Single, WORD_BITS, false) => "fcvt.s.w",
        (FpFormat::Single, WORD_BITS, true) => "fcvt.s.wu",
        (FpFormat::Single, XLEN, false) => "fcvt.s.l",
        (FpFormat::Single, XLEN, true) => "fcvt.s.lu",
        (FpFormat::Double, WORD_BITS, false) => "fcvt.d.w",
        (FpFormat::Double, WORD_BITS, true) => "fcvt.d.wu",
        (FpFormat::Double, XLEN, false) => "fcvt.d.l",
        (FpFormat::Double, XLEN, true) => "fcvt.d.lu",
        _ => unreachable!("F only converts 32- and 64-bit integers"),
    }
}

fn format_suffix(format: FpFormat) -> &'static str {
    match format {
        FpFormat::Half => ".h",
        FpFormat::Single => ".s",
        FpFormat::Double => ".d",
    }
}

fn move_suffix(format: FpFormat) -> &'static str {
    match format {
        FpFormat::Half => "h",
        FpFormat::Single => "w",
        FpFormat::Double => "d",
    }
}

fn is_widening_fp_conversion(from: FpFormat, to: FpFormat) -> bool {
    matches!(
        (from, to),
        (FpFormat::Half, FpFormat::Single)
            | (FpFormat::Half, FpFormat::Double)
            | (FpFormat::Single, FpFormat::Double)
    )
}

/// `mnemonic fd,offset(rs1)`
fn fp_load_op(mnemonic: &str, rd: u8, rs1: u8, offset: i64) -> String {
    format!(
        "{mnemonic} {},{offset}({})",
        fpr_abi_name(rd),
        abi_name(rs1)
    )
}

/// `mnemonic fs2,offset(rs1)`
fn fp_store_op(mnemonic: &str, rs2: u8, rs1: u8, offset: i64) -> String {
    format!(
        "{mnemonic} {},{offset}({})",
        fpr_abi_name(rs2),
        abi_name(rs1)
    )
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

/// Ordering suffix used by `objdump -M no-aliases`.
fn atomic_suffix(aq: bool, rl: bool) -> &'static str {
    match (aq, rl) {
        (false, false) => "",
        (true, false) => ".aq",
        (false, true) => ".rl",
        (true, true) => ".aqrl",
    }
}

/// `mnemonic rd,rs2,(rs1)` with optional AQ/RL suffix.
fn amo_op(mnemonic: &str, rd: u8, rs1: u8, rs2: u8, aq: bool, rl: bool) -> String {
    format!(
        "{mnemonic}{} {},{},({})",
        atomic_suffix(aq, rl),
        abi_name(rd),
        abi_name(rs2),
        abi_name(rs1)
    )
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
        inst::CSR_MEDELEG => "medeleg".to_owned(),
        inst::CSR_MIDELEG => "mideleg".to_owned(),
        inst::CSR_MCOUNTEREN => "mcounteren".to_owned(),
        inst::CSR_MCOUNTINHIBIT => "mcountinhibit".to_owned(),
        inst::CSR_SSTATUS => "sstatus".to_owned(),
        inst::CSR_SIE => "sie".to_owned(),
        inst::CSR_STVEC => "stvec".to_owned(),
        inst::CSR_SCOUNTEREN => "scounteren".to_owned(),
        inst::CSR_SENVCFG => "senvcfg".to_owned(),
        inst::CSR_SSCRATCH => "sscratch".to_owned(),
        inst::CSR_SEPC => "sepc".to_owned(),
        inst::CSR_SCAUSE => "scause".to_owned(),
        inst::CSR_STVAL => "stval".to_owned(),
        inst::CSR_SIP => "sip".to_owned(),
        inst::CSR_SATP => "satp".to_owned(),
        inst::CSR_FFLAGS => "fflags".to_owned(),
        inst::CSR_FRM => "frm".to_owned(),
        inst::CSR_FCSR => "fcsr".to_owned(),
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
    ((imm >> U_IMMEDIATE_SHIFT) as u32) & U_FIELD_MASK
}

/// Decodes a 32-bit or compressed instruction word.
pub fn decode(word: u32) -> Result<Instruction, Trap> {
    decode_word(word).map(DecodedInstruction::instruction)
}

/// Decodes and renders an instruction exactly as the trace will print it.
pub fn render(word: u32) -> Result<String, Trap> {
    decode_word(word).map(DecodedInstruction::render)
}

/// Whether the low instruction parcel selects a compressed encoding.
pub fn is_compressed(word: u32) -> bool {
    word & mask_u32(MINOR_OPCODE_BITS) != u32::from(STANDARD_MINOR_OPCODE)
}

/// Decodes an instruction word while retaining its length and original encoding.
pub(crate) fn decode_word(word: u32) -> Result<DecodedInstruction, Trap> {
    if is_compressed(word) {
        let compressed = CompressedInstruction::decode(word as u16)?;
        let standard_word = compressed.decompress();
        let instruction =
            opcodes::lookup(Word::new(standard_word)).ok_or(Trap::IllegalInstruction)?;
        Ok(DecodedInstruction {
            instruction,
            encoding: Encoding::Compressed(compressed),
        })
    } else {
        let instruction = opcodes::lookup(Word::new(word)).ok_or(Trap::IllegalInstruction)?;
        Ok(DecodedInstruction::standard(instruction, word))
    }
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
        (self.0 >> shift) & mask_u32(width)
    }

    fn gather(self, fields: &[Field]) -> u32 {
        fields.iter().fold(0, |value, field| {
            value | (self.bits(field.source_shift, field.width) << field.target_shift)
        })
    }

    /// Destination register, bits 11:7.
    pub(crate) fn rd(self) -> u8 {
        self.bits(STANDARD_RD_SHIFT, REGISTER_BITS) as u8
    }

    /// First source register, bits 19:15.
    pub(crate) fn rs1(self) -> u8 {
        self.bits(STANDARD_RS1_SHIFT, REGISTER_BITS) as u8
    }

    /// Second source register, bits 24:20.
    pub(crate) fn rs2(self) -> u8 {
        self.bits(STANDARD_RS2_SHIFT, REGISTER_BITS) as u8
    }

    /// Third floating-point source register, bits 31:27.
    pub(crate) fn rs3(self) -> u8 {
        self.bits(STANDARD_RS3_SHIFT, REGISTER_BITS) as u8
    }

    /// Static rounding-mode field, bits 14:12.
    pub(crate) fn rounding(self) -> u8 {
        self.bits(STANDARD_ROUNDING_SHIFT, STANDARD_ROUNDING_BITS) as u8
    }

    /// Acquire-ordering bit, bit 26.
    pub(crate) fn aq(self) -> bool {
        self.bits(STANDARD_AQ_SHIFT, SINGLE_BIT_WIDTH) != 0
    }

    /// Release-ordering bit, bit 25.
    pub(crate) fn rl(self) -> bool {
        self.bits(STANDARD_RL_SHIFT, SINGLE_BIT_WIDTH) != 0
    }

    /// CSR address, bits 31:20.
    pub(crate) fn csr(self) -> u16 {
        self.bits(STANDARD_CSR_SHIFT, STANDARD_CSR_BITS) as u16
    }

    /// Five-bit unsigned immediate for the immediate CSR operations.
    pub(crate) fn zimm(self) -> u8 {
        self.rs1()
    }

    /// I-type immediate: `imm[11:0]`, bits 31:20.
    pub(crate) fn i_immediate(self) -> i64 {
        sign_extend(self.gather(I_IMMEDIATE_FIELDS), I_IMMEDIATE_BITS)
    }

    /// U-type immediate: `imm[31:12]` shifted into place.
    pub(crate) fn u_immediate(self) -> i64 {
        sign_extend(self.gather(U_IMMEDIATE_FIELDS), U_IMMEDIATE_BITS) << U_IMMEDIATE_SHIFT
    }

    /// S-type immediate: `imm[11:5]` at bits 31:25, `imm[4:0]` at bits 11:7.
    pub(crate) fn s_immediate(self) -> i64 {
        sign_extend(self.gather(S_IMMEDIATE_FIELDS), S_IMMEDIATE_BITS)
    }

    /// Fence predecessor set, bits 27:24.
    pub(crate) fn pred(self) -> u8 {
        self.bits(STANDARD_FENCE_PRED_SHIFT, STANDARD_FENCE_BITS) as u8
    }

    /// Fence successor set, bits 23:20.
    pub(crate) fn succ(self) -> u8 {
        self.bits(STANDARD_FENCE_SUCC_SHIFT, STANDARD_FENCE_BITS) as u8
    }

    /// Shift amount of the 64-bit shifts: `imm[5:0]`.
    pub(crate) fn shamt(self) -> u8 {
        self.bits(STANDARD_SHIFT_AMOUNT_SHIFT, STANDARD_SHIFT_AMOUNT_BITS) as u8
    }

    /// Shift amount of the W-forms: `imm[4:0]`.
    pub(crate) fn shamt_w(self) -> u8 {
        self.bits(STANDARD_SHIFT_AMOUNT_SHIFT, WORD_SHIFT_AMOUNT_BITS) as u8
    }

    /// B-type offset: `imm[12|10:5]` at bits 31:25, `imm[4:1|11]` at bits 11:7.
    pub(crate) fn b_immediate(self) -> i64 {
        sign_extend(self.gather(STANDARD_B_IMMEDIATE_FIELDS), B_IMMEDIATE_BITS)
    }

    /// J-type offset: `imm[20|10:1|11|19:12]`, bits 31:12.
    pub(crate) fn j_immediate(self) -> i64 {
        sign_extend(self.gather(STANDARD_J_IMMEDIATE_FIELDS), J_IMMEDIATE_BITS)
    }
}

/// Sign-extends the low `width` bits of `value` to 64 bits.
fn sign_extend(value: u32, width: u32) -> i64 {
    let shift = XLEN - width;
    (i64::from(value) << shift) >> shift
}

#[cfg(test)]
mod tests {
    use super::*;

    const C_ADDI4SPN_A5_SP_32: u32 = 0x101c;
    const C_ADDI16SP_SP_16: u32 = 0x6141;
    const C_ADDI16SP_SP_496: u32 = 0x617d;
    const C_ADDI16SP_SP_MINUS_512: u32 = 0x7101;
    const C_LD_A2_8_S0: u32 = 0x6410;
    const C_BEQZ_A5_PC_PLUS_4: u32 = 0xc391;
    const C_J_PC_PLUS_4: u32 = 0xa011;
    const C_JALR_S8: u32 = 0x9c02;
    const C_JR_RA: u32 = 0x8082;
    const C_LUI_RA_MINUS_32: u32 = 0x7081;
    const C_ADDI_ZERO_1_HINT: u32 = 0x0005;
    const C_MV_ZERO_RA_HINT: u32 = 0x8006;
    const C_ADD_ZERO_RA_HINT: u32 = 0x9006;
    const C_SLLI_ZERO_0_HINT: u32 = 0x0002;
    const C_SRLI_S0_0_HINT: u32 = 0x8001;
    const C_EBREAK: u32 = 0x9002;
    const C_ALL_ZERO_RESERVED: u32 = 0x0000;
    const C_ADDIW_ZERO_RESERVED: u32 = 0x2001;
    const C_ADDI16SP_ZERO_RESERVED: u32 = 0x6101;
    const C_LWSP_ZERO_RESERVED: u32 = 0x4002;
    const C_JR_ZERO_RESERVED: u32 = 0x8002;
    const C_ZEXT_B_UNAVAILABLE: u32 = 0x9c61;

    #[test]
    fn compressed_immediates_and_control_flow_decode() {
        assert_eq!(render(C_ADDI4SPN_A5_SP_32).unwrap(), "c.addi4spn a5,sp,32");
        assert_eq!(render(C_ADDI16SP_SP_16).unwrap(), "c.addi16sp sp,16");
        assert_eq!(render(C_ADDI16SP_SP_496).unwrap(), "c.addi16sp sp,496");
        assert_eq!(
            render(C_ADDI16SP_SP_MINUS_512).unwrap(),
            "c.addi16sp sp,-512"
        );
        assert_eq!(render(C_LD_A2_8_S0).unwrap(), "c.ld a2,8(s0)");
        assert_eq!(render(C_BEQZ_A5_PC_PLUS_4).unwrap(), "c.beqz a5,pc + 0x4");
        assert_eq!(render(C_J_PC_PLUS_4).unwrap(), "c.j pc + 0x4");
        assert_eq!(render(C_JALR_S8).unwrap(), "c.jalr s8");
        assert_eq!(render(C_JR_RA).unwrap(), "c.jr ra");
        assert_eq!(render(C_LUI_RA_MINUS_32).unwrap(), "c.lui ra,0xfffe0");
    }

    #[test]
    fn compressed_hints_remain_legal_no_ops() {
        assert_eq!(
            decode(C_ADDI_ZERO_1_HINT).unwrap(),
            Instruction::Addi {
                rd: 0,
                rs1: 0,
                imm: 0
            }
        );
        assert_eq!(
            decode(C_MV_ZERO_RA_HINT).unwrap(),
            Instruction::Add {
                rd: 0,
                rs1: 0,
                rs2: 1
            }
        );
        assert_eq!(
            decode(C_ADD_ZERO_RA_HINT).unwrap(),
            Instruction::Add {
                rd: 0,
                rs1: 0,
                rs2: 1
            }
        );
        assert_eq!(
            decode(C_SLLI_ZERO_0_HINT).unwrap(),
            Instruction::Slli {
                rd: 0,
                rs1: 0,
                shamt: 0
            }
        );
        assert_eq!(
            decode(C_SRLI_S0_0_HINT).unwrap(),
            Instruction::Srli {
                rd: 8,
                rs1: 8,
                shamt: 0
            }
        );
    }

    #[test]
    fn reserved_and_unavailable_compressed_encodings_are_illegal() {
        for word in [
            C_ALL_ZERO_RESERVED,
            C_ADDIW_ZERO_RESERVED,
            C_ADDI16SP_ZERO_RESERVED,
            C_LWSP_ZERO_RESERVED,
            C_JR_ZERO_RESERVED,
            C_ZEXT_B_UNAVAILABLE,
        ] {
            assert_eq!(decode(word), Err(Trap::IllegalInstruction), "{word:#06x}");
        }
    }

    #[test]
    fn c_ebreak_decodes_as_ebreak() {
        assert_eq!(decode(C_EBREAK), Ok(Instruction::Ebreak));
    }

    #[test]
    fn privileged_control_instructions_round_trip() {
        assert_eq!(decode(inst::MATCH_SRET), Ok(Instruction::Sret));
        assert_eq!(decode(inst::MATCH_WFI), Ok(Instruction::Wfi));
        assert_eq!(
            decode(inst::MATCH_SINVAL_VMA),
            Ok(Instruction::SinvalVma { rs1: 0, rs2: 0 })
        );
        assert_eq!(
            decode(inst::MATCH_SFENCE_W_INVAL),
            Ok(Instruction::SfenceWInval)
        );
        assert_eq!(
            decode(inst::MATCH_SFENCE_INVAL_IR),
            Ok(Instruction::SfenceInvalIr)
        );
        assert_eq!(render(inst::MATCH_SRET).unwrap(), "sret");
        assert_eq!(render(inst::MATCH_WFI).unwrap(), "wfi");
        assert_eq!(
            render(inst::MATCH_SINVAL_VMA).unwrap(),
            "sinval.vma zero,zero"
        );
        assert_eq!(
            render(inst::MATCH_SFENCE_W_INVAL).unwrap(),
            "sfence.w.inval"
        );
        assert_eq!(
            render(inst::MATCH_SFENCE_INVAL_IR).unwrap(),
            "sfence.inval.ir"
        );
    }

    #[test]
    fn static_rounding_is_preserved_in_double_conversions() {
        assert_eq!(
            decode(0xd227_c7d3),
            Ok(Instruction::IntToFp {
                format: FpFormat::Double,
                rd: 15,
                rs1: 15,
                width: XLEN,
                unsigned: false,
                rounding: 4,
            })
        );
    }

    #[test]
    fn prefetch_hints_preserve_aligned_offsets() {
        assert_eq!(render(0x0a11_6013).unwrap(), "prefetch.r 160(sp)");
        assert_eq!(render(0xfe01_e013).unwrap(), "prefetch.i -32(gp)");
        assert_eq!(render(0x3e32_6013).unwrap(), "prefetch.w 992(tp)");
    }
}
