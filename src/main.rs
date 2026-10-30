//! Command-line front end: load a flat binary or ELF executable, run it, and
//! report the result.

use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::ExitCode;

use riscv_simulator_in_rust::boot;
use riscv_simulator_in_rust::bus::{Bus, MemoryType};
use riscv_simulator_in_rust::cli::{self, Command, Options, ProgramInput, USAGE};
use riscv_simulator_in_rust::cpu::{A0, A1, ABI_NAMES, Cpu, FPR_COUNT};
use riscv_simulator_in_rust::elf::{ElfImage, is_elf};
use riscv_simulator_in_rust::machine::{Machine, RunOutcome, StepOptions};
use riscv_simulator_in_rust::platform::Platform;

/// Exit code for usage and input errors.
const EXIT_USAGE: u8 = 2;

/// Exit code for a run that stopped at its step limit.
const EXIT_STEP_LIMIT: u8 = 2;
/// Exit code used when a platform reports exit without a status.
const EXIT_DEVICE_FAILURE: u8 = 1;

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
    let platform = match Platform::with_layout(options.dram) {
        Ok(platform) => platform,
        Err(error) => {
            eprintln!("invalid DRAM configuration: {error}");
            return ExitCode::from(EXIT_USAGE);
        }
    };
    let mut machine = Machine::new(platform);
    let entry = match &options.input {
        ProgramInput::Flat(path) => {
            let binary = match read_input(path) {
                Ok(binary) => binary,
                Err(()) => return ExitCode::from(EXIT_USAGE),
            };
            if is_elf(&binary) {
                eprintln!("ELF input must be supplied with --elf");
                return ExitCode::from(EXIT_USAGE);
            }
            if let Err(trap) =
                machine
                    .bus_mut()
                    .load_bytes(options.dram.base(), &binary, MemoryType::Cacheable)
            {
                eprintln!("cannot load {}: {trap:?}", path.display());
                return ExitCode::from(EXIT_USAGE);
            }
            options.dram.base()
        }
        ProgramInput::Elf(path) => {
            let binary = match read_input(path) {
                Ok(binary) => binary,
                Err(()) => return ExitCode::from(EXIT_USAGE),
            };
            let image = match ElfImage::parse(&binary, options.dram) {
                Ok(image) => image,
                Err(error) => {
                    eprintln!("cannot load {}: {error}", path.display());
                    return ExitCode::from(EXIT_USAGE);
                }
            };
            if let Err(error) = image.load(machine.bus_mut()) {
                eprintln!("cannot load {}: {error}", path.display());
                return ExitCode::from(EXIT_USAGE);
            }
            image.entry()
        }
        ProgramInput::Boot(boot_input) => {
            let firmware = match read_input(&boot_input.firmware) {
                Ok(binary) => binary,
                Err(()) => return ExitCode::from(EXIT_USAGE),
            };
            let device_tree = match read_input(&boot_input.device_tree) {
                Ok(binary) => binary,
                Err(()) => return ExitCode::from(EXIT_USAGE),
            };
            let kernel = match read_input(&boot_input.kernel) {
                Ok(binary) => binary,
                Err(()) => return ExitCode::from(EXIT_USAGE),
            };
            let firmware =
                match ElfImage::parse_firmware(&firmware, options.dram, options.dram.base()) {
                    Ok(image) => image,
                    Err(error) => {
                        eprintln!("cannot load {}: {error}", boot_input.firmware.display());
                        return ExitCode::from(EXIT_USAGE);
                    }
                };
            let kernel = match ElfImage::parse(&kernel, options.dram) {
                Ok(image) => image,
                Err(error) => {
                    eprintln!("cannot load {}: {error}", boot_input.kernel.display());
                    return ExitCode::from(EXIT_USAGE);
                }
            };
            let info = match boot::load(
                options.dram,
                &firmware,
                &device_tree,
                &kernel,
                machine.bus_mut(),
            ) {
                Ok(info) => info,
                Err(error) => {
                    eprintln!("cannot boot firmware: {error}");
                    return ExitCode::from(EXIT_USAGE);
                }
            };
            machine.cpu_mut().write_gpr(A0, info.hart_id);
            machine.cpu_mut().write_gpr(A1, info.device_tree_address);
            info.entry
        }
    };
    machine.cpu_mut().set_pc(entry);

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
        if machine.exit_code().is_some() {
            break RunOutcome::Exited;
        }
    };

    let _ = std::io::stdout().write_all(&machine.bus_mut().take_console_output());
    print_registers(machine.cpu());
    println!("steps = {steps}");

    match outcome {
        RunOutcome::Halted | RunOutcome::Exited => ExitCode::from(machine.exit_code().unwrap_or(
            if outcome == RunOutcome::Halted {
                0
            } else {
                EXIT_DEVICE_FAILURE
            },
        )),
        RunOutcome::StepLimitReached => {
            eprintln!("step limit reached after {steps} steps");
            ExitCode::from(EXIT_STEP_LIMIT)
        }
    }
}

fn read_input(path: &Path) -> Result<Vec<u8>, ()> {
    fs::read(path).map_err(|error| {
        eprintln!("cannot read {}: {error}", path.display());
    })
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
