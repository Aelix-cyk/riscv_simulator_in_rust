//! Fixtures run on the hart, and the whole machine state must match spike.

#[path = "support/assemble.rs"]
mod assemble;

use riscv_simulator_in_rust::bus::Bus;
use riscv_simulator_in_rust::machine::{Machine, RunOutcome};
use riscv_simulator_in_rust::memory::{DRAM_BASE, Memory};

/// Fixtures that run to the halt self-loop.
const FIXTURES: [&str; 10] = [
    "alu.S", "upper.S", "branch.S", "memory.S", "fence.S", "trap.S", "mul.S", "div.S", "lrsc.S",
    "amo.S",
];

#[test]
fn fixtures_match_spike() {
    for fixture in FIXTURES {
        check_fixture(fixture);
    }
}

/// Runs one fixture and compares registers and memory against spike.
fn check_fixture(name: &str) {
    let program = assemble::assemble(name);
    let mut machine = Machine::new(Memory::with_size(program.memory_len()));
    machine.cpu_mut().set_pc(assemble::LOAD_ADDRESS);
    machine
        .bus_mut()
        .load_bytes(DRAM_BASE, &program.binary)
        .unwrap();

    assert_eq!(
        machine.run(10_000),
        RunOutcome::Halted,
        "{name} did not halt"
    );
    assert_eq!(
        machine.cpu().pc(),
        program.halt_address(),
        "{name} halted at the wrong address"
    );

    let spike = program.spike_state();
    for (index, expected) in spike.registers.iter().enumerate() {
        assert_eq!(machine.cpu().read_gpr(index), *expected, "{name}: x{index}");
    }
    for (offset, expected) in spike.memory.iter().enumerate() {
        let address = assemble::LOAD_ADDRESS + offset as u64;
        assert_eq!(
            machine.bus_mut().read_u8(address).unwrap(),
            *expected,
            "{name}: memory at {address:#x}"
        );
    }
}
