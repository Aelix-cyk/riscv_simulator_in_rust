//! End-to-end OpenSBI firmware boot gate.

#[path = "support/opensbi.rs"]
mod opensbi;

use std::env;
use std::fs;
use std::process::Command;

use riscv_simulator_in_rust::boot;
use riscv_simulator_in_rust::cpu::{A0, A1};
use riscv_simulator_in_rust::elf::ElfImage;
use riscv_simulator_in_rust::machine::{Machine, RunOutcome};
use riscv_simulator_in_rust::memory::DramLayout;
use riscv_simulator_in_rust::platform::Platform;

const STEP_LIMIT: u64 = 10_000_000;
const OPENSBI_BANNER: &str = "OpenSBI v1.9";
const PROBE_SUCCESS: &str = "ARVSR-PROBE-OK";
const MACHINE_EXCEPTION_PC_CSR: u16 = 0x341;
const MACHINE_CAUSE_CSR: u16 = 0x342;

#[test]
#[ignore = "requires OPENSBI_DIR, RISCV/DTC, and builds pinned OpenSBI v1.9"]
fn opensbi_hands_off_to_an_smode_probe() {
    let fixture = opensbi::OpenSbiFixture::build();
    let firmware_bytes = fs::read(&fixture.firmware).expect("read OpenSBI firmware");
    let device_tree = fs::read(&fixture.device_tree).expect("read device tree");
    let kernel_bytes = fs::read(&fixture.kernel).expect("read probe kernel");
    let dram = DramLayout::default();
    let firmware = ElfImage::parse_firmware(&firmware_bytes, dram, dram.base())
        .expect("parse OpenSBI firmware");
    let kernel = ElfImage::parse(&kernel_bytes, dram).expect("parse probe kernel");

    let mut machine = Machine::new(Platform::with_layout(dram).unwrap());
    let info = boot::load(dram, &firmware, &device_tree, &kernel, machine.bus_mut())
        .expect("load OpenSBI boot");
    machine.cpu_mut().write_gpr(A0, info.hart_id);
    machine.cpu_mut().write_gpr(A1, info.device_tree_address);
    machine.cpu_mut().set_pc(info.entry);

    let step_limit = env::var("OPENSBI_STEP_LIMIT")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(STEP_LIMIT);
    let outcome = machine.run(step_limit);
    let output = machine.bus_mut().take_console_output();
    let output = String::from_utf8_lossy(&output);
    let cpu = machine.cpu();
    assert_eq!(
        outcome,
        RunOutcome::Exited,
        "OpenSBI did not hand off and shut down: \
         pc={:#x}, mcause={:#x}, mepc={:#x}, \
         ra={:#x}, sp={:#x}, a0={:#x}, a1={:#x}, a2={:#x}, a3={:#x}, \
         t0={:#x}, t1={:#x}, t2={:#x}\n{output}",
        cpu.pc(),
        cpu.read_csr(MACHINE_CAUSE_CSR).unwrap_or(u64::MAX),
        cpu.read_csr(MACHINE_EXCEPTION_PC_CSR).unwrap_or(u64::MAX),
        cpu.read_gpr(1),
        cpu.read_gpr(2),
        cpu.read_gpr(10),
        cpu.read_gpr(11),
        cpu.read_gpr(12),
        cpu.read_gpr(13),
        cpu.read_gpr(5),
        cpu.read_gpr(6),
        cpu.read_gpr(7),
    );
    assert_eq!(machine.exit_code(), Some(0));
    assert!(output.contains(OPENSBI_BANNER), "{output}");
    assert!(output.contains(PROBE_SUCCESS), "{output}");

    let cli = Command::new(env!("CARGO_BIN_EXE_riscv_simulator_in_rust"))
        .arg("--firmware")
        .arg(&fixture.firmware)
        .arg("--dtb")
        .arg(&fixture.device_tree)
        .arg("--kernel")
        .arg(&fixture.kernel)
        .output()
        .expect("run simulator CLI");
    assert!(
        cli.status.success(),
        "CLI boot failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&cli.stdout),
        String::from_utf8_lossy(&cli.stderr)
    );
    let cli_output = String::from_utf8_lossy(&cli.stdout);
    assert!(cli_output.contains(OPENSBI_BANNER), "{cli_output}");
    assert!(cli_output.contains(PROBE_SUCCESS), "{cli_output}");
}
