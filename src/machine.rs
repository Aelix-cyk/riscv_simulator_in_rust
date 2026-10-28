//! A hart plus the bus it executes against.

use crate::bus::Bus;
use crate::cpu::{Cpu, PrivilegeMode, X0};
use crate::csr;
use crate::decode::{
    DecodedInstruction, FpBinaryOp, FpCompareOp, FpFormat, FpFusedOp, Instruction, decode_word,
    is_compressed,
};
use crate::fp::{self, RoundingMode};
use crate::fp16;
use crate::fp64;
use crate::inst;
use crate::trap::Trap;

/// Smallest legal instruction width with Zca implemented.
const MIN_INSTRUCTION_BYTES: u64 = 2;
const HALFWORD_BITS: u32 = 16;
const EMPTY_INSTRUCTION_BITS: u32 = 0;
const DOUBLEWORD_BYTES: u64 = 8;
const RESERVATION_BYTES: u64 = DOUBLEWORD_BYTES;
const RESERVATION_ALIGNMENT_MASK: u64 = !(RESERVATION_BYTES - 1);
const WORD_BITS: u32 = 32;
const HALF_BITS: u32 = 16;
const DYNAMIC_ROUNDING: u8 = 7;
const FLOAT_BITS_MASK: u64 = (1 << WORD_BITS) - 1;
const NAN_BOX: u64 = FLOAT_BITS_MASK << WORD_BITS;
const HALF_NAN_BOX: u64 = u64::MAX << HALF_BITS;
const BYTE_BITS: u32 = 8;
const BYTE_MASK: u64 = u8::MAX as u64;
const SHIFT_ONE: u32 = 1;
const SHIFT_TWO: u32 = 2;
const SHIFT_THREE: u32 = 3;
const CACHE_BLOCK_BYTES: u64 = 64;
const CACHE_BLOCK_DOUBLEWORDS: u64 = CACHE_BLOCK_BYTES / DOUBLEWORD_BYTES;
const CACHE_BLOCK_ALIGNMENT_MASK: u64 = !(CACHE_BLOCK_BYTES - 1);

/// A synchronous exception ready for architectural trap delivery.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Exception {
    cause: u8,
    tval: u64,
}

/// A failed fetch or decode, retaining instruction bits for `mtval`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FetchFailure {
    trap: Trap,
    raw: u32,
}

impl Exception {
    fn new(cause: u8, tval: u64) -> Self {
        Self { cause, tval }
    }
}

/// Arithmetic performed by an atomic memory operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AmoOperation {
    Swap,
    Add,
    Xor,
    Or,
    And,
    Min,
    Max,
    Minu,
    Maxu,
}

/// CSR-controlled cache-block operation classes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CacheBlockAccess {
    CleanFlush,
    Invalidate,
    Zero,
}

/// Purpose of a virtual-address translation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MemoryAccess {
    Fetch,
    Load,
    Store,
}

/// Why [`Machine::run`] returned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunOutcome {
    /// The hart executed the halt self-loop, `jal x0, 0`.
    Halted,
    /// The step limit ran out first.
    StepLimitReached,
}

/// Optional host-side behavior for one [`Machine::step`] call.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StepOptions {
    /// Print the original PC and decoded instruction before execution.
    pub trace: bool,
}

struct FpBinaryRequest {
    format: FpFormat,
    op: FpBinaryOp,
    rd: u8,
    rs1: u8,
    rs2: u8,
    rounding: u8,
    word: u32,
}

struct FpFusedRequest {
    format: FpFormat,
    op: FpFusedOp,
    rd: u8,
    rs1: u8,
    rs2: u8,
    rs3: u8,
    rounding: u8,
    word: u32,
}

struct FpConversionRequest {
    format: FpFormat,
    rd: u8,
    rs1: u8,
    width: u32,
    unsigned: bool,
    rounding: u8,
    word: u32,
}

/// One hart and the bus it fetches from and executes against.
pub struct Machine<B: Bus> {
    cpu: Cpu,
    bus: B,
    halted: bool,
    reservation: Option<u64>,
}

impl<B: Bus> Machine<B> {
    /// Creates a hart with cleared registers.
    pub fn new(bus: B) -> Self {
        Self::with_cpu(Cpu::new(), bus)
    }

    /// Creates a hart with the given architectural state.
    pub fn with_cpu(cpu: Cpu, bus: B) -> Self {
        Self {
            cpu,
            bus,
            halted: false,
            reservation: None,
        }
    }

    /// The hart's registers and program counter.
    pub fn cpu(&self) -> &Cpu {
        &self.cpu
    }

    /// Mutable access, for programs and tests that start from a known state.
    pub fn cpu_mut(&mut self) -> &mut Cpu {
        &mut self.cpu
    }

    /// The bus this hart is connected to.
    pub fn bus_mut(&mut self) -> &mut B {
        &mut self.bus
    }

    /// Whether the halt self-loop has retired.
    pub fn halted(&self) -> bool {
        self.halted
    }

    /// Fetches and decodes the instruction at the PC, without executing it.
    pub fn fetch(&mut self) -> Result<Instruction, Trap> {
        self.fetch_decoded()
            .map(DecodedInstruction::instruction)
            .map_err(|failure| failure.trap)
    }

    /// Fetches, decodes, and executes one instruction.
    ///
    /// Synchronous faults enter the machine or delegated supervisor trap
    /// handler instead of escaping this method.
    pub fn step(&mut self, options: StepOptions) {
        if self.take_pending_supervisor_interrupt() {
            self.cpu.csrs_mut().advance_cycle();
            return;
        }
        let pc = self.cpu.pc();
        let instruction = match self.fetch_decoded() {
            Ok(instruction) => instruction,
            Err(failure) => {
                self.enter_trap(self.fetch_exception(pc, failure.raw, failure.trap));
                self.cpu.csrs_mut().advance_cycle();
                return;
            }
        };
        if options.trace {
            println!("{pc:#010x}  {}", instruction.render());
        }
        self.execute_with_decoded(instruction);
    }

    fn take_pending_supervisor_interrupt(&mut self) -> bool {
        let previous = self.cpu.privilege();
        let Some(cause) = self.cpu.csrs().pending_supervisor_interrupt(previous) else {
            return false;
        };
        let epc = self.cpu.pc();
        self.cpu.csrs_mut().enter_s_trap(epc, cause, 0, previous);
        self.cpu.set_privilege(PrivilegeMode::Supervisor);
        self.cpu.set_pc(self.cpu.csrs().supervisor_trap_vector());
        true
    }

    /// Runs until the halt self-loop retires or `step_limit` steps are taken.
    pub fn run(&mut self, step_limit: u64) -> RunOutcome {
        for _ in 0..step_limit {
            self.step(StepOptions::default());
            if self.halted {
                return RunOutcome::Halted;
            }
        }
        RunOutcome::StepLimitReached
    }

    /// Executes one already-fetched instruction.
    ///
    /// Synchronous faults enter the machine or delegated supervisor trap
    /// handler.
    pub fn execute(&mut self, instruction: Instruction) {
        let decoded = DecodedInstruction::standard(instruction, instruction_word(instruction));
        self.execute_with_decoded(decoded);
    }

    /// Fetches and decodes one compressed or standard instruction.
    fn fetch_decoded(&mut self) -> Result<DecodedInstruction, FetchFailure> {
        let pc = self.cpu.pc();
        if !pc.is_multiple_of(MIN_INSTRUCTION_BYTES) {
            return Err(FetchFailure {
                trap: Trap::InstructionAddressMisaligned,
                raw: EMPTY_INSTRUCTION_BITS,
            });
        }
        let low = self
            .bus_read_u16(pc, MemoryAccess::Fetch)
            .map_err(|trap| FetchFailure {
                trap,
                raw: EMPTY_INSTRUCTION_BITS,
            })?;
        if is_compressed(u32::from(low)) {
            return decode_word(u32::from(low)).map_err(|trap| FetchFailure {
                trap,
                raw: u32::from(low),
            });
        }
        let high = self
            .bus_read_u16(pc.wrapping_add(MIN_INSTRUCTION_BYTES), MemoryAccess::Fetch)
            .map_err(|trap| FetchFailure {
                trap,
                raw: EMPTY_INSTRUCTION_BITS,
            })?;
        let word = u32::from(low) | (u32::from(high) << HALFWORD_BITS);
        decode_word(word).map_err(|trap| FetchFailure { trap, raw: word })
    }

    /// Converts a fetch/decode fault into its architectural exception.
    fn fetch_exception(&self, pc: u64, word: u32, trap: Trap) -> Exception {
        match trap {
            Trap::IllegalInstruction => {
                Exception::new(inst::CAUSE_ILLEGAL_INSTRUCTION, u64::from(word))
            }
            Trap::AccessFault => Exception::new(inst::CAUSE_FETCH_ACCESS, pc),
            Trap::InstructionAddressMisaligned => Exception::new(inst::CAUSE_MISALIGNED_FETCH, pc),
        }
    }

    /// Executes an instruction with the exact fetched encoding available for
    /// `mtval`, then delivers any synchronous exception to the trap handler.
    fn execute_with_decoded(&mut self, decoded: DecodedInstruction) -> bool {
        let instruction = decoded.instruction();
        let written_csr = csr_write_target(instruction);
        let writes_cycle = written_csr == Some(inst::CSR_MCYCLE);
        let events = retirement_events(instruction);
        match self.execute_inner(decoded) {
            Ok(()) => {
                self.cpu.csrs_mut().record_retirement(events, written_csr);
                if !writes_cycle {
                    self.cpu.csrs_mut().advance_cycle();
                }
                true
            }
            Err(exception) => {
                self.enter_trap(exception);
                self.cpu.csrs_mut().advance_cycle();
                false
            }
        }
    }

    /// Delivers a synchronous trap to machine or delegated supervisor mode.
    fn enter_trap(&mut self, exception: Exception) {
        let epc = self.cpu.pc();
        let previous = self.cpu.privilege();
        let delegated = previous != PrivilegeMode::Machine
            && self.cpu.csrs().delegates_exception(exception.cause);
        if delegated {
            self.cpu.csrs_mut().enter_s_trap(
                epc,
                u64::from(exception.cause),
                exception.tval,
                previous,
            );
            self.cpu.set_privilege(PrivilegeMode::Supervisor);
            self.cpu.set_pc(self.cpu.csrs().supervisor_trap_vector());
        } else {
            self.cpu.csrs_mut().enter_trap(
                epc,
                u64::from(exception.cause),
                exception.tval,
                previous,
            );
            self.cpu.set_privilege(PrivilegeMode::Machine);
            self.cpu.set_pc(self.cpu.csrs().trap_vector());
        }
    }

