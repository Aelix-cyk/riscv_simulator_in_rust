#[path = "support/assemble.rs"]
mod assemble;

use std::process::{Command, Output};

use riscv_simulator_in_rust::elf::ElfImage;
use riscv_simulator_in_rust::machine::{Machine, RunOutcome};
use riscv_simulator_in_rust::memory::{DRAM_BASE, DramLayout};
use riscv_simulator_in_rust::platform::Platform;

const STEP_LIMIT: u64 = 100;
const TEST_DRAM_BYTES: u64 = 1 << 16;
const A0_INDEX: usize = 10;
const EXPECTED_A0: u64 = 55;

#[test]
fn toolchain_elf_runs_through_loader() {
    let program = assemble::assemble("loop.S");
    let elf = program.elf_bytes();
    let layout = DramLayout::new(DRAM_BASE, TEST_DRAM_BYTES).unwrap();
    let image = ElfImage::parse(&elf, layout).unwrap();
    let mut machine = Machine::new(Platform::with_layout(layout).unwrap());
    image.load(machine.bus_mut()).unwrap();
    machine.cpu_mut().set_pc(image.entry());

    assert_eq!(machine.run(STEP_LIMIT), RunOutcome::Halted);
    assert_eq!(machine.cpu().read_gpr(A0_INDEX), EXPECTED_A0);
}

#[test]
fn cli_requires_the_matching_input_option() {
    let program = assemble::assemble("loop.S");
    let simulator = env!("CARGO_BIN_EXE_riscv_simulator_in_rust");

    let elf = Command::new(simulator)
        .args(["--elf", program.elf_path().to_str().unwrap()])
        .arg("--max-steps")
        .arg(STEP_LIMIT.to_string())
        .output()
        .expect("run simulator with ELF input");
    assert_success(&elf);

    let flat = Command::new(simulator)
        .args(["--binary", program.elf_path().to_str().unwrap()])
        .output()
        .expect("run simulator with mismatched input option");
    assert_eq!(flat.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&flat.stderr).contains("--elf"),
        "{}",
        String::from_utf8_lossy(&flat.stderr)
    );
}

fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "simulator failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
