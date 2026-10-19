//! A hart plus the bus it executes against.

use crate::bus::Bus;
use crate::cpu::Cpu;
use crate::decode::{Instruction, decode};
use crate::inst;
use crate::trap::Trap;

/// Width of a 32-bit instruction; the C extension will add a 16-bit case.
const INSTRUCTION_BYTES: u64 = 4;

/// A synchronous exception ready for architectural trap delivery.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Exception {
    cause: u8,
    tval: u64,
}

impl Exception {
    fn new(cause: u8, tval: u64) -> Self {
        Self { cause, tval }
    }
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

/// Prints one instruction in the CLI trace format.
fn print_trace(pc: u64, instruction: &Instruction) {
    println!("{pc:#010x}  {}", instruction.render());
}

/// One hart and the bus it fetches from and executes against.
pub struct Machine<B: Bus> {
    cpu: Cpu,
    bus: B,
    halted: bool,
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
        let word = self.fetch_word()?;
        decode(word)
    }

    /// Fetches, decodes, and executes one instruction.
    ///
    /// Synchronous faults enter the machine-mode trap handler instead of
    /// escaping this method.
    pub fn step(&mut self, options: StepOptions) {
        let pc = self.cpu.pc();
        let word = match self.fetch_word() {
            Ok(word) => word,
            Err(trap) => {
                self.enter_trap(self.fetch_exception(pc, 0, trap));
                return;
            }
        };
        let instruction = match decode(word) {
            Ok(instruction) => instruction,
            Err(trap) => {
                self.enter_trap(self.fetch_exception(pc, word, trap));
                return;
            }
        };
        if options.trace {
            print_trace(pc, &instruction);
        }
        self.execute_with_word(instruction, word);
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
    /// Synchronous faults enter the machine-mode trap handler.
    pub fn execute(&mut self, instruction: Instruction) {
        self.execute_with_word(instruction, instruction_word(instruction));
    }

    /// Fetches the raw instruction word and checks its address alignment.
    fn fetch_word(&mut self) -> Result<u32, Trap> {
        let pc = self.cpu.pc();
        if !pc.is_multiple_of(INSTRUCTION_BYTES) {
            return Err(Trap::InstructionAddressMisaligned);
        }
        self.bus.read_u32(pc)
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
    fn execute_with_word(&mut self, instruction: Instruction, word: u32) {
        if let Err(exception) = self.execute_inner(instruction, word) {
            self.enter_trap(exception);
        }
    }

    /// Records trap state and transfers control to `mtvec`.
    fn enter_trap(&mut self, exception: Exception) {
        let epc = self.cpu.pc();
        self.cpu
            .csrs_mut()
            .enter_trap(epc, u64::from(exception.cause), exception.tval);
        let handler = self.cpu.csrs().trap_vector();
        self.cpu.set_pc(handler);
    }

    /// Executes one instruction without delivering exceptions.
    fn execute_inner(&mut self, instruction: Instruction, word: u32) -> Result<(), Exception> {
        let pc = self.cpu.pc();
        let mut next_pc = pc.wrapping_add(INSTRUCTION_BYTES);

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
            Instruction::Lb { rd, rs1, offset } => {
                let address = self.gpr(rs1).wrapping_add_signed(offset);
                let value = self
                    .bus
                    .read_u8(address)
                    .map_err(|_| Exception::new(inst::CAUSE_LOAD_ACCESS, address))?
                    as i8 as u64;
                self.write(rd, value);
            }
            Instruction::Lh { rd, rs1, offset } => {
                let address = self.gpr(rs1).wrapping_add_signed(offset);
                let value = self
                    .bus
                    .read_u16(address)
                    .map_err(|_| Exception::new(inst::CAUSE_LOAD_ACCESS, address))?
                    as i16 as u64;
                self.write(rd, value);
            }
            Instruction::Lw { rd, rs1, offset } => {
                let address = self.gpr(rs1).wrapping_add_signed(offset);
                let value = self
                    .bus
                    .read_u32(address)
                    .map_err(|_| Exception::new(inst::CAUSE_LOAD_ACCESS, address))?
                    as i32 as u64;
                self.write(rd, value);
            }
            Instruction::Ld { rd, rs1, offset } => {
                let address = self.gpr(rs1).wrapping_add_signed(offset);
                let value = self
                    .bus
                    .read_u64(address)
                    .map_err(|_| Exception::new(inst::CAUSE_LOAD_ACCESS, address))?;
                self.write(rd, value);
            }
            Instruction::Lbu { rd, rs1, offset } => {
                let address = self.gpr(rs1).wrapping_add_signed(offset);
                let value = u64::from(
                    self.bus
                        .read_u8(address)
                        .map_err(|_| Exception::new(inst::CAUSE_LOAD_ACCESS, address))?,
                );
                self.write(rd, value);
            }
            Instruction::Lhu { rd, rs1, offset } => {
                let address = self.gpr(rs1).wrapping_add_signed(offset);
                let value = u64::from(
                    self.bus
                        .read_u16(address)
                        .map_err(|_| Exception::new(inst::CAUSE_LOAD_ACCESS, address))?,
                );
                self.write(rd, value);
            }
            Instruction::Lwu { rd, rs1, offset } => {
                let address = self.gpr(rs1).wrapping_add_signed(offset);
                let value = u64::from(
                    self.bus
                        .read_u32(address)
                        .map_err(|_| Exception::new(inst::CAUSE_LOAD_ACCESS, address))?,
                );
                self.write(rd, value);
            }
            Instruction::Sb { rs2, rs1, offset } => {
                let address = self.gpr(rs1).wrapping_add_signed(offset);
                self.bus
                    .write_u8(address, self.gpr(rs2) as u8)
                    .map_err(|_| Exception::new(inst::CAUSE_STORE_ACCESS, address))?;
            }
            Instruction::Sh { rs2, rs1, offset } => {
                let address = self.gpr(rs1).wrapping_add_signed(offset);
                self.bus
                    .write_u16(address, self.gpr(rs2) as u16)
                    .map_err(|_| Exception::new(inst::CAUSE_STORE_ACCESS, address))?;
            }
            Instruction::Sw { rs2, rs1, offset } => {
                let address = self.gpr(rs1).wrapping_add_signed(offset);
                self.bus
                    .write_u32(address, self.gpr(rs2) as u32)
                    .map_err(|_| Exception::new(inst::CAUSE_STORE_ACCESS, address))?;
            }
            Instruction::Sd { rs2, rs1, offset } => {
                let address = self.gpr(rs1).wrapping_add_signed(offset);
                self.bus
                    .write_u64(address, self.gpr(rs2))
                    .map_err(|_| Exception::new(inst::CAUSE_STORE_ACCESS, address))?;
            }
            // Ordering operations have no effect on one hart with no cache model:
            // the shared PC update below retires them.
            Instruction::Fence { .. } | Instruction::FenceI => {}
            Instruction::Csrrw { rd, csr, rs1 } => {
                let old = self.require_csr(csr, word)?;
                let source = self.gpr(rs1);
                debug_assert!(self.cpu.write_csr(csr, source));
                self.write(rd, old);
            }
            Instruction::Csrrs { rd, csr, rs1 } => {
                let old = self.require_csr(csr, word)?;
                let source = self.gpr(rs1);
                if source != 0 {
                    debug_assert!(self.cpu.write_csr(csr, old | source));
                }
                self.write(rd, old);
            }
            Instruction::Csrrc { rd, csr, rs1 } => {
                let old = self.require_csr(csr, word)?;
                let source = self.gpr(rs1);
                if source != 0 {
                    debug_assert!(self.cpu.write_csr(csr, old & !source));
                }
                self.write(rd, old);
            }
            Instruction::Csrrwi { rd, csr, zimm } => {
                let old = self.require_csr(csr, word)?;
                debug_assert!(self.cpu.write_csr(csr, u64::from(zimm)));
                self.write(rd, old);
            }
            Instruction::Csrrsi { rd, csr, zimm } => {
                let old = self.require_csr(csr, word)?;
                if zimm != 0 {
                    debug_assert!(self.cpu.write_csr(csr, old | u64::from(zimm)));
                }
                self.write(rd, old);
            }
            Instruction::Csrrci { rd, csr, zimm } => {
                let old = self.require_csr(csr, word)?;
                if zimm != 0 {
                    debug_assert!(self.cpu.write_csr(csr, old & !u64::from(zimm)));
                }
                self.write(rd, old);
            }
            Instruction::Ecall => {
                return Err(Exception::new(inst::CAUSE_MACHINE_ECALL, 0));
            }
            Instruction::Ebreak => {
                return Err(Exception::new(inst::CAUSE_BREAKPOINT, pc));
            }
            Instruction::Mret => {
                // Check the return target before changing any machine state.
                let target = Self::checked_target(self.cpu.csrs().exception_pc())?;
                let restored_pc = self.cpu.csrs_mut().return_from_m_trap();
                debug_assert_eq!(target, restored_pc);
                self.cpu.set_pc(target);
                return Ok(());
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
                if rd == 0 && offset == 0 {
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

    /// Checks that a control-transfer target is four-byte aligned.
    fn checked_target(target: u64) -> Result<u64, Exception> {
        if !target.is_multiple_of(INSTRUCTION_BYTES) {
            return Err(Exception::new(inst::CAUSE_MISALIGNED_FETCH, target));
        }
        Ok(target)
    }

    /// Writes `value` to general-purpose register `rd`; `x0` discards it.
    fn write(&mut self, rd: u8, value: u64) {
        self.cpu.write_gpr(usize::from(rd), value);
    }
}

/// Re-encodes decoded CSR instructions so direct `execute` calls can report
/// their instruction bits in `mtval` when the CSR is unsupported.
fn instruction_word(instruction: Instruction) -> u32 {
    const CSR_SHIFT: u32 = 20;
    const RS1_SHIFT: u32 = 15;
    const RD_SHIFT: u32 = 7;

    let (opcode, rd, rs1, csr) = match instruction {
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

/// Sign-extends the low 32 bits of `value` to 64 bits.
fn sign_extend_32(value: u64) -> u64 {
    i64::from(value as u32 as i32) as u64
}

/// Shift amount of the 64-bit shifts, taken from the low six bits.
fn shift_64(value: u64) -> u32 {
    (value & 0x3f) as u32
}

/// Shift amount of the W-forms, taken from the low five bits.
fn shift_32(value: u64) -> u32 {
    (value & 0x1f) as u32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{DRAM_BASE, Memory};

    const HANDLER: u64 = DRAM_BASE + 32;

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

    fn assert_trap(machine: &Machine<Memory>, cause: u8, tval: u64, epc: u64) {
        assert_eq!(
            machine.cpu().read_csr(inst::CSR_MCAUSE),
            Some(u64::from(cause)),
            "mcause"
        );
        assert_eq!(machine.cpu().read_csr(inst::CSR_MTVAL), Some(tval), "mtval");
        assert_eq!(machine.cpu().read_csr(inst::CSR_MEPC), Some(epc), "mepc");
        assert_eq!(machine.cpu().pc(), HANDLER, "handler PC");
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
    fn jalr_to_a_misaligned_target_traps_without_linking() {
        let mut machine = trapped_machine();
        machine.cpu_mut().write_gpr(6, DRAM_BASE + 2);
        machine.cpu_mut().write_gpr(5, 7);

        machine.execute(Instruction::Jalr {
            rd: 5,
            rs1: 6,
            offset: 0,
        });

        assert_eq!(machine.cpu().read_gpr(5), 7, "rd must not change");
        assert_trap(&machine, inst::CAUSE_MISALIGNED_FETCH, DRAM_BASE + 2, 0);
    }

    #[test]
    fn jal_to_a_misaligned_target_traps_without_linking() {
        let mut machine = trapped_machine();
        machine.cpu_mut().write_gpr(5, 7);

        machine.execute(Instruction::Jal { rd: 5, offset: 2 });

        assert_eq!(machine.cpu().read_gpr(5), 7, "rd must not change");
        assert_trap(&machine, inst::CAUSE_MISALIGNED_FETCH, 2, 0);
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
            offset: 2,
        });

        assert_trap(&machine, inst::CAUSE_MISALIGNED_FETCH, 2, 0);
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
    fn ecall_enters_the_machine_trap_handler_precisely() {
        let mut machine = trapped_machine();
        machine.cpu_mut().set_pc(DRAM_BASE + 4);
        assert!(machine.cpu_mut().write_csr(inst::CSR_MSTATUS, 1 << 3));

        machine.execute(Instruction::Ecall);

        assert_trap(&machine, inst::CAUSE_MACHINE_ECALL, 0, DRAM_BASE + 4);
        let status = machine.cpu().read_csr(inst::CSR_MSTATUS).unwrap();
        assert_eq!(status & (1 << 3), 0, "MIE is cleared on trap entry");
        assert_eq!(status & (1 << 7), 1 << 7, "MIE is saved in MPIE");
        assert_eq!(status & (0b11 << 11), 0b11 << 11, "MPP records M-mode");
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
        let status = machine.cpu().read_csr(inst::CSR_MSTATUS).unwrap();
        assert_eq!(status & (1 << 3), 1 << 3, "MIE is restored from MPIE");
        assert_eq!(status & (1 << 7), 1 << 7, "MPIE is set");
        assert_eq!(status & (0b11 << 11), 0, "MPP is set to U-mode");
    }

    #[test]
    fn illegal_instruction_fetch_enters_the_trap_handler() {
        let mut machine = trapped_machine();
        machine.cpu_mut().set_pc(DRAM_BASE);
        machine
            .bus_mut()
            .write_u32(DRAM_BASE, inst::MATCH_CSRRCI ^ 1)
            .unwrap();

        machine.step(StepOptions::default());

        assert_trap(
            &machine,
            inst::CAUSE_ILLEGAL_INSTRUCTION,
            u64::from(inst::MATCH_CSRRCI ^ 1),
            DRAM_BASE,
        );
    }

    #[test]
    fn unsupported_csr_traps_before_writing_rd_or_csr_state() {
        let instruction = Instruction::Csrrs {
            rd: 5,
            csr: inst::CSR_MISA,
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
        machine.cpu_mut().set_pc(DRAM_BASE + 2);

        machine.step(StepOptions::default());

        assert_trap(
            &machine,
            inst::CAUSE_MISALIGNED_FETCH,
            DRAM_BASE + 2,
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
}
