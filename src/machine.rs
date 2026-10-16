//! A hart plus the bus it executes against.

use crate::bus::Bus;
use crate::cpu::Cpu;
use crate::decode::{Instruction, decode};
use crate::trap::Trap;

/// Width of a 32-bit instruction; the C extension will add a 16-bit case.
const INSTRUCTION_BYTES: u64 = 4;

/// Why [`Machine::run`] returned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunOutcome {
    /// The hart executed the halt self-loop, `jal x0, 0`.
    Halted,
    /// The step limit ran out first.
    StepLimitReached,
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
        decode(self.bus.read_u32(self.cpu.pc())?)
    }

    /// Fetches, decodes, and executes the instruction at the PC.
    pub fn step(&mut self) -> Result<(), Trap> {
        let instruction = self.fetch()?;
        self.execute(instruction)
    }

    /// Runs until the halt self-loop retires or `step_limit` steps are taken.
    pub fn run(&mut self, step_limit: u64) -> Result<RunOutcome, Trap> {
        for _ in 0..step_limit {
            self.step()?;
            if self.halted {
                return Ok(RunOutcome::Halted);
            }
        }
        Ok(RunOutcome::StepLimitReached)
    }

    /// Executes one already-fetched instruction, for callers that trace them.
    pub fn execute(&mut self, instruction: Instruction) -> Result<(), Trap> {
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
            Instruction::Beq { rs1, rs2, offset } => {
                if self.gpr(rs1) == self.gpr(rs2) {
                    next_pc = pc.wrapping_add_signed(offset);
                }
            }
            Instruction::Lui { rd, imm } => self.write(rd, imm as u64),
            Instruction::Auipc { rd, imm } => {
                self.write(rd, pc.wrapping_add_signed(imm));
            }
            Instruction::Jalr { rd, rs1, offset } => {
                let target = self.gpr(rs1).wrapping_add_signed(offset) & !1;
                if target % INSTRUCTION_BYTES != 0 {
                    return Err(Trap::InstructionAddressMisaligned);
                }
                self.write(rd, next_pc);
                next_pc = target;
            }
            Instruction::Jal { rd, offset } => {
                // `jal x0, 0` is the halt convention: no link, no movement.
                if rd == 0 && offset == 0 {
                    self.halted = true;
                    return Ok(());
                }
                self.write(rd, next_pc);
                next_pc = pc.wrapping_add_signed(offset);
            }
        }
        self.cpu.set_pc(next_pc);
        Ok(())
    }

    /// Reads general-purpose register `index`.
    fn gpr(&self, index: u8) -> u64 {
        self.cpu.read_gpr(usize::from(index))
    }

    /// Writes `value` to general-purpose register `rd`; `x0` discards it.
    fn write(&mut self, rd: u8, value: u64) {
        self.cpu.write_gpr(usize::from(rd), value);
    }
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

    /// A hart on 64 bytes of memory, no program loaded.
    fn machine() -> Machine<Memory> {
        Machine::new(Memory::with_size(64))
    }

    #[test]
    fn addi_sums_and_advances_the_pc() {
        let mut machine = machine();
        machine.cpu_mut().write_gpr(6, 40);

        machine
            .execute(Instruction::Addi {
                rd: 5,
                rs1: 6,
                imm: 2,
            })
            .unwrap();

        assert_eq!(machine.cpu().read_gpr(5), 42);
        assert_eq!(machine.cpu().pc(), 4);
    }

    #[test]
    fn addi_wraps_negative_immediates() {
        let mut machine = machine();

        machine
            .execute(Instruction::Addi {
                rd: 5,
                rs1: 0,
                imm: -1,
            })
            .unwrap();

        assert_eq!(machine.cpu().read_gpr(5), u64::MAX);
    }

    #[test]
    fn addi_to_x0_is_discarded() {
        let mut machine = machine();

        machine
            .execute(Instruction::Addi {
                rd: 0,
                rs1: 0,
                imm: 7,
            })
            .unwrap();

        assert_eq!(machine.cpu().read_gpr(0), 0);
        assert_eq!(machine.cpu().pc(), 4);
    }

    #[test]
    fn add_sums_two_registers() {
        let mut machine = machine();
        machine.cpu_mut().write_gpr(8, 3);
        machine.cpu_mut().write_gpr(9, 4);

        machine
            .execute(Instruction::Add {
                rd: 7,
                rs1: 8,
                rs2: 9,
            })
            .unwrap();

        assert_eq!(machine.cpu().read_gpr(7), 7);
        assert_eq!(machine.cpu().pc(), 4);
    }

    #[test]
    fn beq_taken_branches_from_the_instruction_address() {
        let mut machine = machine();
        machine.cpu_mut().write_gpr(10, 5);
        machine.cpu_mut().write_gpr(11, 5);
        machine.cpu_mut().set_pc(DRAM_BASE + 8);

        machine
            .execute(Instruction::Beq {
                rs1: 10,
                rs2: 11,
                offset: -8,
            })
            .unwrap();

        assert_eq!(machine.cpu().pc(), DRAM_BASE);
    }

    #[test]
    fn beq_not_taken_advances_the_pc() {
        let mut machine = machine();
        machine.cpu_mut().write_gpr(10, 5);
        machine.cpu_mut().write_gpr(11, 6);

        machine
            .execute(Instruction::Beq {
                rs1: 10,
                rs2: 11,
                offset: 16,
            })
            .unwrap();

        assert_eq!(machine.cpu().pc(), 4);
    }

    #[test]
    fn jal_links_and_jumps() {
        let mut machine = machine();
        machine.cpu_mut().set_pc(DRAM_BASE + 12);

        machine
            .execute(Instruction::Jal { rd: 1, offset: -12 })
            .unwrap();

        assert_eq!(machine.cpu().read_gpr(1), DRAM_BASE + 16);
        assert_eq!(machine.cpu().pc(), DRAM_BASE);
    }

    #[test]
    fn halt_self_loop_marks_the_hart_halted() {
        let mut machine = machine();
        machine.cpu_mut().set_pc(DRAM_BASE);

        machine
            .execute(Instruction::Jal { rd: 0, offset: 0 })
            .unwrap();

        assert!(machine.halted());
        assert_eq!(machine.cpu().pc(), DRAM_BASE);
    }

    #[test]
    fn fetch_outside_memory_faults() {
        let mut machine = machine();
        machine.cpu_mut().set_pc(DRAM_BASE - 4);

        assert_eq!(machine.step(), Err(Trap::AccessFault));
    }

    #[test]
    fn jalr_to_a_misaligned_target_traps() {
        let mut machine = machine();
        machine.cpu_mut().write_gpr(6, DRAM_BASE + 2);

        assert_eq!(
            machine.execute(Instruction::Jalr {
                rd: 5,
                rs1: 6,
                offset: 0
            }),
            Err(Trap::InstructionAddressMisaligned)
        );
        assert_eq!(machine.cpu().pc(), 0, "the PC must not move");
    }

    #[test]
    fn jalr_links_and_clears_bit_zero() {
        let mut machine = machine();
        machine.cpu_mut().set_pc(DRAM_BASE);
        machine.cpu_mut().write_gpr(6, DRAM_BASE + 8 + 1);

        machine
            .execute(Instruction::Jalr {
                rd: 5,
                rs1: 6,
                offset: 0,
            })
            .unwrap();

        assert_eq!(machine.cpu().read_gpr(5), DRAM_BASE + 4);
        assert_eq!(machine.cpu().pc(), DRAM_BASE + 8);
    }
}