    /// Executes one instruction without delivering exceptions.
    fn execute_inner(&mut self, decoded: DecodedInstruction) -> Result<(), Exception> {
        let instruction = decoded.instruction();
        let word = decoded.raw();
        let pc = self.cpu.pc();
        let mut next_pc = pc.wrapping_add(decoded.length());

        match instruction {
            Instruction::Addi { rd, rs1, imm } => {
                let value = self.gpr(rs1).wrapping_add_signed(imm);
                self.write(rd, value);
            }
            Instruction::Slti { rd, rs1, imm } => {
                self.write(rd, u64::from((self.gpr(rs1) as i64) < imm))
            }
            Instruction::Sltiu { rd, rs1, imm } => {
                self.write(rd, u64::from(self.gpr(rs1) < imm as u64))
            }
            Instruction::Xori { rd, rs1, imm } => self.write(rd, self.gpr(rs1) ^ imm as u64),
            Instruction::Ori { rd, rs1, imm } => self.write(rd, self.gpr(rs1) | imm as u64),
            Instruction::Andi { rd, rs1, imm } => self.write(rd, self.gpr(rs1) & imm as u64),
            Instruction::Slli { rd, rs1, shamt } => {
                self.write(rd, self.gpr(rs1).wrapping_shl(shamt.into()));
            }
            Instruction::Srli { rd, rs1, shamt } => {
                self.write(rd, self.gpr(rs1).wrapping_shr(shamt.into()));
            }
            Instruction::Srai { rd, rs1, shamt } => {
                self.write(rd, ((self.gpr(rs1) as i64) >> shamt) as u64);
            }
            Instruction::Addiw { rd, rs1, imm } => {
                self.write(rd, sign_extend_32(self.gpr(rs1).wrapping_add_signed(imm)));
            }
            Instruction::Slliw { rd, rs1, shamt } => {
                let value = (self.gpr(rs1) as u32).wrapping_shl(shamt.into());
                self.write(rd, sign_extend_32(u64::from(value)));
            }
            Instruction::Srliw { rd, rs1, shamt } => {
                let value = (self.gpr(rs1) as u32).wrapping_shr(shamt.into());
                self.write(rd, sign_extend_32(u64::from(value)));
            }
            Instruction::Sraiw { rd, rs1, shamt } => {
                let value = ((self.gpr(rs1) as u32) as i32) >> shamt;
                self.write(rd, sign_extend_32(u64::from(value as u32)));
            }
            Instruction::Add { rd, rs1, rs2 } => {
                let value = self.gpr(rs1).wrapping_add(self.gpr(rs2));
                self.write(rd, value);
            }
            Instruction::Sub { rd, rs1, rs2 } => {
                let value = self.gpr(rs1).wrapping_sub(self.gpr(rs2));
                self.write(rd, value);
            }
            Instruction::Sll { rd, rs1, rs2 } => {
                let value = self.gpr(rs1).wrapping_shl(shift_64(self.gpr(rs2)));
                self.write(rd, value);
            }
            Instruction::Slt { rd, rs1, rs2 } => {
                self.write(
                    rd,
                    u64::from((self.gpr(rs1) as i64) < (self.gpr(rs2) as i64)),
                );
            }
            Instruction::Sltu { rd, rs1, rs2 } => {
                self.write(rd, u64::from(self.gpr(rs1) < self.gpr(rs2)));
            }
            Instruction::Xor { rd, rs1, rs2 } => {
                let value = self.gpr(rs1) ^ self.gpr(rs2);
                self.write(rd, value);
            }
            Instruction::Srl { rd, rs1, rs2 } => {
                let value = self.gpr(rs1).wrapping_shr(shift_64(self.gpr(rs2)));
                self.write(rd, value);
            }
            Instruction::Sra { rd, rs1, rs2 } => {
                let value = ((self.gpr(rs1) as i64) >> shift_64(self.gpr(rs2))) as u64;
                self.write(rd, value);
            }
            Instruction::Or { rd, rs1, rs2 } => {
                let value = self.gpr(rs1) | self.gpr(rs2);
                self.write(rd, value);
            }
            Instruction::And { rd, rs1, rs2 } => {
                let value = self.gpr(rs1) & self.gpr(rs2);
                self.write(rd, value);
            }
            Instruction::Addw { rd, rs1, rs2 } => {
                let value = self.gpr(rs1).wrapping_add(self.gpr(rs2));
                self.write(rd, sign_extend_32(value));
            }
            Instruction::Subw { rd, rs1, rs2 } => {
                let value = self.gpr(rs1).wrapping_sub(self.gpr(rs2));
                self.write(rd, sign_extend_32(value));
            }
            Instruction::Sllw { rd, rs1, rs2 } => {
                let value = (self.gpr(rs1) as u32).wrapping_shl(shift_32(self.gpr(rs2)));
                self.write(rd, sign_extend_32(u64::from(value)));
            }
            Instruction::Srlw { rd, rs1, rs2 } => {
                let value = (self.gpr(rs1) as u32).wrapping_shr(shift_32(self.gpr(rs2)));
                self.write(rd, sign_extend_32(u64::from(value)));
            }
            Instruction::Sraw { rd, rs1, rs2 } => {
                let value = ((self.gpr(rs1) as u32) as i32) >> shift_32(self.gpr(rs2));
                self.write(rd, sign_extend_32(u64::from(value as u32)));
            }
            Instruction::AddUw { rd, rs1, rs2 } => {
                let value = u64::from(self.gpr(rs1) as u32).wrapping_add(self.gpr(rs2));
                self.write(rd, value);
            }
            Instruction::Sh1Add { rd, rs1, rs2 } => {
                let value = self
                    .gpr(rs2)
                    .wrapping_add(self.gpr(rs1).wrapping_shl(SHIFT_ONE));
                self.write(rd, value);
            }
            Instruction::Sh1AddUw { rd, rs1, rs2 } => {
                let shifted = u64::from(self.gpr(rs1) as u32).wrapping_shl(SHIFT_ONE);
                self.write(rd, self.gpr(rs2).wrapping_add(shifted));
            }
            Instruction::Sh2Add { rd, rs1, rs2 } => {
                let value = self
                    .gpr(rs2)
                    .wrapping_add(self.gpr(rs1).wrapping_shl(SHIFT_TWO));
                self.write(rd, value);
            }
            Instruction::Sh2AddUw { rd, rs1, rs2 } => {
                let shifted = u64::from(self.gpr(rs1) as u32).wrapping_shl(SHIFT_TWO);
                self.write(rd, self.gpr(rs2).wrapping_add(shifted));
            }
            Instruction::Sh3Add { rd, rs1, rs2 } => {
                let value = self
                    .gpr(rs2)
                    .wrapping_add(self.gpr(rs1).wrapping_shl(SHIFT_THREE));
                self.write(rd, value);
            }
            Instruction::Sh3AddUw { rd, rs1, rs2 } => {
                let shifted = u64::from(self.gpr(rs1) as u32).wrapping_shl(SHIFT_THREE);
                self.write(rd, self.gpr(rs2).wrapping_add(shifted));
            }
            Instruction::SlliUw { rd, rs1, shamt } => {
                let value = u64::from(self.gpr(rs1) as u32).wrapping_shl(shamt.into());
                self.write(rd, value);
            }
            Instruction::Andn { rd, rs1, rs2 } => {
                self.write(rd, self.gpr(rs1) & !self.gpr(rs2));
            }
            Instruction::Orn { rd, rs1, rs2 } => {
                self.write(rd, self.gpr(rs1) | !self.gpr(rs2));
            }
            Instruction::Xnor { rd, rs1, rs2 } => {
                self.write(rd, !(self.gpr(rs1) ^ self.gpr(rs2)));
            }
            Instruction::Clz { rd, rs1 } => {
                self.write(rd, u64::from(self.gpr(rs1).leading_zeros()));
            }
            Instruction::Clzw { rd, rs1 } => {
                self.write(rd, u64::from((self.gpr(rs1) as u32).leading_zeros()));
            }
            Instruction::Ctz { rd, rs1 } => {
                self.write(rd, u64::from(self.gpr(rs1).trailing_zeros()));
            }
            Instruction::Ctzw { rd, rs1 } => {
                self.write(rd, u64::from((self.gpr(rs1) as u32).trailing_zeros()));
            }
            Instruction::Cpop { rd, rs1 } => {
                self.write(rd, u64::from(self.gpr(rs1).count_ones()));
            }
            Instruction::Cpopw { rd, rs1 } => {
                self.write(rd, u64::from((self.gpr(rs1) as u32).count_ones()));
            }
            Instruction::Max { rd, rs1, rs2 } => {
                self.write(rd, (self.gpr(rs1) as i64).max(self.gpr(rs2) as i64) as u64);
            }
            Instruction::Maxu { rd, rs1, rs2 } => {
                self.write(rd, self.gpr(rs1).max(self.gpr(rs2)));
            }
            Instruction::Min { rd, rs1, rs2 } => {
                self.write(rd, (self.gpr(rs1) as i64).min(self.gpr(rs2) as i64) as u64);
            }
            Instruction::Minu { rd, rs1, rs2 } => {
                self.write(rd, self.gpr(rs1).min(self.gpr(rs2)));
            }
            Instruction::SextB { rd, rs1 } => {
                self.write(rd, self.gpr(rs1) as u8 as i8 as i64 as u64);
            }
            Instruction::SextH { rd, rs1 } => {
                self.write(rd, self.gpr(rs1) as u16 as i16 as i64 as u64);
            }
            Instruction::ZextH { rd, rs1 } => {
                self.write(rd, u64::from(self.gpr(rs1) as u16));
            }
            Instruction::Rol { rd, rs1, rs2 } => {
                self.write(rd, self.gpr(rs1).rotate_left(shift_64(self.gpr(rs2))));
            }
            Instruction::Rolw { rd, rs1, rs2 } => {
                let value = (self.gpr(rs1) as u32).rotate_left(shift_32(self.gpr(rs2)));
                self.write(rd, sign_extend_32(u64::from(value)));
            }
            Instruction::Ror { rd, rs1, rs2 } => {
                self.write(rd, self.gpr(rs1).rotate_right(shift_64(self.gpr(rs2))));
            }
            Instruction::Rori { rd, rs1, shamt } => {
                self.write(rd, self.gpr(rs1).rotate_right(shamt.into()));
            }
            Instruction::Roriw { rd, rs1, shamt } => {
                let value = (self.gpr(rs1) as u32).rotate_right(shamt.into());
                self.write(rd, sign_extend_32(u64::from(value)));
            }
            Instruction::Rorw { rd, rs1, rs2 } => {
                let value = (self.gpr(rs1) as u32).rotate_right(shift_32(self.gpr(rs2)));
                self.write(rd, sign_extend_32(u64::from(value)));
            }
            Instruction::OrcB { rd, rs1 } => {
                let value = self.gpr(rs1);
                let mut result = 0;
                for shift in (0..u64::BITS).step_by(BYTE_BITS as usize) {
                    if (value >> shift) & BYTE_MASK != 0 {
                        result |= BYTE_MASK << shift;
                    }
                }
                self.write(rd, result);
            }
            Instruction::Rev8 { rd, rs1 } => {
                self.write(rd, self.gpr(rs1).swap_bytes());
            }
            Instruction::Bset { rd, rs1, rs2 } => {
                self.write(
                    rd,
                    self.gpr(rs1) | 1u64.wrapping_shl(shift_64(self.gpr(rs2))),
                );
            }
            Instruction::Bseti { rd, rs1, shamt } => {
                self.write(rd, self.gpr(rs1) | 1u64.wrapping_shl(shamt.into()));
            }
            Instruction::Bclr { rd, rs1, rs2 } => {
                self.write(
                    rd,
                    self.gpr(rs1) & !1u64.wrapping_shl(shift_64(self.gpr(rs2))),
                );
            }
            Instruction::Bclri { rd, rs1, shamt } => {
                self.write(rd, self.gpr(rs1) & !1u64.wrapping_shl(shamt.into()));
            }
            Instruction::Binv { rd, rs1, rs2 } => {
                self.write(
                    rd,
                    self.gpr(rs1) ^ 1u64.wrapping_shl(shift_64(self.gpr(rs2))),
                );
            }
            Instruction::Binvi { rd, rs1, shamt } => {
                self.write(rd, self.gpr(rs1) ^ 1u64.wrapping_shl(shamt.into()));
            }
            Instruction::Bext { rd, rs1, rs2 } => {
                self.write(rd, (self.gpr(rs1) >> shift_64(self.gpr(rs2))) & 1);
            }
            Instruction::Bexti { rd, rs1, shamt } => {
                self.write(rd, (self.gpr(rs1) >> shamt) & 1);
            }
            Instruction::Flh { rd, rs1, offset } => {
                self.require_fp_enabled(word)?;
                let address = self.gpr(rs1).wrapping_add_signed(offset);
                let value = self
                    .bus_read_u16(address, MemoryAccess::Load)
                    .map_err(|_| Exception::new(inst::CAUSE_LOAD_ACCESS, address))?;
                self.write_f16(rd, value);
            }
            Instruction::Fsh { rs2, rs1, offset } => {
                self.require_fp_enabled(word)?;
                let address = self.gpr(rs1).wrapping_add_signed(offset);
                let value = self.read_fpr(rs2) as u16;
                self.bus_write_u16(address, value, MemoryAccess::Store)
                    .map_err(|_| Exception::new(inst::CAUSE_STORE_ACCESS, address))?;
                self.invalidate_reservation(address, 2);
            }
            Instruction::Flw { rd, rs1, offset } => {
                self.require_fp_enabled(word)?;
                let address = self.gpr(rs1).wrapping_add_signed(offset);
                let value = self
                    .bus_read_u32(address, MemoryAccess::Load)
                    .map_err(|_| Exception::new(inst::CAUSE_LOAD_ACCESS, address))?;
                self.write_f32(rd, value);
            }
            Instruction::Fld { rd, rs1, offset } => {
                self.require_fp_enabled(word)?;
                let address = self.gpr(rs1).wrapping_add_signed(offset);
                let value = self
                    .bus_read_u64(address, MemoryAccess::Load)
                    .map_err(|_| Exception::new(inst::CAUSE_LOAD_ACCESS, address))?;
                self.write_f64(rd, value);
            }
            Instruction::Fsw { rs2, rs1, offset } => {
                self.require_fp_enabled(word)?;
                let address = self.gpr(rs1).wrapping_add_signed(offset);
                let value = self.read_fpr(rs2) as u32;
                self.bus_write_u32(address, value, MemoryAccess::Store)
                    .map_err(|_| Exception::new(inst::CAUSE_STORE_ACCESS, address))?;
                self.invalidate_reservation(address, 4);
            }
            Instruction::Fsd { rs2, rs1, offset } => {
                self.require_fp_enabled(word)?;
                let address = self.gpr(rs1).wrapping_add_signed(offset);
                let value = self.read_fpr(rs2);
                self.bus_write_u64(address, value, MemoryAccess::Store)
                    .map_err(|_| Exception::new(inst::CAUSE_STORE_ACCESS, address))?;
                self.invalidate_reservation(address, 8);
            }
            Instruction::FpBinary {
                format,
                op,
                rd,
                rs1,
                rs2,
                rounding,
            } => self.execute_fp_binary(FpBinaryRequest {
                format,
                op,
                rd,
                rs1,
                rs2,
                rounding,
                word,
            })?,
            Instruction::FpSqrt {
                format,
                rd,
                rs1,
                rounding,
            } => {
                self.execute_fp_sqrt(format, rd, rs1, rounding, word)?;
            }
            Instruction::FpFused {
                format,
                op,
                rd,
                rs1,
                rs2,
                rs3,
                rounding,
            } => self.execute_fp_fused(FpFusedRequest {
                format,
                op,
                rd,
                rs1,
                rs2,
                rs3,
                rounding,
                word,
            })?,
            Instruction::FpCompare {
                format,
                op,
                rd,
                rs1,
                rs2,
            } => self.execute_fp_compare(format, op, rd, rs1, rs2, word)?,
            Instruction::FpClass { format, rd, rs1 } => {
                self.execute_fp_class(format, rd, rs1, word)?;
            }
            Instruction::FpToInt {
                format,
                rd,
                rs1,
                width,
                unsigned,
                rounding,
            } => self.execute_fp_to_int(FpConversionRequest {
                format,
                rd,
                rs1,
                width,
                unsigned,
                rounding,
                word,
            })?,
            Instruction::IntToFp {
                format,
                rd,
                rs1,
                width,
                unsigned,
                rounding,
            } => self.execute_int_to_fp(FpConversionRequest {
                format,
                rd,
                rs1,
                width,
                unsigned,
                rounding,
                word,
            })?,
            Instruction::FpConvert {
                from,
                to,
                rd,
                rs1,
                rounding,
            } => self.execute_fp_convert(from, to, rd, rs1, rounding, word)?,
            Instruction::FpMoveToInt { format, rd, rs1 } => {
                self.require_fp_enabled(word)?;
                let value = match format {
                    FpFormat::Half => sign_extend_16(self.read_fpr(rs1)),
                    FpFormat::Single => sign_extend_32(self.read_fpr(rs1)),
                    FpFormat::Double => self.read_fpr(rs1),
                };
                self.write(rd, value);
            }
            Instruction::FpMoveFromInt { format, rd, rs1 } => {
                self.require_fp_enabled(word)?;
                match format {
                    FpFormat::Half => self.write_f16(rd, self.gpr(rs1) as u16),
                    FpFormat::Single => self.write_f32(rd, self.gpr(rs1) as u32),
                    FpFormat::Double => self.write_f64(rd, self.gpr(rs1)),
                }
            }
            Instruction::Mul { rd, rs1, rs2 } => {
                self.write(rd, self.gpr(rs1).wrapping_mul(self.gpr(rs2)));
            }
            Instruction::Mulh { rd, rs1, rs2 } => {
                let value = i128::from(self.gpr(rs1) as i64) * i128::from(self.gpr(rs2) as i64);
                self.write(rd, (value >> 64) as u64);
            }
            Instruction::Mulhsu { rd, rs1, rs2 } => {
                let value = i128::from(self.gpr(rs1) as i64) * i128::from(self.gpr(rs2));
                self.write(rd, (value >> 64) as u64);
            }
            Instruction::Mulhu { rd, rs1, rs2 } => {
                let value = u128::from(self.gpr(rs1)) * u128::from(self.gpr(rs2));
                self.write(rd, (value >> 64) as u64);
            }
            Instruction::Mulw { rd, rs1, rs2 } => {
                let value = (self.gpr(rs1) as u32).wrapping_mul(self.gpr(rs2) as u32);
                self.write(rd, sign_extend_32(u64::from(value)));
            }
            Instruction::Div { rd, rs1, rs2 } => {
                let dividend = self.gpr(rs1) as i64;
                let divisor = self.gpr(rs2) as i64;
                let value = dividend.checked_div(divisor).unwrap_or(if divisor == 0 {
                    -1
                } else {
                    i64::MIN
                });
                self.write(rd, value as u64);
            }
            Instruction::Divu { rd, rs1, rs2 } => {
                let dividend = self.gpr(rs1);
                let divisor = self.gpr(rs2);
                self.write(rd, dividend.checked_div(divisor).unwrap_or(u64::MAX));
            }
            Instruction::Rem { rd, rs1, rs2 } => {
                let dividend = self.gpr(rs1) as i64;
                let divisor = self.gpr(rs2) as i64;
                let value = dividend.checked_rem(divisor).unwrap_or(if divisor == 0 {
                    dividend
                } else {
                    0
                });
                self.write(rd, value as u64);
            }
            Instruction::Remu { rd, rs1, rs2 } => {
                let dividend = self.gpr(rs1);
                let divisor = self.gpr(rs2);
                self.write(rd, dividend.checked_rem(divisor).unwrap_or(dividend));
            }
            Instruction::Divw { rd, rs1, rs2 } => {
                let dividend = self.gpr(rs1) as u32 as i32;
                let divisor = self.gpr(rs2) as u32 as i32;
                let value = dividend.checked_div(divisor).unwrap_or(if divisor == 0 {
                    -1
                } else {
                    i32::MIN
                });
                self.write(rd, sign_extend_32(u64::from(value as u32)));
            }
            Instruction::Divuw { rd, rs1, rs2 } => {
                let dividend = self.gpr(rs1) as u32;
                let divisor = self.gpr(rs2) as u32;
                let value = dividend.checked_div(divisor).unwrap_or(u32::MAX);
                self.write(rd, sign_extend_32(u64::from(value)));
            }
            Instruction::Remw { rd, rs1, rs2 } => {
                let dividend = self.gpr(rs1) as u32 as i32;
                let divisor = self.gpr(rs2) as u32 as i32;
                let value = dividend.checked_rem(divisor).unwrap_or(if divisor == 0 {
                    dividend
                } else {
                    0
                });
                self.write(rd, sign_extend_32(u64::from(value as u32)));
            }
            Instruction::Remuw { rd, rs1, rs2 } => {
                let dividend = self.gpr(rs1) as u32;
                let divisor = self.gpr(rs2) as u32;
                let value = dividend.checked_rem(divisor).unwrap_or(dividend);
                self.write(rd, sign_extend_32(u64::from(value)));
            }
            Instruction::LrW { rd, rs1, .. } => {
                let address = self.gpr(rs1);
                Self::check_atomic_alignment(address, 4, inst::CAUSE_MISALIGNED_LOAD)?;
                let value = self
                    .bus_read_u32(address, MemoryAccess::Load)
                    .map_err(|_| Exception::new(inst::CAUSE_LOAD_ACCESS, address))?;
                self.reservation = Some(Self::reservation_set(address));
                self.write(rd, sign_extend_32(u64::from(value)));
            }
            Instruction::LrD { rd, rs1, .. } => {
                let address = self.gpr(rs1);
                Self::check_atomic_alignment(address, 8, inst::CAUSE_MISALIGNED_LOAD)?;
                let value = self
                    .bus_read_u64(address, MemoryAccess::Load)
                    .map_err(|_| Exception::new(inst::CAUSE_LOAD_ACCESS, address))?;
                self.reservation = Some(Self::reservation_set(address));
                self.write(rd, value);
            }
            Instruction::ScW { rd, rs1, rs2, .. } => {
                let address = self.gpr(rs1);
                let source = self.gpr(rs2);
                Self::check_atomic_alignment(address, 4, inst::CAUSE_MISALIGNED_STORE)?;
                self.bus_check_write(address, 4, MemoryAccess::Store)
                    .map_err(|_| Exception::new(inst::CAUSE_STORE_ACCESS, address))?;
                let success = self.reservation.take() == Some(Self::reservation_set(address));
                if success {
                    self.bus_write_u32(address, source as u32, MemoryAccess::Store)
                        .map_err(|_| Exception::new(inst::CAUSE_STORE_ACCESS, address))?;
                    self.write(rd, 0);
                } else {
                    self.write(rd, 1);
                }
            }
            Instruction::ScD { rd, rs1, rs2, .. } => {
                let address = self.gpr(rs1);
                let source = self.gpr(rs2);
                Self::check_atomic_alignment(address, 8, inst::CAUSE_MISALIGNED_STORE)?;
                self.bus_check_write(address, 8, MemoryAccess::Store)
                    .map_err(|_| Exception::new(inst::CAUSE_STORE_ACCESS, address))?;
                let success = self.reservation.take() == Some(Self::reservation_set(address));
                if success {
                    self.bus_write_u64(address, source, MemoryAccess::Store)
                        .map_err(|_| Exception::new(inst::CAUSE_STORE_ACCESS, address))?;
                    self.write(rd, 0);
                } else {
                    self.write(rd, 1);
                }
            }
            Instruction::AmoSwapW { rd, rs1, rs2, .. } => {
                self.execute_amo_w(rd, rs1, rs2, AmoOperation::Swap)?;
            }
            Instruction::AmoAddW { rd, rs1, rs2, .. } => {
                self.execute_amo_w(rd, rs1, rs2, AmoOperation::Add)?;
            }
            Instruction::AmoXorW { rd, rs1, rs2, .. } => {
                self.execute_amo_w(rd, rs1, rs2, AmoOperation::Xor)?;
            }
            Instruction::AmoOrW { rd, rs1, rs2, .. } => {
                self.execute_amo_w(rd, rs1, rs2, AmoOperation::Or)?;
            }
            Instruction::AmoAndW { rd, rs1, rs2, .. } => {
                self.execute_amo_w(rd, rs1, rs2, AmoOperation::And)?;
            }
            Instruction::AmoMinW { rd, rs1, rs2, .. } => {
                self.execute_amo_w(rd, rs1, rs2, AmoOperation::Min)?;
            }
            Instruction::AmoMaxW { rd, rs1, rs2, .. } => {
                self.execute_amo_w(rd, rs1, rs2, AmoOperation::Max)?;
            }
            Instruction::AmoMinuW { rd, rs1, rs2, .. } => {
                self.execute_amo_w(rd, rs1, rs2, AmoOperation::Minu)?;
            }
            Instruction::AmoMaxuW { rd, rs1, rs2, .. } => {
                self.execute_amo_w(rd, rs1, rs2, AmoOperation::Maxu)?;
            }
            Instruction::AmoSwapD { rd, rs1, rs2, .. } => {
                self.execute_amo_d(rd, rs1, rs2, AmoOperation::Swap)?;
            }
            Instruction::AmoAddD { rd, rs1, rs2, .. } => {
                self.execute_amo_d(rd, rs1, rs2, AmoOperation::Add)?;
            }
            Instruction::AmoXorD { rd, rs1, rs2, .. } => {
                self.execute_amo_d(rd, rs1, rs2, AmoOperation::Xor)?;
            }
            Instruction::AmoOrD { rd, rs1, rs2, .. } => {
                self.execute_amo_d(rd, rs1, rs2, AmoOperation::Or)?;
            }
            Instruction::AmoAndD { rd, rs1, rs2, .. } => {
                self.execute_amo_d(rd, rs1, rs2, AmoOperation::And)?;
            }
            Instruction::AmoMinD { rd, rs1, rs2, .. } => {
                self.execute_amo_d(rd, rs1, rs2, AmoOperation::Min)?;
            }
            Instruction::AmoMaxD { rd, rs1, rs2, .. } => {
                self.execute_amo_d(rd, rs1, rs2, AmoOperation::Max)?;
            }
            Instruction::AmoMinuD { rd, rs1, rs2, .. } => {
                self.execute_amo_d(rd, rs1, rs2, AmoOperation::Minu)?;
            }
            Instruction::AmoMaxuD { rd, rs1, rs2, .. } => {
                self.execute_amo_d(rd, rs1, rs2, AmoOperation::Maxu)?;
            }
            Instruction::Lb { rd, rs1, offset } => {
                let address = self.gpr(rs1).wrapping_add_signed(offset);
                let value = self
                    .bus_read_u8(address, MemoryAccess::Load)
                    .map_err(|_| Exception::new(inst::CAUSE_LOAD_ACCESS, address))?
                    as i8 as u64;
                self.write(rd, value);
            }
            Instruction::Lh { rd, rs1, offset } => {
                let address = self.gpr(rs1).wrapping_add_signed(offset);
                let value = self
                    .bus_read_u16(address, MemoryAccess::Load)
                    .map_err(|_| Exception::new(inst::CAUSE_LOAD_ACCESS, address))?
                    as i16 as u64;
                self.write(rd, value);
            }
            Instruction::Lw { rd, rs1, offset } => {
                let address = self.gpr(rs1).wrapping_add_signed(offset);
                let value = self
                    .bus_read_u32(address, MemoryAccess::Load)
                    .map_err(|_| Exception::new(inst::CAUSE_LOAD_ACCESS, address))?
                    as i32 as u64;
                self.write(rd, value);
            }
            Instruction::Ld { rd, rs1, offset } => {
                let address = self.gpr(rs1).wrapping_add_signed(offset);
                let value = self
                    .bus_read_u64(address, MemoryAccess::Load)
                    .map_err(|_| Exception::new(inst::CAUSE_LOAD_ACCESS, address))?;
                self.write(rd, value);
            }
            Instruction::Lbu { rd, rs1, offset } => {
                let address = self.gpr(rs1).wrapping_add_signed(offset);
                let value = u64::from(
                    self.bus_read_u8(address, MemoryAccess::Load)
                        .map_err(|_| Exception::new(inst::CAUSE_LOAD_ACCESS, address))?,
                );
                self.write(rd, value);
            }
            Instruction::Lhu { rd, rs1, offset } => {
                let address = self.gpr(rs1).wrapping_add_signed(offset);
                let value = u64::from(
                    self.bus_read_u16(address, MemoryAccess::Load)
                        .map_err(|_| Exception::new(inst::CAUSE_LOAD_ACCESS, address))?,
                );
                self.write(rd, value);
            }
            Instruction::Lwu { rd, rs1, offset } => {
                let address = self.gpr(rs1).wrapping_add_signed(offset);
                let value = u64::from(
                    self.bus_read_u32(address, MemoryAccess::Load)
                        .map_err(|_| Exception::new(inst::CAUSE_LOAD_ACCESS, address))?,
                );
                self.write(rd, value);
            }
            Instruction::Sb { rs2, rs1, offset } => {
                let address = self.gpr(rs1).wrapping_add_signed(offset);
                self.bus_write_u8(address, self.gpr(rs2) as u8, MemoryAccess::Store)
                    .map_err(|_| Exception::new(inst::CAUSE_STORE_ACCESS, address))?;
                self.invalidate_reservation(address, 1);
            }
            Instruction::Sh { rs2, rs1, offset } => {
                let address = self.gpr(rs1).wrapping_add_signed(offset);
                self.bus_write_u16(address, self.gpr(rs2) as u16, MemoryAccess::Store)
                    .map_err(|_| Exception::new(inst::CAUSE_STORE_ACCESS, address))?;
                self.invalidate_reservation(address, 2);
            }
            Instruction::Sw { rs2, rs1, offset } => {
                let address = self.gpr(rs1).wrapping_add_signed(offset);
                self.bus_write_u32(address, self.gpr(rs2) as u32, MemoryAccess::Store)
                    .map_err(|_| Exception::new(inst::CAUSE_STORE_ACCESS, address))?;
                self.invalidate_reservation(address, 4);
            }
            Instruction::Sd { rs2, rs1, offset } => {
                let address = self.gpr(rs1).wrapping_add_signed(offset);
                self.bus_write_u64(address, self.gpr(rs2), MemoryAccess::Store)
                    .map_err(|_| Exception::new(inst::CAUSE_STORE_ACCESS, address))?;
                self.invalidate_reservation(address, 8);
            }
            // Ordering operations have no effect on one hart with no cache model:
            // the shared PC update below retires them.
            Instruction::Fence { .. } | Instruction::FenceI => {}
            Instruction::CboClean { rs1 } => {
                self.execute_cache_block_management(rs1, word, CacheBlockAccess::CleanFlush)?;
            }
            Instruction::CboFlush { rs1 } => {
                self.execute_cache_block_management(rs1, word, CacheBlockAccess::CleanFlush)?;
            }
            Instruction::CboInval { rs1 } => {
                self.execute_cache_block_management(rs1, word, CacheBlockAccess::Invalidate)?;
            }
            Instruction::CboZero { rs1 } => {
                self.execute_cache_block_zero(rs1, word)?;
            }
            Instruction::PrefetchI { .. }
            | Instruction::PrefetchR { .. }
            | Instruction::PrefetchW { .. }
            | Instruction::Pause => {}
            Instruction::Csrrw { rd, csr, rs1 } => {
                let old = self.require_csr(csr, word)?;
                let source = self.gpr(rs1);
                self.write_csr(csr, source, word)?;
                self.write(rd, old);
            }
            Instruction::Csrrs { rd, csr, rs1 } => {
                let old = self.require_csr(csr, word)?;
                let source = self.gpr(rs1);
                if writes_csr_from_register(rs1) {
                    self.write_csr(csr, old | source, word)?;
                }
                self.write(rd, old);
            }
            Instruction::Csrrc { rd, csr, rs1 } => {
                let old = self.require_csr(csr, word)?;
                let source = self.gpr(rs1);
                if writes_csr_from_register(rs1) {
                    self.write_csr(csr, old & !source, word)?;
                }
                self.write(rd, old);
            }
            Instruction::Csrrwi { rd, csr, zimm } => {
                let old = self.require_csr(csr, word)?;
                self.write_csr(csr, u64::from(zimm), word)?;
                self.write(rd, old);
            }
            Instruction::Csrrsi { rd, csr, zimm } => {
                let old = self.require_csr(csr, word)?;
                if zimm != 0 {
                    self.write_csr(csr, old | u64::from(zimm), word)?;
                }
                self.write(rd, old);
            }
            Instruction::Csrrci { rd, csr, zimm } => {
                let old = self.require_csr(csr, word)?;
                if zimm != 0 {
                    self.write_csr(csr, old & !u64::from(zimm), word)?;
                }
                self.write(rd, old);
            }
            Instruction::Ecall => {
                let cause = match self.cpu.privilege() {
                    PrivilegeMode::Machine => inst::CAUSE_MACHINE_ECALL,
                    PrivilegeMode::Supervisor => inst::CAUSE_SUPERVISOR_ECALL,
                    PrivilegeMode::User => inst::CAUSE_USER_ECALL,
                };
                return Err(Exception::new(cause, 0));
            }
            Instruction::Ebreak => {
                return Err(Exception::new(inst::CAUSE_BREAKPOINT, pc));
            }
            Instruction::Mret => {
                if self.cpu.privilege() != PrivilegeMode::Machine {
                    return Err(Exception::new(
                        inst::CAUSE_ILLEGAL_INSTRUCTION,
                        u64::from(word),
                    ));
                }
                // Check the return target before changing any machine state.
                Self::checked_target(self.cpu.csrs().exception_pc())?;
                let (restored_pc, privilege) = self.cpu.csrs_mut().return_from_m_trap();
                self.cpu.set_privilege(privilege);
                self.cpu.set_pc(restored_pc);
                return Ok(());
            }
            Instruction::Sret => {
                if self.cpu.privilege() == PrivilegeMode::User
                    || (self.cpu.privilege() == PrivilegeMode::Supervisor && self.cpu.csrs().tsr())
                {
                    return Err(Exception::new(
                        inst::CAUSE_ILLEGAL_INSTRUCTION,
                        u64::from(word),
                    ));
                }
                Self::checked_target(self.cpu.csrs().supervisor_exception_pc())?;
                let (restored_pc, privilege) = self.cpu.csrs_mut().return_from_s_trap();
                self.cpu.set_privilege(privilege);
                self.cpu.set_pc(restored_pc);
                return Ok(());
            }
            Instruction::Wfi => {
                if self.cpu.privilege() == PrivilegeMode::User
                    || (self.cpu.privilege() == PrivilegeMode::Supervisor && self.cpu.csrs().tw())
                {
                    return Err(Exception::new(
                        inst::CAUSE_ILLEGAL_INSTRUCTION,
                        u64::from(word),
                    ));
                }
            }
            Instruction::SfenceVma { .. } => {
                if self.cpu.privilege() == PrivilegeMode::User
                    || (self.cpu.privilege() == PrivilegeMode::Supervisor
                        && (self.cpu.csrs().tvm() || self.cpu.csrs().satp_mode_read_only_bare()))
                {
                    return Err(Exception::new(
                        inst::CAUSE_ILLEGAL_INSTRUCTION,
                        u64::from(word),
                    ));
                }
            }
            Instruction::Beq { rs1, rs2, offset } => {
                if self.gpr(rs1) == self.gpr(rs2) {
                    next_pc = Self::checked_target(pc.wrapping_add_signed(offset))?;
                }
            }
            Instruction::Bne { rs1, rs2, offset } => {
                if self.gpr(rs1) != self.gpr(rs2) {
                    next_pc = Self::checked_target(pc.wrapping_add_signed(offset))?;
                }
            }
            Instruction::Blt { rs1, rs2, offset } => {
                if (self.gpr(rs1) as i64) < (self.gpr(rs2) as i64) {
                    next_pc = Self::checked_target(pc.wrapping_add_signed(offset))?;
                }
            }
            Instruction::Bge { rs1, rs2, offset } => {
                if (self.gpr(rs1) as i64) >= (self.gpr(rs2) as i64) {
                    next_pc = Self::checked_target(pc.wrapping_add_signed(offset))?;
                }
            }
            Instruction::Bltu { rs1, rs2, offset } => {
                if self.gpr(rs1) < self.gpr(rs2) {
                    next_pc = Self::checked_target(pc.wrapping_add_signed(offset))?;
                }
            }
            Instruction::Bgeu { rs1, rs2, offset } => {
                if self.gpr(rs1) >= self.gpr(rs2) {
                    next_pc = Self::checked_target(pc.wrapping_add_signed(offset))?;
                }
            }
            Instruction::Lui { rd, imm } => self.write(rd, imm as u64),
            Instruction::Auipc { rd, imm } => {
                self.write(rd, pc.wrapping_add_signed(imm));
            }
            Instruction::Jalr { rd, rs1, offset } => {
                let target = Self::checked_target(self.gpr(rs1).wrapping_add_signed(offset) & !1)?;
                self.write(rd, next_pc);
                next_pc = target;
            }
            Instruction::Jal { rd, offset } => {
                // `jal x0, 0` is the halt convention: no link, no movement.
                if decoded.is_standard_jal_self_loop() {
                    self.halted = true;
                    return Ok(());
                }
                // Check the target before linking, so a trapping jump leaves the
                // destination register alone, as `jalr` and the branches do.
                let target = Self::checked_target(pc.wrapping_add_signed(offset))?;
                self.write(rd, next_pc);
                next_pc = target;
            }
        }
        self.cpu.set_pc(next_pc);
        Ok(())
    }

