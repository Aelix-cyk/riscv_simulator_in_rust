//! End-to-end: assemble a fixture, load it, and run it on a `Machine`.

#[path = "support/assemble.rs"]
mod assemble;

use riscv_simulator_in_rust::bus::Bus;
use riscv_simulator_in_rust::machine::{Machine, RunOutcome};
use riscv_simulator_in_rust::memory::{DRAM_BASE, Memory};

#[test]
fn loop_fixture_halts_with_spike_state() {
    let program = assemble::assemble("loop.S");
    let mut machine = Machine::new(Memory::with_size(program.binary.len()));
    machine.cpu_mut().set_pc(assemble::LOAD_ADDRESS);
    machine
        .bus_mut()
        .load_bytes(DRAM_BASE, &program.binary)
        .unwrap();

    let outcome = machine.run(1_000).unwrap();

    assert_eq!(outcome, RunOutcome::Halted);
    assert_eq!(machine.cpu().pc(), program.halt_address());

    let spike = program.spike_registers();
    for (index, spike_value) in spike.iter().enumerate() {
        assert_eq!(machine.cpu().read_gpr(index), *spike_value, "x{index}");
    }
}

#[test]
fn spin_fixture_stops_at_the_step_limit() {
    let program = assemble::assemble("spin.S");
    let mut machine = Machine::new(Memory::with_size(program.binary.len()));
    machine.cpu_mut().set_pc(assemble::LOAD_ADDRESS);
    machine
        .bus_mut()
        .load_bytes(DRAM_BASE, &program.binary)
        .unwrap();

    assert_eq!(machine.run(3).unwrap(), RunOutcome::StepLimitReached);
}
