//! End-to-end RV64D fixture checked against Spike's complete architectural state.

#[path = "support/assemble.rs"]
mod assemble;

use riscv_simulator_in_rust::bus::Bus;
use riscv_simulator_in_rust::machine::{Machine, RunOutcome};
use riscv_simulator_in_rust::memory::{DRAM_BASE, Memory};

#[test]
fn d_fixture_matches_spike() {
    check_fixture("d.S");
}

#[test]
fn compressed_d_fixture_matches_spike() {
    check_fixture("compressed_d.S");
}

fn check_fixture(name: &str) {
    let program = assemble::assemble_double(name);
    let mut machine = Machine::new(Memory::with_size(program.memory_len()));
    machine.cpu_mut().set_pc(assemble::LOAD_ADDRESS);
    machine
        .bus_mut()
        .load_bytes(DRAM_BASE, &program.binary)
        .unwrap();

    assert_eq!(machine.run(10_000), RunOutcome::Halted, "{name}");
    assert_eq!(machine.cpu().pc(), program.halt_address());

    let spike = program.spike_state();
    for (index, expected) in spike.registers.iter().enumerate() {
        assert_eq!(machine.cpu().read_gpr(index), *expected, "x{index}");
    }
    for (index, expected) in spike.floating_registers.iter().enumerate() {
        assert_eq!(machine.cpu().read_fpr(index), *expected, "f{index}");
    }
    assert_eq!(machine.cpu().read_fcsr().unwrap(), spike.fcsr);
    for (offset, expected) in spike.memory.iter().enumerate() {
        let address = assemble::LOAD_ADDRESS + offset as u64;
        assert_eq!(
            machine.bus_mut().read_u8(address).unwrap(),
            *expected,
            "memory at {address:#x}"
        );
    }
}