    /// Reads general-purpose register `index`.
    fn gpr(&self, index: u8) -> u64 {
        self.cpu.read_gpr(usize::from(index))
    }

    /// Reads a CSR or raises the illegal-instruction exception.
    fn require_csr(&self, address: u16, word: u32) -> Result<u64, Exception> {
        self.cpu.read_csr(address).ok_or(Exception::new(
            inst::CAUSE_ILLEGAL_INSTRUCTION,
            u64::from(word),
        ))
    }

    /// Writes a CSR or raises the illegal-instruction exception.
    fn write_csr(&mut self, address: u16, value: u64, word: u32) -> Result<(), Exception> {
        if self.cpu.write_csr(address, value) {
            Ok(())
        } else {
            Err(Exception::new(
                inst::CAUSE_ILLEGAL_INSTRUCTION,
                u64::from(word),
            ))
        }
    }

    /// Checks a naturally aligned atomic operand.
    fn check_atomic_alignment(address: u64, width: u64, cause: u8) -> Result<(), Exception> {
        if address.is_multiple_of(width) {
            Ok(())
        } else {
            Err(Exception::new(cause, address))
        }
    }

    /// Returns the naturally aligned 8-byte reservation-set address.
    fn reservation_set(address: u64) -> u64 {
        address & RESERVATION_ALIGNMENT_MASK
    }

    /// Clears a reservation touched by a successful ordinary write.
    fn invalidate_reservation(&mut self, address: u64, width: u64) {
        let first = Self::reservation_set(address);
        let last = Self::reservation_set(address.saturating_add(width - 1));
        if self
            .reservation
            .is_some_and(|reserved| reserved >= first && reserved <= last)
        {
            self.reservation = None;
        }
    }

    /// Executes an RV64A word-sized atomic memory operation.
    fn execute_amo_w(
        &mut self,
        rd: u8,
        rs1: u8,
        rs2: u8,
        operation: AmoOperation,
    ) -> Result<(), Exception> {
        let address = self.gpr(rs1);
        let source = self.gpr(rs2) as u32;
        Self::check_atomic_alignment(address, 4, inst::CAUSE_MISALIGNED_STORE)?;
        let old = self
            .bus_read_u32(address, MemoryAccess::Store)
            .map_err(|_| Exception::new(inst::CAUSE_STORE_ACCESS, address))?;
        let new = apply_amo_w(old, source, operation);
        self.bus_write_u32(address, new, MemoryAccess::Store)
            .map_err(|_| Exception::new(inst::CAUSE_STORE_ACCESS, address))?;
        self.invalidate_reservation(address, 4);
        self.write(rd, sign_extend_32(u64::from(old)));
        Ok(())
    }

    /// Executes an RV64A doubleword-sized atomic memory operation.
    fn execute_amo_d(
        &mut self,
        rd: u8,
        rs1: u8,
        rs2: u8,
        operation: AmoOperation,
    ) -> Result<(), Exception> {
        let address = self.gpr(rs1);
        let source = self.gpr(rs2);
        Self::check_atomic_alignment(address, 8, inst::CAUSE_MISALIGNED_STORE)?;
        let old = self
            .bus_read_u64(address, MemoryAccess::Store)
            .map_err(|_| Exception::new(inst::CAUSE_STORE_ACCESS, address))?;
        let new = apply_amo_d(old, source, operation);
        self.bus_write_u64(address, new, MemoryAccess::Store)
            .map_err(|_| Exception::new(inst::CAUSE_STORE_ACCESS, address))?;
        self.invalidate_reservation(address, 8);
        self.write(rd, old);
        Ok(())
    }

    /// Checks that a control-transfer target is two-byte aligned.
    fn checked_target(target: u64) -> Result<u64, Exception> {
        if !target.is_multiple_of(MIN_INSTRUCTION_BYTES) {
            return Err(Exception::new(inst::CAUSE_MISALIGNED_FETCH, target));
        }
        Ok(target)
    }

    fn execute_fp_binary(&mut self, request: FpBinaryRequest) -> Result<(), Exception> {
        let FpBinaryRequest {
            format,
            op,
            rd,
            rs1,
            rs2,
            rounding,
            word,
        } = request;
        self.require_fp_enabled(word)?;
        let a = self.read_fp(format, rs1);
        let b = self.read_fp(format, rs2);
        match op {
            FpBinaryOp::Add | FpBinaryOp::Sub | FpBinaryOp::Mul | FpBinaryOp::Div => {
                let mode = self.rounding_mode(rounding, word)?;
                match format {
                    FpFormat::Half => {
                        let a = a as u16;
                        let b = b as u16;
                        let result = match op {
                            FpBinaryOp::Add => fp16::add(a, b, mode),
                            FpBinaryOp::Sub => fp16::sub(a, b, mode),
                            FpBinaryOp::Mul => fp16::mul(a, b, mode),
                            FpBinaryOp::Div => fp16::div(a, b, mode),
                            _ => unreachable!(),
                        };
                        self.write_fp16_result(rd, result);
                    }
                    FpFormat::Single => {
                        let a = a as u32;
                        let b = b as u32;
                        let result = match op {
                            FpBinaryOp::Add => fp::add(a, b, mode),
                            FpBinaryOp::Sub => fp::sub(a, b, mode),
                            FpBinaryOp::Mul => fp::mul(a, b, mode),
                            FpBinaryOp::Div => fp::div(a, b, mode),
                            _ => unreachable!(),
                        };
                        self.write_fp_result(rd, result);
                    }
                    FpFormat::Double => {
                        let result = match op {
                            FpBinaryOp::Add => fp64::add(a, b, mode),
                            FpBinaryOp::Sub => fp64::sub(a, b, mode),
                            FpBinaryOp::Mul => fp64::mul(a, b, mode),
                            FpBinaryOp::Div => fp64::div(a, b, mode),
                            _ => unreachable!(),
                        };
                        self.write_fp64_result(rd, result);
                    }
                }
            }
            FpBinaryOp::SignInject
            | FpBinaryOp::SignInjectNegated
            | FpBinaryOp::SignInjectXor
            | FpBinaryOp::Min
            | FpBinaryOp::Max => {
                let (bits, flags) = match format {
                    FpFormat::Half => {
                        let a = a as u16;
                        let b = b as u16;
                        let sign_mask = format_sign_mask(format) as u16;
                        let (bits, flags) = match op {
                            FpBinaryOp::SignInject => ((a & !sign_mask) | (b & sign_mask), 0),
                            FpBinaryOp::SignInjectNegated => {
                                ((a & !sign_mask) | ((b ^ sign_mask) & sign_mask), 0)
                            }
                            FpBinaryOp::SignInjectXor => {
                                ((a & !sign_mask) | ((a ^ b) & sign_mask), 0)
                            }
                            FpBinaryOp::Min => {
                                let result = fp16::min(a, b);
                                (result.bits, result.flags)
                            }
                            FpBinaryOp::Max => {
                                let result = fp16::max(a, b);
                                (result.bits, result.flags)
                            }
                            _ => unreachable!(),
                        };
                        (u64::from(bits), flags)
                    }
                    FpFormat::Single => {
                        let a = a as u32;
                        let b = b as u32;
                        let sign_mask = format_sign_mask(format) as u32;
                        let (bits, flags) = match op {
                            FpBinaryOp::SignInject => ((a & !sign_mask) | (b & sign_mask), 0),
                            FpBinaryOp::SignInjectNegated => {
                                ((a & !sign_mask) | ((b ^ sign_mask) & sign_mask), 0)
                            }
                            FpBinaryOp::SignInjectXor => {
                                ((a & !sign_mask) | ((a ^ b) & sign_mask), 0)
                            }
                            FpBinaryOp::Min => {
                                let result = fp::min(a, b);
                                (result.bits, result.flags)
                            }
                            FpBinaryOp::Max => {
                                let result = fp::max(a, b);
                                (result.bits, result.flags)
                            }
                            _ => unreachable!(),
                        };
                        (u64::from(bits), flags)
                    }
                    FpFormat::Double => {
                        let sign_mask = format_sign_mask(format);
                        match op {
                            FpBinaryOp::SignInject => ((a & !sign_mask) | (b & sign_mask), 0),
                            FpBinaryOp::SignInjectNegated => {
                                ((a & !sign_mask) | ((b ^ sign_mask) & sign_mask), 0)
                            }
                            FpBinaryOp::SignInjectXor => {
                                ((a & !sign_mask) | ((a ^ b) & sign_mask), 0)
                            }
                            FpBinaryOp::Min => {
                                let result = fp64::min(a, b);
                                (result.bits, result.flags)
                            }
                            FpBinaryOp::Max => {
                                let result = fp64::max(a, b);
                                (result.bits, result.flags)
                            }
                            _ => unreachable!(),
                        }
                    }
                };
                self.write_fp_bits(rd, format, bits);
                self.write_fp_flags(flags);
            }
        }
        Ok(())
    }

    fn execute_fp_sqrt(
        &mut self,
        format: FpFormat,
        rd: u8,
        rs1: u8,
        rounding: u8,
        word: u32,
    ) -> Result<(), Exception> {
        self.require_fp_enabled(word)?;
        let mode = self.rounding_mode(rounding, word)?;
        match format {
            FpFormat::Half => {
                let result = fp16::sqrt(self.read_f16(rs1), mode);
                self.write_fp16_result(rd, result);
            }
            FpFormat::Single => {
                let result = fp::sqrt(self.read_f32(rs1), mode);
                self.write_fp_result(rd, result);
            }
            FpFormat::Double => {
                let result = fp64::sqrt(self.read_f64(rs1), mode);
                self.write_fp64_result(rd, result);
            }
        }
        Ok(())
    }

    fn execute_fp_fused(&mut self, request: FpFusedRequest) -> Result<(), Exception> {
        let FpFusedRequest {
            format,
            op,
            rd,
            rs1,
            rs2,
            rs3,
            rounding,
            word,
        } = request;
        self.require_fp_enabled(word)?;
        let mode = self.rounding_mode(rounding, word)?;
        let (negate_product, negate_addend) = fused_signs(op);
        match format {
            FpFormat::Half => {
                let result = fp16::fma(
                    self.read_f16(rs1),
                    self.read_f16(rs2),
                    self.read_f16(rs3),
                    mode,
                    negate_product,
                    negate_addend,
                );
                self.write_fp16_result(rd, result);
            }
            FpFormat::Single => {
                let result = fp::fma(
                    self.read_f32(rs1),
                    self.read_f32(rs2),
                    self.read_f32(rs3),
                    mode,
                    negate_product,
                    negate_addend,
                );
                self.write_fp_result(rd, result);
            }
            FpFormat::Double => {
                let result = fp64::fma(
                    self.read_f64(rs1),
                    self.read_f64(rs2),
                    self.read_f64(rs3),
                    mode,
                    negate_product,
                    negate_addend,
                );
                self.write_fp64_result(rd, result);
            }
        }
        Ok(())
    }

    fn execute_fp_compare(
        &mut self,
        format: FpFormat,
        op: FpCompareOp,
        rd: u8,
        rs1: u8,
        rs2: u8,
        word: u32,
    ) -> Result<(), Exception> {
        self.require_fp_enabled(word)?;
        let (result, flags) = match format {
            FpFormat::Half => {
                let comparison = match op {
                    FpCompareOp::Equal => fp16::Compare::Equal,
                    FpCompareOp::Less => fp16::Compare::Less,
                    FpCompareOp::LessEqual => fp16::Compare::LessEqual,
                };
                fp16::compare(self.read_f16(rs1), self.read_f16(rs2), comparison)
            }
            FpFormat::Single => {
                let comparison = match op {
                    FpCompareOp::Equal => fp::Compare::Equal,
                    FpCompareOp::Less => fp::Compare::Less,
                    FpCompareOp::LessEqual => fp::Compare::LessEqual,
                };
                fp::compare(self.read_f32(rs1), self.read_f32(rs2), comparison)
            }
            FpFormat::Double => {
                let comparison = match op {
                    FpCompareOp::Equal => fp64::Compare::Equal,
                    FpCompareOp::Less => fp64::Compare::Less,
                    FpCompareOp::LessEqual => fp64::Compare::LessEqual,
                };
                fp64::compare(self.read_f64(rs1), self.read_f64(rs2), comparison)
            }
        };
        self.write_fp_flags(flags);
        self.write(rd, u64::from(result == Some(true)));
        Ok(())
    }

    fn execute_fp_class(
        &mut self,
        format: FpFormat,
        rd: u8,
        rs1: u8,
        word: u32,
    ) -> Result<(), Exception> {
        self.require_fp_enabled(word)?;
        let class = match format {
            FpFormat::Half => u64::from(fp16::classify_bits(self.read_f16(rs1))),
            FpFormat::Single => u64::from(fp::classify_bits(self.read_f32(rs1))),
            FpFormat::Double => fp64::classify_bits(self.read_f64(rs1)),
        };
        self.write(rd, class);
        Ok(())
    }

    fn execute_fp_to_int(&mut self, request: FpConversionRequest) -> Result<(), Exception> {
        let FpConversionRequest {
            format,
            rd,
            rs1,
            width,
            unsigned,
            rounding,
            word,
        } = request;
        self.require_fp_enabled(word)?;
        let mode = self.rounding_mode(rounding, word)?;
        let (value, flags) = match format {
            FpFormat::Half => {
                let result = fp16::to_int(self.read_f16(rs1), width, unsigned, mode);
                (result.value, result.flags)
            }
            FpFormat::Single => {
                let result = fp::to_int(self.read_f32(rs1), width, unsigned, mode);
                (result.value, result.flags)
            }
            FpFormat::Double => {
                let result = fp64::to_int(self.read_f64(rs1), width, unsigned, mode);
                (result.value, result.flags)
            }
        };
        self.write_fp_flags(flags);
        self.write(rd, value);
        Ok(())
    }

    fn execute_int_to_fp(&mut self, request: FpConversionRequest) -> Result<(), Exception> {
        let FpConversionRequest {
            format,
            rd,
            rs1,
            width,
            unsigned,
            rounding,
            word,
        } = request;
        self.require_fp_enabled(word)?;
        let mode = self.rounding_mode(rounding, word)?;
        match format {
            FpFormat::Half => {
                let result = fp16::from_int(self.gpr(rs1), width, unsigned, mode);
                self.write_fp16_result(rd, result);
            }
            FpFormat::Single => {
                let result = fp::from_int(self.gpr(rs1), width, unsigned, mode);
                self.write_fp_result(rd, result);
            }
            FpFormat::Double => {
                let result = fp64::from_int(self.gpr(rs1), width, unsigned, mode);
                self.write_fp64_result(rd, result);
            }
        }
        Ok(())
    }

