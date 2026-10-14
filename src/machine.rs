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

    /// Fetches, decodes, and executes the instruction at the PC.
    pub fn step(&mut self) -> Result<(), Trap> {
        let instruction = decode(self.bus.read_u32(self.cpu.pc())?)?;
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

    /// Executes one already-decoded instruction.
    fn execute(&mut self, instruction: Instruction) -> Result<(), Trap> {
        let pc = self.cpu.pc();

        match instruction {
            Instruction::Addi { rd, rs1, imm } => {
                let value = self.cpu.read_gpr(rs1.into()).wrapping_add_signed(imm);
                self.cpu.write_gpr(rd.into(), value);
                self.advance();
            }
            Instruction::Add { rd, rs1, rs2 } => {
                let value = self
                    .cpu
                    .read_gpr(rs1.into())
                    .wrapping_add(self.cpu.read_gpr(rs2.into()));
                self.cpu.write_gpr(rd.into(), value);
                self.advance();
            }
            Instruction::Beq { rs1, rs2, offset } => {
                if self.cpu.read_gpr(rs1.into()) == self.cpu.read_gpr(rs2.into()) {
                    self.cpu.set_pc(pc.wrapping_add_signed(offset));
                } else {
                    self.advance();
                }
            }
            Instruction::Jal { rd, offset } => {
                // `jal x0, 0` is the halt convention: no link, no movement.
                if rd == 0 && offset == 0 {
                    self.halted = true;
                    return Ok(());
                }
                self.cpu
                    .write_gpr(rd.into(), pc.wrapping_add(INSTRUCTION_BYTES));
                self.cpu.set_pc(pc.wrapping_add_signed(offset));
            }
        }
        Ok(())
    }

    /// Moves the PC to the next instruction.
    fn advance(&mut self) {
        let next = self.cpu.pc().wrapping_add(INSTRUCTION_BYTES);
        self.cpu.set_pc(next);
    }
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
}
