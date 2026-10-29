//! Command-line front end: load a flat binary, run it, and report the result.

use std::fs;
use std::process::ExitCode;

use riscv_simulator_in_rust::bus::{Bus, MemoryType};
use riscv_simulator_in_rust::cli::{self, Command, Options, USAGE};
use riscv_simulator_in_rust::cpu::{ABI_NAMES, Cpu, FPR_COUNT};
use riscv_simulator_in_rust::machine::{Machine, RunOutcome, StepOptions};
use riscv_simulator_in_rust::memory::{DRAM_BASE, Memory};

/// Exit code for usage and input errors.
const EXIT_USAGE: u8 = 2;

/// Exit code for a run that stopped at its step limit.
const EXIT_STEP_LIMIT: u8 = 2;

/// Number of defined IEEE exception flags in `fcsr`.
const FFLAGS_BITS: u32 = 5;
const FFLAGS_MASK: u64 = (1 << FFLAGS_BITS) - 1;
const FRM_SHIFT: u32 = FFLAGS_BITS;
const FRM_MASK: u64 = 0b111;

fn main() -> ExitCode {
    let options = match cli::parse(std::env::args().skip(1)) {
        Ok(Command::Run(options)) => options,
        Ok(Command::Help) => {
            print!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        Err(message) => {
            eprintln!("{message}");
            eprint!("{USAGE}");
            return ExitCode::from(EXIT_USAGE);
        }
    };

    run(&options)
}

/// Loads the program, runs it, prints the final state, and picks an exit code.
fn run(options: &Options) -> ExitCode {
    let binary = match fs::read(&options.binary) {
        Ok(binary) => binary,
        Err(error) => {
            eprintln!("cannot read {}: {error}", options.binary.display());
            return ExitCode::from(EXIT_USAGE);
        }
    };

    let mut machine = Machine::new(Memory::new());
    machine.cpu_mut().set_pc(DRAM_BASE);
    if let Err(trap) = machine
        .bus_mut()
        .load_bytes(DRAM_BASE, &binary, MemoryType::Cacheable)
    {
        eprintln!("cannot load {}: {trap:?}", options.binary.display());
        return ExitCode::from(EXIT_USAGE);
    }

    let mut steps = 0;
    let outcome = loop {
        if steps == options.max_steps {
            break RunOutcome::StepLimitReached;
        }
        machine.step(StepOptions {
            trace: options.trace,
        });
        steps += 1;
        if machine.halted() {
            break RunOutcome::Halted;
        }
    };

    print_registers(machine.cpu());
    println!("steps = {steps}");

    match outcome {
        RunOutcome::Halted => ExitCode::SUCCESS,
        RunOutcome::StepLimitReached => {
            eprintln!("step limit reached after {steps} steps");
            ExitCode::from(EXIT_STEP_LIMIT)
        }
    }
}

/// Prints the general-purpose registers by ABI name.
fn print_registers(cpu: &Cpu) {
    for (index, name) in ABI_NAMES.iter().enumerate() {
        println!("{name} = {:#018x}", cpu.read_gpr(index));
    }
    for index in 0..FPR_COUNT {
        println!("f{index} = {:#018x}", cpu.read_fpr(index));
    }
    let fcsr = cpu.read_fcsr().unwrap_or(0);
    println!(
        "fcsr = {fcsr:#x} (fflags = {:#x}, frm = {})",
        fcsr & FFLAGS_MASK,
        (fcsr >> FRM_SHIFT) & FRM_MASK
    );
}