    fn execute_fp_convert(
        &mut self,
        from: FpFormat,
        to: FpFormat,
        rd: u8,
        rs1: u8,
        rounding: u8,
        word: u32,
    ) -> Result<(), Exception> {
        self.require_fp_enabled(word)?;
        let mode = self.rounding_mode(rounding, word)?;
        match (from, to) {
            (FpFormat::Half, FpFormat::Single) => {
                let result = fp16::to_single(self.read_f16(rs1));
                self.write_fp_result(rd, result);
            }
            (FpFormat::Half, FpFormat::Double) => {
                let result = fp16::to_double(self.read_f16(rs1));
                self.write_fp64_result(rd, result);
            }
            (FpFormat::Single, FpFormat::Double) => {
                let result = fp64::from_single(self.read_f32(rs1));
                self.write_fp64_result(rd, result);
            }
            (FpFormat::Single, FpFormat::Half) => {
                let result = fp16::from_single(self.read_f32(rs1), mode);
                self.write_fp16_result(rd, result);
            }
            (FpFormat::Double, FpFormat::Half) => {
                let result = fp16::from_double(self.read_f64(rs1), mode);
                self.write_fp16_result(rd, result);
            }
            (FpFormat::Double, FpFormat::Single) => {
                let result = fp64::to_single(self.read_f64(rs1), mode);
                self.write_fp_result(rd, result);
            }
            _ => unreachable!("format conversion changes width"),
        }
        Ok(())
    }

    /// Requires `mstatus.FS` to enable floating-point state.
    fn require_fp_enabled(&self, word: u32) -> Result<(), Exception> {
        if self.cpu.fp_enabled() {
            Ok(())
        } else {
            Err(Exception::new(
                inst::CAUSE_ILLEGAL_INSTRUCTION,
                u64::from(word),
            ))
        }
    }

    /// Executes a cache-block management operation on the cacheless bus.
    fn execute_cache_block_management(
        &mut self,
        rs1: u8,
        word: u32,
        access: CacheBlockAccess,
    ) -> Result<(), Exception> {
        self.require_cache_block_access(access, word)?;
        let address = self.gpr(rs1);
        let block = address & CACHE_BLOCK_ALIGNMENT_MASK;
        self.bus_check_write(block, CACHE_BLOCK_BYTES, MemoryAccess::Store)
            .map_err(|_| Exception::new(inst::CAUSE_STORE_ACCESS, address))
    }

    /// Executes cache-block zero after checking the complete block.
    fn execute_cache_block_zero(&mut self, rs1: u8, word: u32) -> Result<(), Exception> {
        self.require_cache_block_access(CacheBlockAccess::Zero, word)?;
        let address = self.gpr(rs1);
        let block = address & CACHE_BLOCK_ALIGNMENT_MASK;
        self.bus_check_write(block, CACHE_BLOCK_BYTES, MemoryAccess::Store)
            .map_err(|_| Exception::new(inst::CAUSE_STORE_ACCESS, address))?;

        for index in 0..CACHE_BLOCK_DOUBLEWORDS {
            let offset = index * DOUBLEWORD_BYTES;
            self.bus_write_u64(block.wrapping_add(offset), 0, MemoryAccess::Store)
                .map_err(|_| Exception::new(inst::CAUSE_STORE_ACCESS, address))?;
        }
        self.invalidate_reservation(block, CACHE_BLOCK_BYTES);
        Ok(())
    }

    /// Applies U-mode CBO enable controls; M-mode is always enabled.
    fn require_cache_block_access(
        &self,
        access: CacheBlockAccess,
        word: u32,
    ) -> Result<(), Exception> {
        let enabled = match (self.cpu.privilege(), access) {
            (PrivilegeMode::Machine, _) => true,
            (PrivilegeMode::Supervisor, CacheBlockAccess::CleanFlush) => {
                self.cpu.csrs().supervisor_cache_block_clean_flush_enabled()
            }
            (PrivilegeMode::Supervisor, CacheBlockAccess::Invalidate) => {
                self.cpu.csrs().supervisor_cache_block_invalidate_enabled()
            }
            (PrivilegeMode::Supervisor, CacheBlockAccess::Zero) => {
                self.cpu.csrs().supervisor_cache_block_zero_enabled()
            }
            (PrivilegeMode::User, CacheBlockAccess::CleanFlush) => {
                self.cpu.csrs().user_cache_block_clean_flush_enabled()
            }
            (PrivilegeMode::User, CacheBlockAccess::Invalidate) => {
                self.cpu.csrs().user_cache_block_invalidate_enabled()
            }
            (PrivilegeMode::User, CacheBlockAccess::Zero) => {
                self.cpu.csrs().user_cache_block_zero_enabled()
            }
        };
        if enabled {
            Ok(())
        } else {
            Err(Exception::new(
                inst::CAUSE_ILLEGAL_INSTRUCTION,
                u64::from(word),
            ))
        }
    }

    /// Translates an execution address to a physical Bus address.
    ///
    /// Phase 1 supports Bare translation only. Sv39 will replace this identity
    /// mapping without changing instruction execution paths.
    fn translate_address(&self, address: u64, _access: MemoryAccess) -> u64 {
        address
    }

    fn bus_read_u8(&mut self, address: u64, access: MemoryAccess) -> Result<u8, Trap> {
        let physical = self.translate_address(address, access);
        self.bus.read_u8(physical)
    }

    fn bus_read_u16(&mut self, address: u64, access: MemoryAccess) -> Result<u16, Trap> {
        let physical = self.translate_address(address, access);
        self.bus.read_u16(physical)
    }

    fn bus_read_u32(&mut self, address: u64, access: MemoryAccess) -> Result<u32, Trap> {
        let physical = self.translate_address(address, access);
        self.bus.read_u32(physical)
    }

    fn bus_read_u64(&mut self, address: u64, access: MemoryAccess) -> Result<u64, Trap> {
        let physical = self.translate_address(address, access);
        self.bus.read_u64(physical)
    }

    fn bus_write_u8(&mut self, address: u64, value: u8, access: MemoryAccess) -> Result<(), Trap> {
        let physical = self.translate_address(address, access);
        self.bus.write_u8(physical, value)
    }

    fn bus_write_u16(
        &mut self,
        address: u64,
        value: u16,
        access: MemoryAccess,
    ) -> Result<(), Trap> {
        let physical = self.translate_address(address, access);
        self.bus.write_u16(physical, value)
    }

    fn bus_write_u32(
        &mut self,
        address: u64,
        value: u32,
        access: MemoryAccess,
    ) -> Result<(), Trap> {
        let physical = self.translate_address(address, access);
        self.bus.write_u32(physical, value)
    }

    fn bus_write_u64(
        &mut self,
        address: u64,
        value: u64,
        access: MemoryAccess,
    ) -> Result<(), Trap> {
        let physical = self.translate_address(address, access);
        self.bus.write_u64(physical, value)
    }

    fn bus_check_write(
        &mut self,
        address: u64,
        width: u64,
        access: MemoryAccess,
    ) -> Result<(), Trap> {
        let physical = self.translate_address(address, access);
        self.bus.check_write(physical, width)
    }

    /// Reads an FPR, converting an unboxed value to the canonical NaN.
    fn read_f32(&self, index: u8) -> u32 {
        let value = self.read_fpr(index);
        if value >> WORD_BITS == u64::from(u32::MAX) {
            value as u32
        } else {
            fp::CANONICAL_NAN
        }
    }

    /// Reads a half-precision operand, converting an unboxed value to the
    /// canonical NaN.
    fn read_f16(&self, index: u8) -> u16 {
        let value = self.read_fpr(index);
        if value >> HALF_BITS == HALF_NAN_BOX >> HALF_BITS {
            value as u16
        } else {
            fp16::CANONICAL_NAN
        }
    }

    /// Reads a double-precision operand from an FPR.
    fn read_f64(&self, index: u8) -> u64 {
        self.read_fpr(index)
    }

    fn read_fp(&self, format: FpFormat, index: u8) -> u64 {
        match format {
            FpFormat::Half => u64::from(self.read_f16(index)),
            FpFormat::Single => u64::from(self.read_f32(index)),
            FpFormat::Double => self.read_f64(index),
        }
    }

    /// Reads the architectural bits of an FPR.
    fn read_fpr(&self, index: u8) -> u64 {
        self.cpu.read_fpr(usize::from(index))
    }

    /// Writes a single-precision value with architectural NaN boxing.
    fn write_f32(&mut self, index: u8, value: u32) {
        self.cpu
            .write_fpr(usize::from(index), NAN_BOX | u64::from(value));
    }

    /// Writes a half-precision value with architectural NaN boxing.
    fn write_f16(&mut self, index: u8, value: u16) {
        self.cpu
            .write_fpr(usize::from(index), HALF_NAN_BOX | u64::from(value));
    }

    /// Writes a double-precision value as architectural bits.
    fn write_f64(&mut self, index: u8, value: u64) {
        self.cpu.write_fpr(usize::from(index), value);
    }

    fn write_fp_bits(&mut self, index: u8, format: FpFormat, bits: u64) {
        match format {
            FpFormat::Half => self.write_f16(index, bits as u16),
            FpFormat::Single => self.write_f32(index, bits as u32),
            FpFormat::Double => self.write_f64(index, bits),
        }
    }

    /// Writes a result to an FPR and accumulates any raised flags.
    fn write_fp_result(&mut self, index: u8, result: fp::FpResult) {
        self.write_f32(index, result.bits);
        self.write_fp_flags(result.flags);
    }

    /// Writes a half-precision result and accumulates raised flags.
    fn write_fp16_result(&mut self, index: u8, result: fp16::FpResult) {
        self.write_f16(index, result.bits);
        self.write_fp_flags(result.flags);
    }

    /// Writes a double-precision result and accumulates raised flags.
    fn write_fp64_result(&mut self, index: u8, result: fp64::DoubleResult) {
        self.write_f64(index, result.bits);
        self.write_fp_flags(result.flags);
    }

    /// Accumulates newly raised IEEE exception flags.
    fn write_fp_flags(&mut self, flags: u8) {
        if flags == 0 {
            return;
        }
        let current = self
            .cpu
            .read_csr(inst::CSR_FCSR)
            .expect("F instructions require enabled FCSR access");
        assert!(
            self.cpu
                .write_csr(inst::CSR_FCSR, current | u64::from(flags))
        );
    }

    /// Resolves the static or dynamic rounding field for an FP instruction.
    fn rounding_mode(&self, encoded: u8, word: u32) -> Result<RoundingMode, Exception> {
        let value = if encoded == DYNAMIC_ROUNDING {
            self.cpu.read_csr(inst::CSR_FRM).ok_or(Exception::new(
                inst::CAUSE_ILLEGAL_INSTRUCTION,
                u64::from(word),
            ))? as u8
        } else {
            encoded
        };
        RoundingMode::from_bits(value).ok_or(Exception::new(
            inst::CAUSE_ILLEGAL_INSTRUCTION,
            u64::from(word),
        ))
    }

    /// Writes `value` to general-purpose register `rd`; `x0` discards it.
    fn write(&mut self, rd: u8, value: u64) {
        self.cpu.write_gpr(usize::from(rd), value);
    }
}

fn format_sign_mask(format: FpFormat) -> u64 {
    match format {
        FpFormat::Half => u64::from(fp16::SIGN_MASK),
        FpFormat::Single => u64::from(fp::SIGN_MASK),
        FpFormat::Double => fp64::SIGN_MASK,
    }
}

fn fused_signs(op: FpFusedOp) -> (bool, bool) {
    match op {
        FpFusedOp::Madd => (false, false),
        FpFusedOp::Msub => (false, true),
        FpFusedOp::Nmsub => (true, false),
        FpFusedOp::Nmadd => (true, true),
    }
}

/// Re-encodes decoded CSR instructions so direct `execute` calls can report
/// their instruction bits in `mtval` when the CSR is unsupported.
fn instruction_word(instruction: Instruction) -> u32 {
    const CSR_SHIFT: u32 = 20;
    const RS1_SHIFT: u32 = 15;
    const RS2_SHIFT: u32 = 20;
    const RD_SHIFT: u32 = 7;

    let (opcode, rd, rs1, csr) = match instruction {
        Instruction::Ecall => return inst::MATCH_ECALL,
        Instruction::Ebreak => return inst::MATCH_EBREAK,
        Instruction::Mret => return inst::MATCH_MRET,
        Instruction::Sret => return inst::MATCH_SRET,
        Instruction::Wfi => return inst::MATCH_WFI,
        Instruction::SfenceVma { rs1, rs2 } => {
            return inst::MATCH_SFENCE_VMA
                | (u32::from(rs1) << RS1_SHIFT)
                | (u32::from(rs2) << RS2_SHIFT);
        }
        Instruction::Csrrw { rd, csr, rs1 } => (inst::MATCH_CSRRW, rd, rs1, csr),
        Instruction::Csrrs { rd, csr, rs1 } => (inst::MATCH_CSRRS, rd, rs1, csr),
        Instruction::Csrrc { rd, csr, rs1 } => (inst::MATCH_CSRRC, rd, rs1, csr),
        Instruction::Csrrwi { rd, csr, zimm } => (inst::MATCH_CSRRWI, rd, zimm, csr),
        Instruction::Csrrsi { rd, csr, zimm } => (inst::MATCH_CSRRSI, rd, zimm, csr),
        Instruction::Csrrci { rd, csr, zimm } => (inst::MATCH_CSRRCI, rd, zimm, csr),
        _ => return 0,
    };
    opcode
        | (u32::from(csr) << CSR_SHIFT)
        | (u32::from(rs1) << RS1_SHIFT)
        | (u32::from(rd) << RD_SHIFT)
}

fn csr_write_target(instruction: Instruction) -> Option<u16> {
    match instruction {
        Instruction::Csrrw { csr, .. } | Instruction::Csrrwi { csr, .. } => Some(csr),
        Instruction::Csrrs { csr, rs1, .. } | Instruction::Csrrc { csr, rs1, .. }
            if writes_csr_from_register(rs1) =>
        {
            Some(csr)
        }
        Instruction::Csrrsi { csr, zimm, .. } if zimm != 0 => Some(csr),
        Instruction::Csrrci { csr, zimm, .. } if zimm != 0 => Some(csr),
        _ => None,
    }
}

fn writes_csr_from_register(register: u8) -> bool {
    usize::from(register) != X0
}

/// Computes the new value for an AMO word operation.
fn apply_amo_w(old: u32, source: u32, operation: AmoOperation) -> u32 {
    match operation {
        AmoOperation::Swap => source,
        AmoOperation::Add => old.wrapping_add(source),
        AmoOperation::Xor => old ^ source,
        AmoOperation::Or => old | source,
        AmoOperation::And => old & source,
        AmoOperation::Min => (old as i32).min(source as i32) as u32,
        AmoOperation::Max => (old as i32).max(source as i32) as u32,
        AmoOperation::Minu => old.min(source),
        AmoOperation::Maxu => old.max(source),
    }
}

/// Computes the new value for an AMO doubleword operation.
fn apply_amo_d(old: u64, source: u64, operation: AmoOperation) -> u64 {
    match operation {
        AmoOperation::Swap => source,
        AmoOperation::Add => old.wrapping_add(source),
        AmoOperation::Xor => old ^ source,
        AmoOperation::Or => old | source,
        AmoOperation::And => old & source,
        AmoOperation::Min => (old as i64).min(source as i64) as u64,
        AmoOperation::Max => (old as i64).max(source as i64) as u64,
        AmoOperation::Minu => old.min(source),
        AmoOperation::Maxu => old.max(source),
    }
}

/// Sign-extends the low 32 bits of `value` to 64 bits.
fn sign_extend_32(value: u64) -> u64 {
    i64::from(value as u32 as i32) as u64
}

/// Sign-extends the low 16 bits of `value` to 64 bits.
fn sign_extend_16(value: u64) -> u64 {
    i64::from(value as u16 as i16) as u64
}

/// Shift amount of the 64-bit shifts, taken from the low six bits.
fn shift_64(value: u64) -> u32 {
    (value & 0x3f) as u32
}

/// Shift amount of the W-forms, taken from the low five bits.
fn shift_32(value: u64) -> u32 {
    (value & 0x1f) as u32
}

/// Event categories produced by one successfully retired instruction.
fn retirement_events(instruction: Instruction) -> u8 {
    let mut events = csr::HPM_EVENT_INSTRUCTION;
    if matches!(
        instruction,
        Instruction::Lb { .. }
            | Instruction::Lh { .. }
            | Instruction::Lw { .. }
            | Instruction::Ld { .. }
            | Instruction::Lbu { .. }
            | Instruction::Lhu { .. }
            | Instruction::Lwu { .. }
            | Instruction::Flh { .. }
            | Instruction::Flw { .. }
            | Instruction::Fld { .. }
            | Instruction::LrW { .. }
            | Instruction::LrD { .. }
    ) {
        events |= csr::HPM_EVENT_LOAD;
    }
    if matches!(
        instruction,
        Instruction::Sb { .. }
            | Instruction::Sh { .. }
            | Instruction::Sw { .. }
            | Instruction::Sd { .. }
            | Instruction::Fsh { .. }
            | Instruction::Fsw { .. }
            | Instruction::Fsd { .. }
            | Instruction::ScW { .. }
            | Instruction::ScD { .. }
            | Instruction::CboZero { .. }
            | Instruction::AmoSwapW { .. }
            | Instruction::AmoAddW { .. }
            | Instruction::AmoXorW { .. }
            | Instruction::AmoOrW { .. }
            | Instruction::AmoAndW { .. }
            | Instruction::AmoMinW { .. }
            | Instruction::AmoMaxW { .. }
            | Instruction::AmoMinuW { .. }
            | Instruction::AmoMaxuW { .. }
            | Instruction::AmoSwapD { .. }
            | Instruction::AmoAddD { .. }
            | Instruction::AmoXorD { .. }
            | Instruction::AmoOrD { .. }
            | Instruction::AmoAndD { .. }
            | Instruction::AmoMinD { .. }
            | Instruction::AmoMaxD { .. }
            | Instruction::AmoMinuD { .. }
            | Instruction::AmoMaxuD { .. }
    ) {
        events |= csr::HPM_EVENT_STORE;
    }
    if matches!(
        instruction,
        Instruction::Beq { .. }
            | Instruction::Bne { .. }
            | Instruction::Blt { .. }
            | Instruction::Bge { .. }
            | Instruction::Bltu { .. }
            | Instruction::Bgeu { .. }
            | Instruction::Jal { .. }
            | Instruction::Jalr { .. }
    ) {
        events |= csr::HPM_EVENT_BRANCH;
    }
    events
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cpu::RA;
    use crate::decode::decode;
    use crate::memory::{DRAM_BASE, Memory};

    const HANDLER: u64 = DRAM_BASE + 32;
    const SUPERVISOR_HANDLER: u64 = DRAM_BASE + 48;
    const COMPRESSED_ADD_X5_ONE: u16 = 0x4285;
    const COMPRESSED_ADDI_X5_ONE: u16 = 0x0285;
    const STANDARD_ADDI_X5_X0_THREE: u32 = 0x0030_0293;
    const COMPRESSED_J_SELF: u16 = 0xa001;
    const COMPRESSED_EBREAK: u16 = 0x9002;
    const ILLEGAL_COMPRESSED_ZERO: u16 = 0;
    const TEST_REGISTER_X5: usize = 5;
    const STANDARD_INSTRUCTION_BYTES: u64 = 4;
    const COUNTER_RELOAD_VALUE: u64 = 5;
    const COUNTER_CY_ENABLE: u64 = 1;
    const CYCLE_RELOAD_VALUE: u64 = 42;
    const HPM_COUNTER_3_BIT: u64 = 1 << 3;
    const MENVCFG_CBIE_FLUSH: u64 = 1 << 4;
    const MENVCFG_CBCFE: u64 = 1 << 6;
    const MENVCFG_CBZE: u64 = 1 << 7;
    const FP_INITIAL_STATUS: u64 = 1 << 13;
    const FP_DIRTY_STATUS: u64 = 0b11 << 13;
    const MSTATUS_SIE_BIT: u64 = 1 << 1;
    const MSTATUS_SPIE_BIT: u64 = 1 << 5;
    const MSTATUS_SPP_BIT: u64 = 1 << 8;
    const MSTATUS_MPP_SUPERVISOR: u64 = 1 << 11;
    const MSTATUS_TVM_BIT: u64 = 1 << 20;
    const MSTATUS_TW_BIT: u64 = 1 << 21;
    const MSTATUS_TSR_BIT: u64 = 1 << 22;
    const INTERRUPT_CAUSE_BIT: u64 = 1 << 63;
    const FADD_FT2_FT0_FT1: u32 = 0x0010_7153;
    const BOXED_ONE: u64 = 0xffff_ffff_3f80_0000;
    const BOXED_TWO: u64 = 0xffff_ffff_4000_0000;
    const INVALID_ROUNDING_MODE: u64 = 5;

    struct WriteFaultBus(Memory);

    impl Bus for WriteFaultBus {
        fn read_u8(&mut self, address: u64) -> Result<u8, Trap> {
            self.0.read_u8(address)
        }

        fn write_u8(&mut self, _address: u64, _value: u8) -> Result<(), Trap> {
            Err(Trap::AccessFault)
        }

        fn check_write(&mut self, _address: u64, _width: u64) -> Result<(), Trap> {
            Err(Trap::AccessFault)
        }
    }

    /// A hart on 64 bytes of memory, no program loaded.
    fn machine() -> Machine<Memory> {
        Machine::new(Memory::with_size(64))
    }

    /// A hart whose synchronous traps enter a known handler.
    fn trapped_machine() -> Machine<Memory> {
        let mut machine = machine();
        assert!(machine.cpu_mut().write_csr(inst::CSR_MTVEC, HANDLER));
        machine
    }

    fn assert_trap<B: Bus>(machine: &Machine<B>, cause: u8, tval: u64, epc: u64) {
        assert_eq!(
            machine.cpu().read_csr(inst::CSR_MCAUSE),
            Some(u64::from(cause)),
            "mcause"
        );
        assert_eq!(machine.cpu().read_csr(inst::CSR_MTVAL), Some(tval), "mtval");
        assert_eq!(machine.cpu().read_csr(inst::CSR_MEPC), Some(epc), "mepc");
        assert_eq!(machine.cpu().pc(), HANDLER, "handler PC");
    }

    fn assert_supervisor_trap<B: Bus>(machine: &Machine<B>, cause: u8, tval: u64, epc: u64) {
        assert_eq!(
            machine.cpu().read_csr(inst::CSR_SCAUSE),
            Some(u64::from(cause)),
            "scause"
        );
        assert_eq!(machine.cpu().read_csr(inst::CSR_STVAL), Some(tval), "stval");
        assert_eq!(machine.cpu().read_csr(inst::CSR_SEPC), Some(epc), "sepc");
        assert_eq!(machine.cpu().pc(), SUPERVISOR_HANDLER, "handler PC");
        assert_eq!(machine.cpu().privilege(), PrivilegeMode::Supervisor);
    }

    #[test]
    fn addi_sums_and_advances_the_pc() {
        let mut machine = machine();
        machine.cpu_mut().write_gpr(6, 40);

        machine.execute(Instruction::Addi {
            rd: 5,
            rs1: 6,
            imm: 2,
        });

        assert_eq!(machine.cpu().read_gpr(5), 42);
        assert_eq!(machine.cpu().pc(), 4);
    }

    #[test]
    fn floating_point_instruction_traps_when_fs_is_off() {
        let mut machine = trapped_machine();
        machine.cpu_mut().set_pc(DRAM_BASE);
        machine
            .bus_mut()
            .write_u32(DRAM_BASE, FADD_FT2_FT0_FT1)
            .unwrap();

        machine.step(StepOptions::default());

        assert_trap(
            &machine,
            inst::CAUSE_ILLEGAL_INSTRUCTION,
            u64::from(FADD_FT2_FT0_FT1),
            DRAM_BASE,
        );
    }

    #[test]
    fn enabled_floating_point_arithmetic_boxes_results_and_marks_fs_dirty() {
        let mut machine = machine();
        assert!(
            machine
                .cpu_mut()
                .write_csr(inst::CSR_MSTATUS, FP_INITIAL_STATUS)
        );
        machine.cpu_mut().write_fpr(0, BOXED_ONE);
        machine.cpu_mut().write_fpr(1, BOXED_ONE);
        assert!(
            machine
                .cpu_mut()
                .write_csr(inst::CSR_MSTATUS, FP_INITIAL_STATUS)
        );

        machine.execute(Instruction::FpBinary {
            format: FpFormat::Single,
            op: FpBinaryOp::Add,
            rd: 2,
            rs1: 0,
            rs2: 1,
            rounding: 0,
        });

        assert_eq!(machine.cpu().read_fpr(2), BOXED_TWO);
        assert_eq!(
            machine.cpu().read_csr(inst::CSR_MSTATUS).unwrap() & FP_DIRTY_STATUS,
            FP_DIRTY_STATUS
        );
    }

    #[test]
    fn invalid_dynamic_rounding_mode_is_illegal() {
        let mut machine = trapped_machine();
        assert!(
            machine
                .cpu_mut()
                .write_csr(inst::CSR_MSTATUS, FP_INITIAL_STATUS)
        );
        assert!(
            machine
                .cpu_mut()
                .write_csr(inst::CSR_FRM, INVALID_ROUNDING_MODE)
        );

        machine.execute(Instruction::FpBinary {
            format: FpFormat::Single,
            op: FpBinaryOp::Add,
            rd: 2,
            rs1: 0,
            rs2: 1,
            rounding: DYNAMIC_ROUNDING,
        });

        assert_trap(&machine, inst::CAUSE_ILLEGAL_INSTRUCTION, 0, 0);
    }

    #[test]
    fn widening_fp_conversions_validate_rounding_mode() {
        const RESERVED_ROUNDING_MODES: [u8; 2] = [5, 6];
        const DESTINATION_REGISTER: usize = 2;
        const UNCHANGED_DESTINATION: u64 = 0x0123_4567_89ab_cdef;
        let conversions = [
            (FpFormat::Half, FpFormat::Single),
            (FpFormat::Half, FpFormat::Double),
            (FpFormat::Single, FpFormat::Double),
        ];

        for (from, to) in conversions {
            for rounding in RESERVED_ROUNDING_MODES {
                let mut machine = trapped_machine();
                assert!(
                    machine
                        .cpu_mut()
                        .write_csr(inst::CSR_MSTATUS, FP_INITIAL_STATUS)
                );
                machine
                    .cpu_mut()
                    .write_fpr(DESTINATION_REGISTER, UNCHANGED_DESTINATION);
                assert!(
                    machine
                        .cpu_mut()
                        .write_csr(inst::CSR_MSTATUS, FP_INITIAL_STATUS)
                );

                machine.execute(Instruction::FpConvert {
                    from,
                    to,
                    rd: DESTINATION_REGISTER as u8,
                    rs1: 1,
                    rounding,
                });

                assert_trap(&machine, inst::CAUSE_ILLEGAL_INSTRUCTION, 0, 0);
                assert_eq!(
                    machine.cpu().read_fpr(DESTINATION_REGISTER),
                    UNCHANGED_DESTINATION
                );
            }

            let mut machine = trapped_machine();
            assert!(
                machine
                    .cpu_mut()
                    .write_csr(inst::CSR_MSTATUS, FP_INITIAL_STATUS)
            );
            assert!(
                machine
                    .cpu_mut()
                    .write_csr(inst::CSR_FRM, INVALID_ROUNDING_MODE)
            );
            assert!(
                machine
                    .cpu_mut()
                    .write_csr(inst::CSR_MSTATUS, FP_INITIAL_STATUS)
            );

            machine.execute(Instruction::FpConvert {
                from,
                to,
                rd: DESTINATION_REGISTER as u8,
                rs1: 1,
                rounding: DYNAMIC_ROUNDING,
            });

            assert_trap(&machine, inst::CAUSE_ILLEGAL_INSTRUCTION, 0, 0);
        }
    }

    #[test]
    fn widening_fp_conversions_ignore_valid_rounding_mode() {
        let conversions = [
            (
                FpFormat::Half,
                FpFormat::Single,
                HALF_NAN_BOX | 0x3e00,
                NAN_BOX | 0x3fc0_0000,
            ),
            (
                FpFormat::Half,
                FpFormat::Double,
                HALF_NAN_BOX | 0x3e00,
                0x3ff8_0000_0000_0000,
            ),
            (
                FpFormat::Single,
                FpFormat::Double,
                NAN_BOX | 0x3fc0_0000,
                0x3ff8_0000_0000_0000,
            ),
        ];

        for (from, to, source, expected) in conversions {
            let mut machine = machine();
            assert!(
                machine
                    .cpu_mut()
                    .write_csr(inst::CSR_MSTATUS, FP_INITIAL_STATUS)
            );
            machine.cpu_mut().write_fpr(1, source);

            machine.execute(Instruction::FpConvert {
                from,
                to,
                rd: 2,
                rs1: 1,
                rounding: 3,
            });

            assert_eq!(machine.cpu().read_fpr(2), expected);
            assert_eq!(machine.cpu().read_fcsr(), Some(0));
        }
    }

    #[test]
    fn addi_wraps_negative_immediates() {
        let mut machine = machine();

        machine.execute(Instruction::Addi {
            rd: 5,
            rs1: 0,
            imm: -1,
        });

        assert_eq!(machine.cpu().read_gpr(5), u64::MAX);
    }

    #[test]
    fn addi_to_x0_is_discarded() {
        let mut machine = machine();

        machine.execute(Instruction::Addi {
            rd: 0,
            rs1: 0,
            imm: 7,
        });

        assert_eq!(machine.cpu().read_gpr(0), 0);
        assert_eq!(machine.cpu().pc(), 4);
    }

    #[test]
    fn add_sums_two_registers() {
        let mut machine = machine();
        machine.cpu_mut().write_gpr(8, 3);
        machine.cpu_mut().write_gpr(9, 4);

        machine.execute(Instruction::Add {
            rd: 7,
            rs1: 8,
            rs2: 9,
        });

        assert_eq!(machine.cpu().read_gpr(7), 7);
        assert_eq!(machine.cpu().pc(), 4);
    }

    #[test]
    fn multiply_high_halves_obey_operand_signedness() {
        let mut machine = machine();
        machine.cpu_mut().write_gpr(5, i64::MIN as u64);
        machine.cpu_mut().write_gpr(6, 2);
        let cases = [
            (
                Instruction::Mul {
                    rd: 7,
                    rs1: 5,
                    rs2: 6,
                },
                0,
            ),
            (
                Instruction::Mulh {
                    rd: 7,
                    rs1: 5,
                    rs2: 6,
                },
                u64::MAX,
            ),
            (
                Instruction::Mulhsu {
                    rd: 7,
                    rs1: 5,
                    rs2: 6,
                },
                u64::MAX,
            ),
            (
                Instruction::Mulhu {
                    rd: 7,
                    rs1: 5,
                    rs2: 6,
                },
                1,
            ),
        ];

        for (instruction, expected) in cases {
            machine.execute(instruction);
            assert_eq!(machine.cpu().read_gpr(7), expected, "{instruction:?}");
        }
    }

    #[test]
    fn mulw_uses_the_low_word_and_sign_extends() {
        let mut machine = machine();
        machine.cpu_mut().write_gpr(5, 0x0000_0001_0000_0002);
        machine.cpu_mut().write_gpr(6, u64::MAX);

        machine.execute(Instruction::Mulw {
            rd: 7,
            rs1: 5,
            rs2: 6,
        });

        assert_eq!(machine.cpu().read_gpr(7), u64::MAX - 1);
    }

    #[test]
    fn divide_and_remainder_handle_signedness_zero_and_overflow() {
        let mut machine = machine();
        machine.cpu_mut().write_gpr(5, (-7_i64) as u64);
        machine.cpu_mut().write_gpr(6, 3);

        machine.execute(Instruction::Div {
            rd: 7,
            rs1: 5,
            rs2: 6,
        });
        machine.execute(Instruction::Rem {
            rd: 8,
            rs1: 5,
            rs2: 6,
        });
        machine.execute(Instruction::Divu {
            rd: 9,
            rs1: 5,
            rs2: 6,
        });
        machine.execute(Instruction::Remu {
            rd: 10,
            rs1: 5,
            rs2: 6,
        });
        assert_eq!(machine.cpu().read_gpr(7), (-2_i64) as u64);
        assert_eq!(machine.cpu().read_gpr(8), u64::MAX);
        assert_eq!(machine.cpu().read_gpr(9), (u64::MAX - 6) / 3);
        assert_eq!(machine.cpu().read_gpr(10), 0);

        machine.cpu_mut().write_gpr(5, i64::MIN as u64);
        machine.cpu_mut().write_gpr(6, u64::MAX);
        machine.execute(Instruction::Div {
            rd: 7,
            rs1: 5,
            rs2: 6,
        });
        machine.execute(Instruction::Rem {
            rd: 8,
            rs1: 5,
            rs2: 6,
        });
        assert_eq!(machine.cpu().read_gpr(7), i64::MIN as u64);
        assert_eq!(machine.cpu().read_gpr(8), 0);

        machine.cpu_mut().write_gpr(5, (-7_i64) as u64);
        machine.cpu_mut().write_gpr(6, 0);
        machine.execute(Instruction::Div {
            rd: 7,
            rs1: 5,
            rs2: 6,
        });
        machine.execute(Instruction::Rem {
            rd: 8,
            rs1: 5,
            rs2: 6,
        });
        machine.execute(Instruction::Divu {
            rd: 9,
            rs1: 5,
            rs2: 6,
        });
        machine.execute(Instruction::Remu {
            rd: 10,
            rs1: 5,
            rs2: 6,
        });
        assert_eq!(machine.cpu().read_gpr(7), u64::MAX);
        assert_eq!(machine.cpu().read_gpr(8), (-7_i64) as u64);
        assert_eq!(machine.cpu().read_gpr(9), u64::MAX);
        assert_eq!(machine.cpu().read_gpr(10), (-7_i64) as u64);
    }

    #[test]
    fn division_word_forms_sign_extend_and_handle_zero() {
        let mut machine = machine();
        machine.cpu_mut().write_gpr(5, 0x8000_0000);
        machine.cpu_mut().write_gpr(6, u64::MAX);

        machine.execute(Instruction::Divw {
            rd: 7,
            rs1: 5,
            rs2: 6,
        });
        machine.execute(Instruction::Remw {
            rd: 8,
            rs1: 5,
            rs2: 6,
        });
        assert_eq!(machine.cpu().read_gpr(7), 0xffff_ffff_8000_0000);
        assert_eq!(machine.cpu().read_gpr(8), 0);

        machine.cpu_mut().write_gpr(6, 0);
        machine.execute(Instruction::Divw {
            rd: 7,
            rs1: 5,
            rs2: 6,
        });
        machine.execute(Instruction::Divuw {
            rd: 8,
            rs1: 5,
            rs2: 6,
        });
        machine.execute(Instruction::Remw {
            rd: 9,
            rs1: 5,
            rs2: 6,
        });
        machine.execute(Instruction::Remuw {
            rd: 10,
            rs1: 5,
            rs2: 6,
        });
        assert_eq!(machine.cpu().read_gpr(7), u64::MAX);
        assert_eq!(machine.cpu().read_gpr(8), u64::MAX);
        assert_eq!(machine.cpu().read_gpr(9), 0xffff_ffff_8000_0000);
        assert_eq!(machine.cpu().read_gpr(10), 0xffff_ffff_8000_0000);
    }

    #[test]
    fn lr_sc_success_updates_memory_and_clears_the_reservation() {
        let mut machine = machine();
        machine.cpu_mut().write_gpr(5, DRAM_BASE);
        machine.cpu_mut().write_gpr(8, 0x1122_3344_5566_7788);
        machine
            .bus_mut()
            .write_u64(DRAM_BASE, 0x8877_6655_4433_2211)
            .unwrap();

        machine.execute(Instruction::LrD {
            rd: 7,
            rs1: 5,
            aq: true,
            rl: false,
        });
        assert_eq!(machine.cpu().read_gpr(7), 0x8877_6655_4433_2211);
        assert_eq!(machine.reservation, Some(DRAM_BASE));

        machine.execute(Instruction::ScD {
            rd: 9,
            rs1: 5,
            rs2: 8,
            aq: false,
            rl: true,
        });
        assert_eq!(machine.cpu().read_gpr(9), 0);
        assert_eq!(
            machine.bus_mut().read_u64(DRAM_BASE).unwrap(),
            0x1122_3344_5566_7788
        );
        assert_eq!(machine.reservation, None);
    }

    #[test]
    fn sc_succeeds_anywhere_in_the_reserved_doubleword() {
        let mut machine = machine();
        machine
            .bus_mut()
            .write_u32(DRAM_BASE + 4, 0x1122_3344)
            .unwrap();
        machine.cpu_mut().write_gpr(5, DRAM_BASE + 4);
        machine.cpu_mut().write_gpr(6, DRAM_BASE);
        machine.cpu_mut().write_gpr(7, 0x5566_7788);

        machine.execute(Instruction::LrW {
            rd: 8,
            rs1: 5,
            aq: false,
            rl: false,
        });
        machine.execute(Instruction::ScW {
            rd: 9,
            rs1: 6,
            rs2: 7,
            aq: false,
            rl: false,
        });

        assert_eq!(machine.cpu().read_gpr(8), 0x1122_3344);
        assert_eq!(machine.cpu().read_gpr(9), 0);
        assert_eq!(machine.bus_mut().read_u32(DRAM_BASE).unwrap(), 0x5566_7788);
    }

    #[test]
    fn sc_failure_preserves_memory_and_clears_the_reservation() {
        let mut machine = machine();
        machine.cpu_mut().write_gpr(5, DRAM_BASE);
        machine.cpu_mut().write_gpr(6, DRAM_BASE + 8);
        machine.cpu_mut().write_gpr(7, 0xdead_beef);
        machine
            .bus_mut()
            .write_u64(DRAM_BASE, 0x1122_3344_5566_7788)
            .unwrap();

        machine.execute(Instruction::LrD {
            rd: 8,
            rs1: 5,
            aq: false,
            rl: false,
        });
        machine.execute(Instruction::ScD {
            rd: 9,
            rs1: 6,
            rs2: 7,
            aq: false,
            rl: false,
        });
        machine.execute(Instruction::ScD {
            rd: 10,
            rs1: 5,
            rs2: 7,
            aq: false,
            rl: false,
        });

        assert_eq!(machine.cpu().read_gpr(9), 1);
        assert_eq!(machine.cpu().read_gpr(10), 1);
        assert_eq!(
            machine.bus_mut().read_u64(DRAM_BASE).unwrap(),
            0x1122_3344_5566_7788
        );
    }

    #[test]
    fn sc_failure_to_unmapped_address_traps_as_store_access() {
        for (instruction, address) in [
            (
                Instruction::ScW {
                    rd: 7,
                    rs1: 5,
                    rs2: 0,
                    aq: false,
                    rl: false,
                },
                DRAM_BASE - 8,
            ),
            (
                Instruction::ScD {
                    rd: 7,
                    rs1: 5,
                    rs2: 0,
                    aq: false,
                    rl: false,
                },
                DRAM_BASE - 8,
            ),
        ] {
            let mut machine = trapped_machine();
            machine.cpu_mut().set_pc(DRAM_BASE);
            machine.cpu_mut().write_gpr(5, address);
            machine.cpu_mut().write_gpr(7, 0x55);

            machine.execute(instruction);

            assert_eq!(machine.cpu().read_gpr(7), 0x55, "rd must not change");
            assert_trap(&machine, inst::CAUSE_STORE_ACCESS, address, DRAM_BASE);
        }
    }

    #[test]
    fn ordinary_store_invalidates_the_reservation() {
        let mut machine = machine();
        machine.cpu_mut().write_gpr(5, DRAM_BASE);
        machine.cpu_mut().write_gpr(6, 0x1234_5678);
        machine.execute(Instruction::LrD {
            rd: 7,
            rs1: 5,
            aq: false,
            rl: false,
        });
        machine.execute(Instruction::Sd {
            rs2: 6,
            rs1: 5,
            offset: 0,
        });
        machine.execute(Instruction::ScD {
            rd: 8,
            rs1: 5,
            rs2: 6,
            aq: false,
            rl: false,
        });

        assert_eq!(machine.cpu().read_gpr(8), 1);
    }

    #[test]
    fn misaligned_atomic_accesses_report_their_phase() {
        let mut load = trapped_machine();
        load.cpu_mut().set_pc(DRAM_BASE);
        load.cpu_mut().write_gpr(5, DRAM_BASE + 2);
        load.execute(Instruction::LrW {
            rd: 7,
            rs1: 5,
            aq: false,
            rl: false,
        });
        assert_trap(&load, inst::CAUSE_MISALIGNED_LOAD, DRAM_BASE + 2, DRAM_BASE);

        let mut store = trapped_machine();
        store.cpu_mut().set_pc(DRAM_BASE);
        store.cpu_mut().write_gpr(5, DRAM_BASE + 4);
        store.execute(Instruction::ScD {
            rd: 7,
            rs1: 5,
            rs2: 0,
            aq: false,
            rl: false,
        });
        assert_trap(
            &store,
            inst::CAUSE_MISALIGNED_STORE,
            DRAM_BASE + 4,
            DRAM_BASE,
        );
    }

    #[test]
    fn amo_min_max_operations_honor_their_signedness() {
        assert_eq!(apply_amo_w(0xffff_ffff, 1, AmoOperation::Min), 0xffff_ffff);
        assert_eq!(apply_amo_w(0xffff_ffff, 1, AmoOperation::Max), 1);
        assert_eq!(apply_amo_w(0xffff_ffff, 1, AmoOperation::Minu), 1);
        assert_eq!(apply_amo_w(0xffff_ffff, 1, AmoOperation::Maxu), 0xffff_ffff);
        assert_eq!(apply_amo_d(u64::MAX, 1, AmoOperation::Min), u64::MAX);
        assert_eq!(apply_amo_d(u64::MAX, 1, AmoOperation::Max), 1);
        assert_eq!(apply_amo_d(u64::MAX, 1, AmoOperation::Minu), 1);
        assert_eq!(apply_amo_d(u64::MAX, 1, AmoOperation::Maxu), u64::MAX);
    }

    #[test]
    fn amo_word_returns_the_sign_extended_old_value() {
        let mut machine = machine();
        machine.cpu_mut().write_gpr(5, DRAM_BASE);
        machine.cpu_mut().write_gpr(6, 1);
        machine.bus_mut().write_u32(DRAM_BASE, 0x8000_0000).unwrap();

        machine.execute(Instruction::AmoSwapW {
            rd: 7,
            rs1: 5,
            rs2: 6,
            aq: false,
            rl: false,
        });

        assert_eq!(machine.cpu().read_gpr(7), 0xffff_ffff_8000_0000);
        assert_eq!(machine.bus_mut().read_u32(DRAM_BASE).unwrap(), 1);
    }

    #[test]
    fn amo_write_invalidates_the_reservation() {
        let mut machine = machine();
        machine.cpu_mut().write_gpr(5, DRAM_BASE);
        machine.cpu_mut().write_gpr(6, 1);
        machine.execute(Instruction::LrD {
            rd: 7,
            rs1: 5,
            aq: false,
            rl: false,
        });
        machine.execute(Instruction::AmoAddD {
            rd: 8,
            rs1: 5,
            rs2: 6,
            aq: false,
            rl: false,
        });
        machine.execute(Instruction::ScD {
            rd: 9,
            rs1: 5,
            rs2: 6,
            aq: false,
            rl: false,
        });

        assert_eq!(machine.cpu().read_gpr(9), 1);
    }

    #[test]
    fn amo_alignment_faults_report_store_misaligned() {
        let mut misaligned = trapped_machine();
        misaligned.cpu_mut().set_pc(DRAM_BASE);
        misaligned.cpu_mut().write_gpr(5, DRAM_BASE + 2);
        misaligned.execute(Instruction::AmoAddW {
            rd: 7,
            rs1: 5,
            rs2: 0,
            aq: false,
            rl: false,
        });
        assert_trap(
            &misaligned,
            inst::CAUSE_MISALIGNED_STORE,
            DRAM_BASE + 2,
            DRAM_BASE,
        );
    }

    #[test]
    fn amo_read_faults_report_store_access() {
        for instruction in [
            Instruction::AmoAddW {
                rd: 7,
                rs1: 5,
                rs2: 0,
                aq: false,
                rl: false,
            },
            Instruction::AmoAddD {
                rd: 7,
                rs1: 5,
                rs2: 0,
                aq: false,
                rl: false,
            },
        ] {
            let mut machine = trapped_machine();
            machine.cpu_mut().set_pc(DRAM_BASE);
            machine.cpu_mut().write_gpr(5, DRAM_BASE - 8);
            machine.cpu_mut().write_gpr(7, 0x55);

            machine.execute(instruction);

            assert_eq!(machine.cpu().read_gpr(7), 0x55);
            assert_trap(&machine, inst::CAUSE_STORE_ACCESS, DRAM_BASE - 8, DRAM_BASE);
        }
    }

    #[test]
    fn atomic_write_faults_report_store_access_and_preserve_rd() {
        let mut memory = Memory::with_size(64);
        memory.write_u32(DRAM_BASE, 1).unwrap();
        let mut amo = Machine::new(WriteFaultBus(memory));
        assert!(amo.cpu_mut().write_csr(inst::CSR_MTVEC, HANDLER));
        amo.cpu_mut().set_pc(DRAM_BASE);
        amo.cpu_mut().write_gpr(5, DRAM_BASE);
        amo.cpu_mut().write_gpr(6, 2);
        amo.cpu_mut().write_gpr(7, 0x55);

        amo.execute(Instruction::AmoAddW {
            rd: 7,
            rs1: 5,
            rs2: 6,
            aq: false,
            rl: false,
        });

        assert_eq!(amo.cpu().read_gpr(7), 0x55);
        assert_trap(&amo, inst::CAUSE_STORE_ACCESS, DRAM_BASE, DRAM_BASE);

        let mut memory = Memory::with_size(64);
        memory.write_u64(DRAM_BASE, 1).unwrap();
        let mut sc = Machine::new(WriteFaultBus(memory));
        assert!(sc.cpu_mut().write_csr(inst::CSR_MTVEC, HANDLER));
        sc.cpu_mut().set_pc(DRAM_BASE);
        sc.cpu_mut().write_gpr(5, DRAM_BASE);
        sc.cpu_mut().write_gpr(6, 2);
        sc.execute(Instruction::LrD {
            rd: 7,
            rs1: 5,
            aq: false,
            rl: false,
        });
        sc.cpu_mut().write_gpr(7, 0x55);

        sc.execute(Instruction::ScD {
            rd: 7,
            rs1: 5,
            rs2: 6,
            aq: false,
            rl: false,
        });

        assert_eq!(sc.cpu().read_gpr(7), 0x55);
        assert_trap(&sc, inst::CAUSE_STORE_ACCESS, DRAM_BASE, DRAM_BASE + 4);
    }

    #[test]
    fn beq_taken_branches_from_the_instruction_address() {
        let mut machine = machine();
        machine.cpu_mut().write_gpr(10, 5);
        machine.cpu_mut().write_gpr(11, 5);
        machine.cpu_mut().set_pc(DRAM_BASE + 8);

        machine.execute(Instruction::Beq {
            rs1: 10,
            rs2: 11,
            offset: -8,
        });

        assert_eq!(machine.cpu().pc(), DRAM_BASE);
    }

    #[test]
    fn beq_not_taken_advances_the_pc() {
        let mut machine = machine();
        machine.cpu_mut().write_gpr(10, 5);
        machine.cpu_mut().write_gpr(11, 6);

        machine.execute(Instruction::Beq {
            rs1: 10,
            rs2: 11,
            offset: 16,
        });

        assert_eq!(machine.cpu().pc(), 4);
    }

    #[test]
    fn jal_links_and_jumps() {
        let mut machine = machine();
        machine.cpu_mut().set_pc(DRAM_BASE + 12);

        machine.execute(Instruction::Jal { rd: 1, offset: -12 });

        assert_eq!(machine.cpu().read_gpr(1), DRAM_BASE + 16);
        assert_eq!(machine.cpu().pc(), DRAM_BASE);
    }

    #[test]
    fn halt_self_loop_marks_the_hart_halted() {
        let mut machine = machine();
        machine.cpu_mut().set_pc(DRAM_BASE);

        machine.execute(Instruction::Jal { rd: 0, offset: 0 });

        assert!(machine.halted());
        assert_eq!(machine.cpu().pc(), DRAM_BASE);
    }

    #[test]
    fn fetch_outside_memory_enters_the_trap_handler() {
        let mut machine = trapped_machine();
        machine.cpu_mut().set_pc(DRAM_BASE - 4);

        machine.step(StepOptions::default());

        assert_trap(
            &machine,
            inst::CAUSE_FETCH_ACCESS,
            DRAM_BASE - 4,
            DRAM_BASE - 4,
        );
    }

    #[test]
    fn compressed_instruction_advances_pc_by_two() {
        let mut machine = machine();
        machine
            .bus_mut()
            .write_u16(DRAM_BASE, COMPRESSED_ADD_X5_ONE)
            .unwrap();
        machine.cpu_mut().set_pc(DRAM_BASE);

        machine.step(StepOptions::default());

        assert_eq!(machine.cpu().read_gpr(TEST_REGISTER_X5), 1);
        assert_eq!(machine.cpu().pc(), DRAM_BASE + MIN_INSTRUCTION_BYTES);
    }

    #[test]
    fn standard_instruction_can_follow_a_compressed_instruction() {
        let mut machine = machine();
        machine
            .bus_mut()
            .write_u16(DRAM_BASE, COMPRESSED_ADDI_X5_ONE)
            .unwrap();
        machine
            .bus_mut()
            .write_u32(DRAM_BASE + MIN_INSTRUCTION_BYTES, STANDARD_ADDI_X5_X0_THREE)
            .unwrap();
        machine.cpu_mut().set_pc(DRAM_BASE);

        machine.step(StepOptions::default());
        machine.step(StepOptions::default());

        assert_eq!(machine.cpu().read_gpr(TEST_REGISTER_X5), 3);
        assert_eq!(
            machine.cpu().pc(),
            DRAM_BASE + MIN_INSTRUCTION_BYTES + STANDARD_INSTRUCTION_BYTES
        );
    }

    #[test]
    fn compressed_jump_to_itself_runs_to_the_step_limit() {
        let mut machine = machine();
        machine
            .bus_mut()
            .write_u16(DRAM_BASE, COMPRESSED_J_SELF)
            .unwrap();
        machine.cpu_mut().set_pc(DRAM_BASE);

        assert_eq!(machine.run(3), RunOutcome::StepLimitReached);
        assert!(!machine.halted());
        assert_eq!(machine.cpu().pc(), DRAM_BASE);
    }

    #[test]
    fn jalr_clears_bit_zero_so_every_target_is_aligned() {
        let mut machine = machine();
        machine.cpu_mut().write_gpr(6, DRAM_BASE + 3);

        machine.execute(Instruction::Jalr {
            rd: RA,
            rs1: 6,
            offset: 0,
        });

        assert_eq!(machine.cpu().read_gpr(usize::from(RA)), 4);
        assert_eq!(machine.cpu().pc(), DRAM_BASE + MIN_INSTRUCTION_BYTES);
    }

    #[test]
    fn jal_to_a_misaligned_target_traps_without_linking() {
        let mut machine = trapped_machine();
        machine.cpu_mut().write_gpr(5, 7);

        machine.execute(Instruction::Jal { rd: 5, offset: 3 });

        assert_eq!(machine.cpu().read_gpr(5), 7, "rd must not change");
        assert_trap(&machine, inst::CAUSE_MISALIGNED_FETCH, 3, 0);
    }

    #[test]
    fn jalr_links_and_clears_bit_zero() {
        let mut machine = machine();
        machine.cpu_mut().set_pc(DRAM_BASE);
        machine.cpu_mut().write_gpr(6, DRAM_BASE + 8 + 1);

        machine.execute(Instruction::Jalr {
            rd: 5,
            rs1: 6,
            offset: 0,
        });

        assert_eq!(machine.cpu().read_gpr(5), DRAM_BASE + 4);
        assert_eq!(machine.cpu().pc(), DRAM_BASE + 8);
    }

    #[test]
    fn each_branch_condition_decides_as_the_spec_says() {
        const TAKEN: u64 = DRAM_BASE + 16;
        const NEXT: u64 = DRAM_BASE + 4;
        let neg_one = u64::MAX;
        let cases = [
            (
                Instruction::Beq {
                    rs1: 5,
                    rs2: 6,
                    offset: 16,
                },
                1,
                1,
                TAKEN,
            ),
            (
                Instruction::Beq {
                    rs1: 5,
                    rs2: 6,
                    offset: 16,
                },
                1,
                2,
                NEXT,
            ),
            (
                Instruction::Bne {
                    rs1: 5,
                    rs2: 6,
                    offset: 16,
                },
                1,
                2,
                TAKEN,
            ),
            (
                Instruction::Bne {
                    rs1: 5,
                    rs2: 6,
                    offset: 16,
                },
                1,
                1,
                NEXT,
            ),
            (
                Instruction::Blt {
                    rs1: 5,
                    rs2: 6,
                    offset: 16,
                },
                neg_one,
                1,
                TAKEN,
            ),
            (
                Instruction::Blt {
                    rs1: 5,
                    rs2: 6,
                    offset: 16,
                },
                1,
                neg_one,
                NEXT,
            ),
            (
                Instruction::Bge {
                    rs1: 5,
                    rs2: 6,
                    offset: 16,
                },
                1,
                neg_one,
                TAKEN,
            ),
            (
                Instruction::Bge {
                    rs1: 5,
                    rs2: 6,
                    offset: 16,
                },
                neg_one,
                1,
                NEXT,
            ),
            (
                Instruction::Bltu {
                    rs1: 5,
                    rs2: 6,
                    offset: 16,
                },
                1,
                2,
                TAKEN,
            ),
            (
                Instruction::Bltu {
                    rs1: 5,
                    rs2: 6,
                    offset: 16,
                },
                neg_one,
                1,
                NEXT,
            ),
            (
                Instruction::Bgeu {
                    rs1: 5,
                    rs2: 6,
                    offset: 16,
                },
                2,
                1,
                TAKEN,
            ),
            (
                Instruction::Bgeu {
                    rs1: 5,
                    rs2: 6,
                    offset: 16,
                },
                1,
                neg_one,
                NEXT,
            ),
        ];

        for (instruction, left, right, expected_pc) in cases {
            let mut machine = machine();
            machine.cpu_mut().set_pc(DRAM_BASE);
            machine.cpu_mut().write_gpr(5, left);
            machine.cpu_mut().write_gpr(6, right);

            machine.execute(instruction);

            assert_eq!(
                machine.cpu().pc(),
                expected_pc,
                "{instruction:?} with {left:#x} and {right:#x}"
            );
        }
    }

    #[test]
    fn taken_branch_to_a_misaligned_target_traps() {
        let mut machine = trapped_machine();
        machine.cpu_mut().write_gpr(5, 1);
        machine.cpu_mut().write_gpr(6, 2);

        machine.execute(Instruction::Bne {
            rs1: 5,
            rs2: 6,
            offset: 3,
        });

        assert_trap(&machine, inst::CAUSE_MISALIGNED_FETCH, 3, 0);
    }

    #[test]
    fn store_then_load_round_trips() {
        let mut machine = machine();
        machine.cpu_mut().write_gpr(5, DRAM_BASE);
        machine.cpu_mut().write_gpr(6, 0xdead_beef);

        machine.execute(Instruction::Sw {
            rs2: 6,
            rs1: 5,
            offset: 8,
        });
        machine.execute(Instruction::Lwu {
            rd: 7,
            rs1: 5,
            offset: 8,
        });

        assert_eq!(machine.cpu().read_gpr(7), 0xdead_beef);
    }

    #[test]
    fn loads_sign_or_zero_extend() {
        let mut machine = machine();
        machine.cpu_mut().write_gpr(5, DRAM_BASE);
        machine.cpu_mut().write_gpr(6, u64::MAX);
        machine.execute(Instruction::Sd {
            rs2: 6,
            rs1: 5,
            offset: 0,
        });

        let loads = [
            (
                Instruction::Lb {
                    rd: 7,
                    rs1: 5,
                    offset: 0,
                },
                u64::MAX,
            ),
            (
                Instruction::Lbu {
                    rd: 7,
                    rs1: 5,
                    offset: 0,
                },
                0xff,
            ),
            (
                Instruction::Lh {
                    rd: 7,
                    rs1: 5,
                    offset: 0,
                },
                u64::MAX,
            ),
            (
                Instruction::Lhu {
                    rd: 7,
                    rs1: 5,
                    offset: 0,
                },
                0xffff,
            ),
            (
                Instruction::Lw {
                    rd: 7,
                    rs1: 5,
                    offset: 0,
                },
                u64::MAX,
            ),
            (
                Instruction::Lwu {
                    rd: 7,
                    rs1: 5,
                    offset: 0,
                },
                0xffff_ffff,
            ),
            (
                Instruction::Ld {
                    rd: 7,
                    rs1: 5,
                    offset: 0,
                },
                u64::MAX,
            ),
        ];

        for (load, expected) in loads {
            machine.cpu_mut().write_gpr(7, 0);
            machine.execute(load);
            assert_eq!(machine.cpu().read_gpr(7), expected, "{load:?}");
        }
    }

    #[test]
    fn out_of_range_load_traps_without_touching_rd() {
        let mut machine = trapped_machine();
        machine.cpu_mut().write_gpr(5, 7);
        machine.cpu_mut().write_gpr(6, DRAM_BASE + 1024);

        machine.execute(Instruction::Lw {
            rd: 5,
            rs1: 6,
            offset: 0,
        });

        assert_eq!(machine.cpu().read_gpr(5), 7, "rd must not change");
        assert_trap(&machine, inst::CAUSE_LOAD_ACCESS, DRAM_BASE + 1024, 0);
    }

    #[test]
    fn out_of_range_store_enters_the_trap_handler() {
        let mut machine = trapped_machine();
        machine.cpu_mut().write_gpr(5, 1);
        machine.cpu_mut().write_gpr(6, DRAM_BASE + 1024);

        machine.execute(Instruction::Sw {
            rs2: 5,
            rs1: 6,
            offset: 0,
        });

        assert_trap(&machine, inst::CAUSE_STORE_ACCESS, DRAM_BASE + 1024, 0);
    }

    #[test]
    fn csr_operations_return_the_old_value_and_apply_each_update() {
        let mut machine = trapped_machine();

        machine.execute(Instruction::Csrrwi {
            rd: 5,
            csr: inst::CSR_MSCRATCH,
            zimm: 1,
        });
        assert_eq!(machine.cpu().read_gpr(5), 0);
        assert_eq!(machine.cpu().read_csr(inst::CSR_MSCRATCH), Some(1));

        machine.execute(Instruction::Csrrsi {
            rd: 6,
            csr: inst::CSR_MSCRATCH,
            zimm: 2,
        });
        assert_eq!(machine.cpu().read_gpr(6), 1);
        assert_eq!(machine.cpu().read_csr(inst::CSR_MSCRATCH), Some(3));

        machine.execute(Instruction::Csrrci {
            rd: 7,
            csr: inst::CSR_MSCRATCH,
            zimm: 1,
        });
        assert_eq!(machine.cpu().read_gpr(7), 3);
        assert_eq!(machine.cpu().read_csr(inst::CSR_MSCRATCH), Some(2));

        machine.execute(Instruction::Csrrs {
            rd: 8,
            csr: inst::CSR_MSCRATCH,
            rs1: 0,
        });
        assert_eq!(machine.cpu().read_gpr(8), 2);
        assert_eq!(machine.cpu().read_csr(inst::CSR_MSCRATCH), Some(2));

        machine.cpu_mut().write_gpr(9, 2);
        machine.execute(Instruction::Csrrc {
            rd: 10,
            csr: inst::CSR_MSCRATCH,
            rs1: 9,
        });
        assert_eq!(machine.cpu().read_gpr(10), 2);
        assert_eq!(machine.cpu().read_csr(inst::CSR_MSCRATCH), Some(0));

        machine.execute(Instruction::Csrrw {
            rd: 11,
            csr: inst::CSR_MSCRATCH,
            rs1: 9,
        });
        assert_eq!(machine.cpu().read_gpr(11), 0);
        assert_eq!(machine.cpu().read_csr(inst::CSR_MSCRATCH), Some(2));
        assert_eq!(machine.cpu().pc(), 24);
    }

    #[test]
    fn csr_read_modify_write_uses_the_old_source_register_value() {
        let mut machine = trapped_machine();
        machine.cpu_mut().write_gpr(5, 1);

        machine.execute(Instruction::Csrrs {
            rd: 5,
            csr: inst::CSR_MSCRATCH,
            rs1: 5,
        });

        assert_eq!(machine.cpu().read_gpr(5), 0);
        assert_eq!(machine.cpu().read_csr(inst::CSR_MSCRATCH), Some(1));
    }

    #[test]
    fn misa_reports_rv64imafdcbsu_and_ignores_warl_writes() {
        let mut machine = trapped_machine();
        machine.execute(Instruction::Csrrs {
            rd: 5,
            csr: inst::CSR_MISA,
            rs1: 0,
        });
        let misa = machine.cpu().read_gpr(5);
        assert_eq!(misa >> 62, 2, "MXL");
        assert_ne!(misa & 1, 0, "A extension");
        assert_ne!(misa & (1 << 3), 0, "D extension");
        assert_ne!(misa & (1 << 5), 0, "F extension");
        assert_ne!(misa & (1 << 8), 0, "I extension");
        assert_ne!(misa & (1 << 12), 0, "M extension");
        assert_ne!(misa & (1 << 18), 0, "S extension");

        machine.cpu_mut().write_gpr(5, 7);
        machine.cpu_mut().write_gpr(6, 0);
        machine.execute(Instruction::Csrrw {
            rd: 5,
            csr: inst::CSR_MISA,
            rs1: 6,
        });

        assert_eq!(
            machine.cpu().read_gpr(5),
            misa,
            "csrrw returns the old value"
        );
        assert_eq!(machine.cpu().read_csr(inst::CSR_MISA), Some(misa));
        assert_eq!(machine.cpu().pc(), 8);
    }

    #[test]
    fn csr_instruction_words_round_trip_through_generated_encodings() {
        for word in [
            0x3003_12f3,
            0x3053_22f3,
            0x3413_32f3,
            0x3422_52f3,
            0x3431_62f3,
            0x3400_f2f3,
        ] {
            let instruction = decode(word).unwrap();
            assert_eq!(instruction_word(instruction), word);
        }
    }

    #[test]
    fn system_instruction_words_round_trip_through_generated_encodings() {
        const SFENCE_RS1: u8 = 5;
        const SFENCE_RS2: u8 = 6;
        const RS1_FIELD_SHIFT: u32 = 15;
        const RS2_FIELD_SHIFT: u32 = 20;
        let cases = [
            (Instruction::Ecall, inst::MATCH_ECALL),
            (Instruction::Ebreak, inst::MATCH_EBREAK),
            (Instruction::Mret, inst::MATCH_MRET),
            (Instruction::Sret, inst::MATCH_SRET),
            (Instruction::Wfi, inst::MATCH_WFI),
            (
                Instruction::SfenceVma {
                    rs1: SFENCE_RS1,
                    rs2: SFENCE_RS2,
                },
                inst::MATCH_SFENCE_VMA
                    | (u32::from(SFENCE_RS1) << RS1_FIELD_SHIFT)
                    | (u32::from(SFENCE_RS2) << RS2_FIELD_SHIFT),
            ),
        ];

        for (instruction, expected) in cases {
            assert_eq!(instruction_word(instruction), expected);
            assert_eq!(decode(expected), Ok(instruction));
        }
    }

    #[test]
    fn ecall_enters_the_machine_trap_handler_precisely() {
        let mut machine = trapped_machine();
        machine.cpu_mut().set_pc(DRAM_BASE + 4);
        assert!(machine.cpu_mut().write_csr(inst::CSR_MSTATUS, 1 << 3));

        machine.execute(Instruction::Ecall);

        assert_trap(&machine, inst::CAUSE_MACHINE_ECALL, 0, DRAM_BASE + 4);
        let status = machine.cpu().csrs().read(inst::CSR_MSTATUS).unwrap();
        assert_eq!(status & (1 << 3), 0, "MIE is cleared on trap entry");
        assert_eq!(status & (1 << 7), 1 << 7, "MIE is saved in MPIE");
        assert_eq!(status & (0b11 << 11), 0b11 << 11, "MPP records M-mode");
    }

    #[test]
    fn delegated_user_ecall_enters_machine_handler_without_medeleg() {
        let mut machine = trapped_machine();
        assert!(
            machine
                .cpu_mut()
                .write_csr(inst::CSR_STVEC, SUPERVISOR_HANDLER)
        );
        machine.cpu_mut().set_privilege(PrivilegeMode::User);
        machine.cpu_mut().set_pc(DRAM_BASE + 4);

        machine.execute(Instruction::Ecall);

        assert_trap(&machine, inst::CAUSE_USER_ECALL, 0, DRAM_BASE + 4);
        let status = machine.cpu().csrs().read(inst::CSR_MSTATUS).unwrap();
        assert_eq!(status & (0b11 << 11), 0, "MPP records U-mode");
    }

    #[test]
    fn delegated_supervisor_ecall_enters_supervisor_handler() {
        let mut machine = trapped_machine();
        assert!(
            machine
                .cpu_mut()
                .write_csr(inst::CSR_STVEC, SUPERVISOR_HANDLER)
        );
        assert!(
            machine
                .cpu_mut()
                .write_csr(inst::CSR_MEDELEG, 1 << inst::CAUSE_SUPERVISOR_ECALL)
        );
        assert!(
            machine
                .cpu_mut()
                .write_csr(inst::CSR_MSTATUS, MSTATUS_SIE_BIT)
        );
        machine.cpu_mut().set_privilege(PrivilegeMode::Supervisor);
        machine.cpu_mut().set_pc(DRAM_BASE + 4);

        machine.execute(Instruction::Ecall);

        assert_supervisor_trap(&machine, inst::CAUSE_SUPERVISOR_ECALL, 0, DRAM_BASE + 4);
        let status = machine.cpu().csrs().read(inst::CSR_MSTATUS).unwrap();
        assert_eq!(status & MSTATUS_SIE_BIT, 0);
        assert_eq!(status & MSTATUS_SPIE_BIT, MSTATUS_SPIE_BIT);
        assert_eq!(status & MSTATUS_SPP_BIT, MSTATUS_SPP_BIT);
    }

    #[test]
    fn delegated_user_ecall_uses_supervisor_handler() {
        let mut machine = trapped_machine();
        assert!(
            machine
                .cpu_mut()
                .write_csr(inst::CSR_STVEC, SUPERVISOR_HANDLER)
        );
        assert!(
            machine
                .cpu_mut()
                .write_csr(inst::CSR_MEDELEG, 1 << inst::CAUSE_USER_ECALL)
        );
        machine.cpu_mut().set_privilege(PrivilegeMode::User);
        machine.cpu_mut().set_pc(DRAM_BASE + 8);

        machine.execute(Instruction::Ecall);

        assert_supervisor_trap(&machine, inst::CAUSE_USER_ECALL, 0, DRAM_BASE + 8);
        let status = machine.cpu().csrs().read(inst::CSR_MSTATUS).unwrap();
        assert_eq!(status & MSTATUS_SPP_BIT, 0);
    }

    #[test]
    fn software_pending_supervisor_timer_interrupt_enters_supervisor_handler() {
        const SUPERVISOR_TIMER_INTERRUPT: u64 = 5;
        const STIP: u64 = 1 << SUPERVISOR_TIMER_INTERRUPT;

        let mut machine = trapped_machine();
        assert!(
            machine
                .cpu_mut()
                .write_csr(inst::CSR_STVEC, SUPERVISOR_HANDLER)
        );
        assert!(
            machine
                .cpu_mut()
                .write_csr(inst::CSR_MSTATUS, MSTATUS_SIE_BIT)
        );
        assert!(machine.cpu_mut().write_csr(inst::CSR_MIDELEG, STIP));
        assert!(machine.cpu_mut().write_csr(inst::CSR_MIE, STIP));
        assert!(machine.cpu_mut().write_csr(inst::CSR_MIP, STIP));
        machine.cpu_mut().set_privilege(PrivilegeMode::Supervisor);
        machine.cpu_mut().set_pc(DRAM_BASE + 16);

        machine.step(StepOptions::default());

        assert_eq!(
            machine.cpu().read_csr(inst::CSR_SCAUSE),
            Some(INTERRUPT_CAUSE_BIT | SUPERVISOR_TIMER_INTERRUPT)
        );
        assert_eq!(machine.cpu().read_csr(inst::CSR_STVAL), Some(0));
        assert_eq!(machine.cpu().read_csr(inst::CSR_SEPC), Some(DRAM_BASE + 16));
        assert_eq!(machine.cpu().pc(), SUPERVISOR_HANDLER);
        assert_eq!(machine.cpu().privilege(), PrivilegeMode::Supervisor);
    }

    #[test]
    fn sret_restores_supervisor_state_and_target() {
        let mut machine = machine();
        assert!(
            machine
                .cpu_mut()
                .write_csr(inst::CSR_MSTATUS, MSTATUS_SPIE_BIT | MSTATUS_SPP_BIT)
        );
        assert!(machine.cpu_mut().write_csr(inst::CSR_SEPC, DRAM_BASE + 16));
        machine.cpu_mut().set_privilege(PrivilegeMode::Supervisor);

        machine.execute(Instruction::Sret);

        assert_eq!(machine.cpu().pc(), DRAM_BASE + 16);
        assert_eq!(machine.cpu().privilege(), PrivilegeMode::Supervisor);
        let status = machine.cpu().csrs().read(inst::CSR_MSTATUS).unwrap();
        assert_eq!(status & MSTATUS_SIE_BIT, MSTATUS_SIE_BIT);
        assert_eq!(status & MSTATUS_SPIE_BIT, MSTATUS_SPIE_BIT);
        assert_eq!(status & MSTATUS_SPP_BIT, 0);
    }

    #[test]
    fn privileged_instruction_controls_trap_in_supervisor_mode() {
        for (instruction, status, privilege, expected_tval) in [
            (
                Instruction::Sret,
                MSTATUS_TSR_BIT,
                PrivilegeMode::Supervisor,
                inst::MATCH_SRET,
            ),
            (
                Instruction::Wfi,
                MSTATUS_TW_BIT,
                PrivilegeMode::Supervisor,
                inst::MATCH_WFI,
            ),
            (Instruction::Sret, 0, PrivilegeMode::User, inst::MATCH_SRET),
            (Instruction::Wfi, 0, PrivilegeMode::User, inst::MATCH_WFI),
        ] {
            let mut machine = trapped_machine();
            assert!(machine.cpu_mut().write_csr(inst::CSR_MSTATUS, status));
            machine.cpu_mut().set_privilege(privilege);

            machine.execute(instruction);

            assert_trap(
                &machine,
                inst::CAUSE_ILLEGAL_INSTRUCTION,
                u64::from(expected_tval),
                0,
            );
        }
    }

    #[test]
    fn delegated_illegal_instruction_reports_encoding_in_stval() {
        let mut machine = trapped_machine();
        assert!(
            machine
                .cpu_mut()
                .write_csr(inst::CSR_STVEC, SUPERVISOR_HANDLER)
        );
        assert!(
            machine
                .cpu_mut()
                .write_csr(inst::CSR_MEDELEG, 1 << inst::CAUSE_ILLEGAL_INSTRUCTION)
        );
        machine.cpu_mut().set_privilege(PrivilegeMode::User);

        machine.execute(Instruction::Sret);

        assert_supervisor_trap(
            &machine,
            inst::CAUSE_ILLEGAL_INSTRUCTION,
            u64::from(inst::MATCH_SRET),
            0,
        );
    }

    #[test]
    fn wfi_is_a_no_op_without_interrupts() {
        let mut machine = machine();
        machine.cpu_mut().set_privilege(PrivilegeMode::Supervisor);

        machine.execute(Instruction::Wfi);

        assert_eq!(machine.cpu().pc(), 4);
        assert_eq!(machine.cpu().privilege(), PrivilegeMode::Supervisor);
    }

    #[test]
    fn supervisor_satp_is_warl_bare_and_tvm_is_read_only_zero() {
        let mut machine = machine();
        assert!(
            machine
                .cpu_mut()
                .write_csr(inst::CSR_MSTATUS, MSTATUS_TVM_BIT)
        );
        assert_eq!(
            machine.cpu().read_csr(inst::CSR_MSTATUS).unwrap() & MSTATUS_TVM_BIT,
            0
        );
        machine.cpu_mut().set_privilege(PrivilegeMode::Supervisor);

        let instruction = Instruction::Csrrw {
            rd: 5,
            csr: inst::CSR_SATP,
            rs1: 1,
        };
        machine.cpu_mut().write_gpr(1, u64::MAX);

        machine.execute(instruction);

        assert_eq!(machine.cpu().read_gpr(5), 0);
        assert_eq!(machine.cpu().read_csr(inst::CSR_SATP), Some(0));
        assert_eq!(machine.cpu().pc(), 4);
    }

    #[test]
    fn sfence_vma_traps_in_supervisor_mode_when_satp_is_read_only_bare() {
        let mut supervisor_machine = trapped_machine();
        assert!(
            supervisor_machine
                .cpu_mut()
                .write_csr(inst::CSR_MSTATUS, MSTATUS_TVM_BIT)
        );
        supervisor_machine
            .cpu_mut()
            .set_privilege(PrivilegeMode::Supervisor);

        supervisor_machine.execute(Instruction::SfenceVma { rs1: 0, rs2: 0 });

        assert_trap(
            &supervisor_machine,
            inst::CAUSE_ILLEGAL_INSTRUCTION,
            u64::from(inst::MATCH_SFENCE_VMA),
            0,
        );

        let mut machine_mode = machine();
        machine_mode.execute(Instruction::SfenceVma { rs1: 0, rs2: 0 });
        assert_eq!(machine_mode.cpu().pc(), 4);
        assert_eq!(machine_mode.cpu().privilege(), PrivilegeMode::Machine);
    }

    #[test]
    fn ebreak_records_breakpoint_and_its_address() {
        let mut machine = trapped_machine();
        machine.cpu_mut().set_pc(DRAM_BASE + 8);

        machine.execute(Instruction::Ebreak);

        assert_trap(
            &machine,
            inst::CAUSE_BREAKPOINT,
            DRAM_BASE + 8,
            DRAM_BASE + 8,
        );
    }

    #[test]
    fn mret_restores_interrupt_state_and_returns_to_mepc() {
        let mut machine = trapped_machine();
        assert!(machine.cpu_mut().write_csr(inst::CSR_MSTATUS, 1 << 7));
        assert!(machine.cpu_mut().write_csr(inst::CSR_MEPC, DRAM_BASE + 12));

        machine.execute(Instruction::Mret);

        assert_eq!(machine.cpu().pc(), DRAM_BASE + 12);
        assert_eq!(machine.cpu().privilege(), PrivilegeMode::User);
        let status = machine.cpu().csrs().read(inst::CSR_MSTATUS).unwrap();
        assert_eq!(status & (1 << 3), 1 << 3, "MIE is restored from MPIE");
        assert_eq!(status & (1 << 7), 1 << 7, "MPIE is set");
        assert_eq!(status & (0b11 << 11), 0, "MPP is set to U-mode");
    }

    #[test]
    fn mret_can_enter_supervisor_mode() {
        let mut machine = machine();
        assert!(
            machine
                .cpu_mut()
                .write_csr(inst::CSR_MSTATUS, MSTATUS_MPP_SUPERVISOR)
        );
        assert!(machine.cpu_mut().write_csr(inst::CSR_MEPC, DRAM_BASE + 12));

        machine.execute(Instruction::Mret);

        assert_eq!(machine.cpu().pc(), DRAM_BASE + 12);
        assert_eq!(machine.cpu().privilege(), PrivilegeMode::Supervisor);
    }

    #[test]
    fn illegal_instruction_fetch_enters_the_trap_handler() {
        let mut machine = trapped_machine();
        machine.cpu_mut().set_pc(DRAM_BASE);
        machine
            .bus_mut()
            .write_u32(DRAM_BASE, inst::MATCH_AMOCAS_D)
            .unwrap();

        machine.step(StepOptions::default());

        assert_trap(
            &machine,
            inst::CAUSE_ILLEGAL_INSTRUCTION,
            u64::from(inst::MATCH_AMOCAS_D),
            DRAM_BASE,
        );
    }

    #[test]
    fn illegal_compressed_instruction_reports_its_zero_extended_bits() {
        let mut machine = trapped_machine();
        machine.cpu_mut().set_pc(DRAM_BASE + MIN_INSTRUCTION_BYTES);
        machine
            .bus_mut()
            .write_u16(DRAM_BASE + MIN_INSTRUCTION_BYTES, ILLEGAL_COMPRESSED_ZERO)
            .unwrap();

        machine.step(StepOptions::default());

        assert_trap(
            &machine,
            inst::CAUSE_ILLEGAL_INSTRUCTION,
            u64::from(ILLEGAL_COMPRESSED_ZERO),
            DRAM_BASE + MIN_INSTRUCTION_BYTES,
        );
    }

    #[test]
    fn compressed_ebreak_reports_its_instruction_address() {
        let mut machine = trapped_machine();
        machine.cpu_mut().set_pc(DRAM_BASE + MIN_INSTRUCTION_BYTES);
        machine
            .bus_mut()
            .write_u16(DRAM_BASE + MIN_INSTRUCTION_BYTES, COMPRESSED_EBREAK)
            .unwrap();

        machine.step(StepOptions::default());

        assert_trap(
            &machine,
            inst::CAUSE_BREAKPOINT,
            DRAM_BASE + MIN_INSTRUCTION_BYTES,
            DRAM_BASE + MIN_INSTRUCTION_BYTES,
        );
    }

    #[test]
    fn unsupported_csr_traps_before_writing_rd_or_csr_state() {
        let instruction = Instruction::Csrrs {
            rd: 5,
            csr: inst::CSR_MCYCLEH,
            rs1: 0,
        };
        let mut machine = trapped_machine();
        machine.cpu_mut().write_gpr(5, 7);

        machine.execute(instruction);

        assert_eq!(machine.cpu().read_gpr(5), 7);
        assert_trap(
            &machine,
            inst::CAUSE_ILLEGAL_INSTRUCTION,
            u64::from(instruction_word(instruction)),
            0,
        );
    }

    #[test]
    fn misaligned_fetch_uses_the_instruction_address_cause() {
        let mut machine = trapped_machine();
        machine.cpu_mut().set_pc(DRAM_BASE + 1);

        machine.step(StepOptions::default());

        assert_trap(
            &machine,
            inst::CAUSE_MISALIGNED_FETCH,
            DRAM_BASE + 1,
            DRAM_BASE,
        );
    }

    #[test]
    fn unhandled_trap_eventually_reaches_the_step_limit() {
        let mut machine = machine();
        machine
            .bus_mut()
            .write_u32(DRAM_BASE, inst::MATCH_ECALL)
            .unwrap();
        machine.cpu_mut().set_pc(DRAM_BASE);

        assert_eq!(machine.run(3), RunOutcome::StepLimitReached);
    }

    #[test]
    fn fences_advance_the_pc_and_change_nothing_else() {
        let mut machine = machine();
        machine.cpu_mut().write_gpr(5, 42);

        machine.execute(Instruction::Fence {
            pred: 0b1111,
            succ: 0b1111,
        });
        machine.execute(Instruction::FenceI);

        assert_eq!(machine.cpu().pc(), 8, "both fences retire");
        assert_eq!(machine.cpu().read_gpr(5), 42, "no register changes");
    }

    #[test]
    fn user_ecall_reports_the_user_cause_and_privilege() {
        let mut machine = trapped_machine();
        machine.cpu_mut().set_privilege(PrivilegeMode::User);

        machine.execute(Instruction::Ecall);

        assert_trap(&machine, inst::CAUSE_USER_ECALL, 0, 0);
        assert_eq!(machine.cpu().privilege(), PrivilegeMode::Machine);
        let status = machine.cpu().csrs().read(inst::CSR_MSTATUS).unwrap();
        assert_eq!(status & (PrivilegeMode::User.mpp_bits() << 11), 0);
    }

    #[test]
    fn user_mode_cannot_access_machine_csrs() {
        let mut machine = trapped_machine();
        machine.cpu_mut().set_privilege(PrivilegeMode::User);

        let instruction = Instruction::Csrrs {
            rd: 5,
            csr: inst::CSR_MSCRATCH,
            rs1: 0,
        };
        machine.execute(instruction);

        assert_trap(
            &machine,
            inst::CAUSE_ILLEGAL_INSTRUCTION,
            u64::from(instruction_word(instruction)),
            0,
        );
    }

    #[test]
    fn counters_count_retirement_and_suppress_counter_writes() {
        let mut machine = machine();
        assert_eq!(machine.cpu().read_csr(inst::CSR_MCOUNTINHIBIT), Some(0));
        machine.cpu_mut().set_pc(DRAM_BASE);
        machine
            .bus_mut()
            .write_u32(DRAM_BASE, inst::MATCH_ADDI)
            .unwrap();

        machine.step(StepOptions::default());
        assert_eq!(machine.cpu().read_csr(inst::CSR_MCYCLE), Some(1));
        assert_eq!(machine.cpu().read_csr(inst::CSR_MINSTRET), Some(1));

        assert!(
            machine
                .cpu_mut()
                .write_csr(inst::CSR_MINSTRET, COUNTER_RELOAD_VALUE)
        );
        machine.execute(Instruction::Csrrwi {
            rd: 0,
            csr: inst::CSR_MINSTRET,
            zimm: 0,
        });

        assert_eq!(machine.cpu().read_csr(inst::CSR_MINSTRET), Some(0));
    }

    #[test]
    fn zero_valued_csr_source_still_attempts_a_read_only_write() {
        let mut machine = trapped_machine();
        machine.cpu_mut().write_gpr(6, 0);
        machine.cpu_mut().write_gpr(5, 7);

        machine.execute(Instruction::Csrrs {
            rd: 5,
            csr: inst::CSR_CYCLE,
            rs1: 6,
        });

        assert_eq!(machine.cpu().read_gpr(5), 7);
        assert_eq!(
            machine.cpu().read_csr(inst::CSR_MCAUSE),
            Some(u64::from(inst::CAUSE_ILLEGAL_INSTRUCTION))
        );
    }

    #[test]
    fn user_counter_read_requires_x0_for_read_only_csrrs() {
        let mut machine = trapped_machine();
        assert!(
            machine
                .cpu_mut()
                .write_csr(inst::CSR_MCOUNTEREN, COUNTER_CY_ENABLE)
        );
        machine.cpu_mut().set_privilege(PrivilegeMode::User);

        machine.execute(Instruction::Csrrs {
            rd: 5,
            csr: inst::CSR_CYCLE,
            rs1: 0,
        });
        assert_eq!(machine.cpu().read_gpr(5), 0);

        machine.cpu_mut().write_gpr(6, 0);
        machine.execute(Instruction::Csrrc {
            rd: 5,
            csr: inst::CSR_INSTRET,
            rs1: 6,
        });
        assert_eq!(
            machine.cpu().read_csr(inst::CSR_MCAUSE),
            Some(u64::from(inst::CAUSE_ILLEGAL_INSTRUCTION))
        );
    }

    #[test]
    fn execute_advances_both_counters() {
        let mut machine = machine();

        machine.execute(Instruction::Addi {
            rd: 0,
            rs1: 0,
            imm: 0,
        });

        assert_eq!(machine.cpu().read_csr(inst::CSR_MCYCLE), Some(1));
        assert_eq!(machine.cpu().read_csr(inst::CSR_MINSTRET), Some(1));
    }

    #[test]
    fn execute_and_step_observe_identical_counter_effects() {
        let instruction = Instruction::Addi {
            rd: 0,
            rs1: 0,
            imm: 0,
        };
        let mut stepped = machine();
        stepped
            .bus_mut()
            .write_u32(DRAM_BASE, inst::MATCH_ADDI)
            .unwrap();
        stepped.cpu_mut().set_pc(DRAM_BASE);
        stepped.step(StepOptions::default());

        let mut executed = machine();
        executed.execute(instruction);

        assert_eq!(
            stepped.cpu().read_csr(inst::CSR_MCYCLE),
            executed.cpu().read_csr(inst::CSR_MCYCLE)
        );
        assert_eq!(
            stepped.cpu().read_csr(inst::CSR_MINSTRET),
            executed.cpu().read_csr(inst::CSR_MINSTRET)
        );
    }

    #[test]
    fn counter_write_and_trap_advance_cycles_once() {
        let mut machine = trapped_machine();
        machine.cpu_mut().write_gpr(6, CYCLE_RELOAD_VALUE);
        machine.execute(Instruction::Csrrw {
            rd: 0,
            csr: inst::CSR_MCYCLE,
            rs1: 6,
        });
        assert_eq!(
            machine.cpu().read_csr(inst::CSR_MCYCLE),
            Some(CYCLE_RELOAD_VALUE)
        );

        machine.execute(Instruction::Ecall);
        assert_eq!(
            machine.cpu().read_csr(inst::CSR_MCYCLE),
            Some(CYCLE_RELOAD_VALUE + 1)
        );
        assert_eq!(machine.cpu().read_csr(inst::CSR_MINSTRET), Some(1));
    }

    #[test]
    fn bitmanip_counts_handle_zero_and_word_widths() {
        let mut machine = machine();

        for (instruction, rd, expected) in [
            (Instruction::Clz { rd: 5, rs1: 0 }, 5u8, 64),
            (Instruction::Ctz { rd: 6, rs1: 0 }, 6u8, 64),
            (Instruction::Clzw { rd: 7, rs1: 0 }, 7u8, 32),
            (Instruction::Ctzw { rd: 8, rs1: 0 }, 8u8, 32),
            (Instruction::Cpop { rd: 9, rs1: 0 }, 9u8, 0),
        ] {
            machine.execute(instruction);
            assert_eq!(machine.cpu().read_gpr(usize::from(rd)), expected, "{rd}");
        }

        machine.cpu_mut().write_gpr(10, 0xffff_ffff_0000_0001);
        machine.execute(Instruction::Clzw { rd: 5, rs1: 10 });
        machine.execute(Instruction::Cpopw { rd: 6, rs1: 10 });
        assert_eq!(machine.cpu().read_gpr(5), 31);
        assert_eq!(machine.cpu().read_gpr(6), 1);
    }

    #[test]
    fn bitmanip_word_operations_ignore_upper_source_bits() {
        let mut machine = machine();
        machine.cpu_mut().write_gpr(5, 0xffff_ffff_8000_0001);
        machine.cpu_mut().write_gpr(6, 1);

        for instruction in [
            Instruction::AddUw {
                rd: 7,
                rs1: 5,
                rs2: 6,
            },
            Instruction::Sh1AddUw {
                rd: 8,
                rs1: 5,
                rs2: 6,
            },
            Instruction::SlliUw {
                rd: 9,
                rs1: 5,
                shamt: 1,
            },
            Instruction::Rolw {
                rd: 10,
                rs1: 5,
                rs2: 6,
            },
        ] {
            machine.execute(instruction);
        }

        assert_eq!(machine.cpu().read_gpr(7), 0x8000_0002);
        assert_eq!(machine.cpu().read_gpr(8), 0x1_0000_0003);
        assert_eq!(machine.cpu().read_gpr(9), 0x1_0000_0002);
        assert_eq!(machine.cpu().read_gpr(10), 3);
    }

    #[test]
    fn bitmanip_byte_and_single_bit_operations_cover_edges() {
        let mut machine = machine();
        machine.cpu_mut().write_gpr(5, 0x0100_ff00_0000_8000);
        machine.cpu_mut().write_gpr(6, 0);

        machine.execute(Instruction::OrcB { rd: 7, rs1: 5 });
        machine.execute(Instruction::Rev8 { rd: 8, rs1: 5 });
        machine.execute(Instruction::Bclr {
            rd: 9,
            rs1: 5,
            rs2: 6,
        });
        machine.execute(Instruction::Bseti {
            rd: 10,
            rs1: 6,
            shamt: 63,
        });
        machine.execute(Instruction::Bexti {
            rd: 11,
            rs1: 5,
            shamt: 63,
        });

        assert_eq!(machine.cpu().read_gpr(7), 0xff00_ff00_0000_ff00);
        assert_eq!(machine.cpu().read_gpr(8), 0x0080_0000_00ff_0001);
        assert_eq!(machine.cpu().read_gpr(9), 0x0100_ff00_0000_8000);
        assert_eq!(machine.cpu().read_gpr(10), 1 << 63);
        assert_eq!(machine.cpu().read_gpr(11), 0);
    }

    #[test]
    fn hpm_counters_count_selected_retirement_events() {
        let mut machine = machine();
        machine.cpu_mut().set_pc(DRAM_BASE);
        machine.cpu_mut().write_gpr(1, DRAM_BASE);
        machine
            .bus_mut()
            .write_u64(DRAM_BASE + 8, 0x0123_4567_89ab_cdef)
            .unwrap();
        assert!(
            machine
                .cpu_mut()
                .write_csr(inst::CSR_MHPMEVENT3, csr::HPM_EVENT_INSTRUCTION.into())
        );
        assert!(
            machine
                .cpu_mut()
                .write_csr(inst::CSR_MHPMEVENT4, csr::HPM_EVENT_LOAD.into())
        );
        assert!(
            machine
                .cpu_mut()
                .write_csr(inst::CSR_MHPMEVENT5, csr::HPM_EVENT_BRANCH.into())
        );

        machine.execute(Instruction::Addi {
            rd: 2,
            rs1: 0,
            imm: 1,
        });
        machine.execute(Instruction::Lw {
            rd: 3,
            rs1: 1,
            offset: 8,
        });
        machine.execute(Instruction::Beq {
            rs1: 0,
            rs2: 0,
            offset: 0,
        });
        assert!(
            machine
                .cpu_mut()
                .write_csr(inst::CSR_MHPMEVENT5, csr::HPM_EVENT_STORE.into())
        );
        machine.execute(Instruction::Sw {
            rs2: 0,
            rs1: 1,
            offset: 8,
        });

        assert_eq!(machine.cpu().read_csr(inst::CSR_MHPMCOUNTER3), Some(4));
        assert_eq!(machine.cpu().read_csr(inst::CSR_MHPMCOUNTER4), Some(1));
        assert_eq!(machine.cpu().read_csr(inst::CSR_MHPMCOUNTER5), Some(2));
    }

    #[test]
    fn hpm_counters_honor_enable_and_inhibit_bits() {
        let mut machine = machine();
        assert!(
            machine
                .cpu_mut()
                .write_csr(inst::CSR_MHPMEVENT3, csr::HPM_EVENT_INSTRUCTION.into())
        );
        assert!(
            machine
                .cpu_mut()
                .write_csr(inst::CSR_MHPMEVENT4, csr::HPM_EVENT_LOAD.into())
        );
        assert!(
            machine
                .cpu_mut()
                .write_csr(inst::CSR_MCOUNTINHIBIT, HPM_COUNTER_3_BIT)
        );

        machine.execute(Instruction::Addi {
            rd: 2,
            rs1: 0,
            imm: 1,
        });
        assert_eq!(machine.cpu().read_csr(inst::CSR_MHPMCOUNTER3), Some(0));
        assert_eq!(machine.cpu().read_csr(inst::CSR_MINSTRET), Some(1));

        assert!(machine.cpu_mut().write_csr(inst::CSR_MCOUNTINHIBIT, 0));
        assert!(
            machine
                .cpu_mut()
                .write_csr(inst::CSR_MCOUNTEREN, HPM_COUNTER_3_BIT)
        );
        assert!(
            machine
                .cpu_mut()
                .write_csr(inst::CSR_SCOUNTEREN, HPM_COUNTER_3_BIT)
        );
        machine.cpu_mut().set_privilege(PrivilegeMode::User);
        assert_eq!(machine.cpu().read_csr(inst::CSR_HPMCOUNTER3), Some(0));
        assert_eq!(machine.cpu().read_csr(inst::CSR_HPMCOUNTER4), None);
        assert_eq!(machine.cpu().read_csr(inst::CSR_HPMCOUNTER5), None);
        assert!(!machine.cpu_mut().write_csr(inst::CSR_MHPMCOUNTER3, 1));
    }

    #[test]
    fn hpm_counters_ignore_faulting_and_unselected_instructions() {
        let mut machine = trapped_machine();
        machine.cpu_mut().set_pc(DRAM_BASE);
        machine.cpu_mut().write_gpr(1, DRAM_BASE);
        assert!(
            machine
                .cpu_mut()
                .write_csr(inst::CSR_MHPMEVENT3, csr::HPM_EVENT_LOAD.into())
        );
        assert!(
            machine
                .cpu_mut()
                .write_csr(inst::CSR_MHPMEVENT4, csr::HPM_EVENT_STORE.into())
        );
        assert!(machine.cpu_mut().write_csr(
            inst::CSR_MHPMEVENT5,
            u64::from(csr::HPM_EVENT_INSTRUCTION | csr::HPM_EVENT_BRANCH)
        ));

        machine.execute(Instruction::Lw {
            rd: 2,
            rs1: 1,
            offset: -0x100,
        });

        assert_eq!(machine.cpu().read_csr(inst::CSR_MHPMCOUNTER3), Some(0));
        assert_eq!(machine.cpu().read_csr(inst::CSR_MHPMCOUNTER4), Some(0));
        assert_eq!(machine.cpu().read_csr(inst::CSR_MHPMCOUNTER5), Some(0));
        assert_eq!(
            machine.cpu().read_csr(inst::CSR_MCAUSE),
            Some(u64::from(inst::CAUSE_LOAD_ACCESS))
        );
    }

    #[test]
    fn hpm_memory_events_include_floating_and_atomic_operations() {
        let cases = [
            (
                Instruction::Lw {
                    rd: 2,
                    rs1: 1,
                    offset: 8,
                },
                true,
                false,
            ),
            (
                Instruction::Flw {
                    rd: 2,
                    rs1: 1,
                    offset: 8,
                },
                true,
                false,
            ),
            (
                Instruction::Flh {
                    rd: 2,
                    rs1: 1,
                    offset: 8,
                },
                true,
                false,
            ),
            (
                Instruction::LrW {
                    rd: 2,
                    rs1: 1,
                    aq: false,
                    rl: false,
                },
                true,
                false,
            ),
            (
                Instruction::Sw {
                    rs2: 0,
                    rs1: 1,
                    offset: 8,
                },
                false,
                true,
            ),
            (
                Instruction::Fsw {
                    rs2: 0,
                    rs1: 1,
                    offset: 8,
                },
                false,
                true,
            ),
            (
                Instruction::Fsh {
                    rs2: 0,
                    rs1: 1,
                    offset: 8,
                },
                false,
                true,
            ),
            (
                Instruction::ScW {
                    rd: 2,
                    rs1: 1,
                    rs2: 0,
                    aq: false,
                    rl: false,
                },
                false,
                true,
            ),
            (
                Instruction::AmoAddW {
                    rd: 2,
                    rs1: 1,
                    rs2: 0,
                    aq: false,
                    rl: false,
                },
                false,
                true,
            ),
            (Instruction::CboZero { rs1: 1 }, false, true),
        ];

        for (instruction, load, store) in cases {
            let mut machine = machine();
            assert!(
                machine
                    .cpu_mut()
                    .write_csr(inst::CSR_MSTATUS, FP_INITIAL_STATUS)
            );
            machine.cpu_mut().set_pc(DRAM_BASE);
            machine.cpu_mut().write_gpr(1, DRAM_BASE);
            machine
                .bus_mut()
                .write_u64(DRAM_BASE + 8, 0x0123_4567_89ab_cdef)
                .unwrap();
            assert!(
                machine
                    .cpu_mut()
                    .write_csr(inst::CSR_MHPMEVENT3, csr::HPM_EVENT_LOAD.into())
            );
            assert!(
                machine
                    .cpu_mut()
                    .write_csr(inst::CSR_MHPMEVENT4, csr::HPM_EVENT_STORE.into())
            );

            machine.execute(instruction);

            assert_eq!(
                machine.cpu().read_csr(inst::CSR_MHPMCOUNTER3),
                Some(u64::from(load)),
                "{instruction:?}"
            );
            assert_eq!(
                machine.cpu().read_csr(inst::CSR_MHPMCOUNTER4),
                Some(u64::from(store)),
                "{instruction:?}"
            );
        }
    }

    #[test]
    fn hpm_control_transfer_events_include_jumps() {
        for instruction in [
            Instruction::Beq {
                rs1: 0,
                rs2: 0,
                offset: 0,
            },
            Instruction::Jal { rd: 2, offset: 0 },
            Instruction::Jalr {
                rd: 2,
                rs1: 0,
                offset: DRAM_BASE as i64,
            },
        ] {
            let mut machine = machine();
            machine.cpu_mut().set_pc(DRAM_BASE);
            assert!(
                machine
                    .cpu_mut()
                    .write_csr(inst::CSR_MHPMEVENT5, csr::HPM_EVENT_BRANCH.into())
            );

            machine.execute(instruction);

            assert_eq!(
                machine.cpu().read_csr(inst::CSR_MHPMCOUNTER5),
                Some(1),
                "{instruction:?}"
            );
        }
    }

    #[test]
    fn hpm_counter_write_suppresses_its_own_event_increment() {
        let mut machine = machine();
        assert!(
            machine
                .cpu_mut()
                .write_csr(inst::CSR_MHPMEVENT3, csr::HPM_EVENT_INSTRUCTION.into())
        );
        machine.cpu_mut().write_gpr(1, 42);

        machine.execute(Instruction::Csrrw {
            rd: 0,
            csr: inst::CSR_MHPMCOUNTER3,
            rs1: 1,
        });

        assert_eq!(machine.cpu().read_csr(inst::CSR_MHPMCOUNTER3), Some(42));
    }

    #[test]
    fn cache_block_management_checks_the_containing_block() {
        let mut machine = Machine::new(Memory::new());
        machine.cpu_mut().set_pc(DRAM_BASE);
        machine.cpu_mut().write_gpr(1, DRAM_BASE + 17);
        machine.bus_mut().write_u8(DRAM_BASE + 10, 0x5a).unwrap();

        machine.execute(Instruction::CboClean { rs1: 1 });

        assert_eq!(machine.cpu().pc(), DRAM_BASE + 4);
        assert_eq!(machine.bus_mut().read_u8(DRAM_BASE + 10).unwrap(), 0x5a);

        let mut trapped = trapped_machine();
        trapped.cpu_mut().set_pc(DRAM_BASE);
        trapped.cpu_mut().write_gpr(1, DRAM_BASE - 1);
        trapped.execute(Instruction::CboFlush { rs1: 1 });

        assert_trap(&trapped, inst::CAUSE_STORE_ACCESS, DRAM_BASE - 1, DRAM_BASE);
    }

    #[test]
    fn cache_block_zero_clears_the_containing_block() {
        let mut machine = Machine::new(Memory::new());
        machine.cpu_mut().set_pc(DRAM_BASE);
        machine.cpu_mut().write_gpr(1, DRAM_BASE + 17);
        for offset in 0..CACHE_BLOCK_BYTES {
            machine
                .bus_mut()
                .write_u8(DRAM_BASE + offset, 0xa5)
                .unwrap();
        }

        machine.execute(Instruction::CboZero { rs1: 1 });

        for offset in 0..CACHE_BLOCK_BYTES {
            assert_eq!(machine.bus_mut().read_u8(DRAM_BASE + offset).unwrap(), 0);
        }
    }

    #[test]
    fn cache_block_zero_faults_without_partial_writes() {
        let mut machine = Machine::new(Memory::with_size(32));
        machine.cpu_mut().set_pc(DRAM_BASE);
        machine.cpu_mut().write_gpr(1, DRAM_BASE);
        for offset in 0..32 {
            machine
                .bus_mut()
                .write_u8(DRAM_BASE + offset, 0x5a)
                .unwrap();
        }

        machine.execute(Instruction::CboZero { rs1: 1 });

        assert_eq!(
            machine.cpu().read_csr(inst::CSR_MCAUSE),
            Some(u64::from(inst::CAUSE_STORE_ACCESS))
        );
        for offset in 0..32 {
            assert_eq!(machine.bus_mut().read_u8(DRAM_BASE + offset).unwrap(), 0x5a);
        }
    }

    #[test]
    fn user_cache_block_operations_require_menvcfg_enables() {
        let mut disabled = trapped_machine();
        disabled.cpu_mut().set_privilege(PrivilegeMode::User);
        disabled.cpu_mut().write_gpr(1, DRAM_BASE);

        disabled.execute(Instruction::CboZero { rs1: 1 });

        assert_eq!(
            disabled.cpu().read_csr(inst::CSR_MCAUSE),
            Some(u64::from(inst::CAUSE_ILLEGAL_INSTRUCTION))
        );

        let mut enabled = machine();
        assert!(enabled.cpu_mut().write_csr(
            inst::CSR_MENVCFG,
            MENVCFG_CBIE_FLUSH | MENVCFG_CBCFE | MENVCFG_CBZE
        ));
        assert!(enabled.cpu_mut().write_csr(
            inst::CSR_SENVCFG,
            MENVCFG_CBIE_FLUSH | MENVCFG_CBCFE | MENVCFG_CBZE
        ));
        enabled.cpu_mut().set_privilege(PrivilegeMode::User);
        enabled.cpu_mut().write_gpr(1, DRAM_BASE);

        enabled.execute(Instruction::CboClean { rs1: 1 });
        enabled.execute(Instruction::CboFlush { rs1: 1 });
        enabled.execute(Instruction::CboInval { rs1: 1 });
        enabled.execute(Instruction::CboZero { rs1: 1 });
        enabled.cpu_mut().set_privilege(PrivilegeMode::Machine);

        assert_eq!(
            enabled.cpu().read_csr(inst::CSR_MCAUSE),
            Some(0),
            "enabled U-mode CBO operations must retire without trapping"
        );
    }

    #[test]
    fn prefetch_and_pause_are_untrapping_hints() {
        let mut machine = machine();
        machine.cpu_mut().set_pc(DRAM_BASE);
        machine.cpu_mut().write_gpr(1, DRAM_BASE - 1);

        for instruction in [
            Instruction::PrefetchI { rs1: 1, offset: 0 },
            Instruction::PrefetchR {
                rs1: 1,
                offset: 160,
            },
            Instruction::PrefetchW {
                rs1: 1,
                offset: -32,
            },
            Instruction::Pause,
        ] {
            machine.execute(instruction);
        }

        assert_eq!(machine.cpu().read_csr(inst::CSR_MCAUSE), Some(0));
        assert_eq!(machine.cpu().pc(), DRAM_BASE + 16);
    }

    #[test]
    fn cache_block_zero_invalidates_every_reservation_set_in_the_block() {
        let block = DRAM_BASE + 0x100;
        for offset in 0..CACHE_BLOCK_DOUBLEWORDS {
            let address = block + offset * DOUBLEWORD_BYTES;
            let mut machine = Machine::new(Memory::new());
            machine.cpu_mut().set_pc(DRAM_BASE);
            machine.cpu_mut().write_gpr(1, block);
            machine.cpu_mut().write_gpr(3, address);

            machine.execute(Instruction::LrD {
                rd: 2,
                rs1: 3,
                aq: false,
                rl: false,
            });
            machine.execute(Instruction::CboZero { rs1: 1 });
            machine.execute(Instruction::ScD {
                rd: 5,
                rs1: 3,
                rs2: 4,
                aq: false,
                rl: false,
            });

            assert_eq!(machine.cpu().read_gpr(5), 1, "offset {offset}");
        }
    }

    #[test]
    fn cache_block_zero_preserves_reservations_outside_the_block() {
        let block = DRAM_BASE + 0x100;
        let outside = block + CACHE_BLOCK_BYTES;
        let mut machine = Machine::new(Memory::new());
        machine.cpu_mut().set_pc(DRAM_BASE);
        machine.cpu_mut().write_gpr(1, block);
        machine.cpu_mut().write_gpr(3, outside);

        machine.execute(Instruction::LrD {
            rd: 2,
            rs1: 3,
            aq: false,
            rl: false,
        });
        machine.execute(Instruction::CboZero { rs1: 1 });
        machine.execute(Instruction::ScD {
            rd: 5,
            rs1: 3,
            rs2: 4,
            aq: false,
            rl: false,
        });

        assert_eq!(machine.cpu().read_gpr(5), 0);
    }
}
