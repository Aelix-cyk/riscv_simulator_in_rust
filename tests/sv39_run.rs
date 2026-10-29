//! End-to-end Sv39 translation fixture checked against Spike.

#[path = "support/assemble.rs"]
mod assemble;

use riscv_simulator_in_rust::bus::{Bus, MemoryType};
use riscv_simulator_in_rust::cpu::ABI_NAMES;
use riscv_simulator_in_rust::machine::{Machine, RunOutcome};
use riscv_simulator_in_rust::memory::{DRAM_BASE, Memory};

#[test]
fn sv39_fixture_matches_spike() {
    let program = assemble::assemble_supervisor("sv39.S");
    let mut machine = Machine::new(Memory::with_size(program.memory_len()));
    machine.cpu_mut().set_pc(assemble::LOAD_ADDRESS);
    machine
        .bus_mut()
        .load_bytes(DRAM_BASE, &program.binary, MemoryType::Cacheable)
        .unwrap();

    assert_eq!(machine.run(10_000), RunOutcome::Halted);
    assert_eq!(machine.cpu().pc(), program.halt_address());
    assert_eq!(register(&machine, "a7"), 0, "machine trap must not fire");
    assert_eq!(register(&machine, "s0"), 123);
    assert_eq!(register(&machine, "s2"), 13, "load page fault cause");
    assert_eq!(register(&machine, "s4"), 456);

    let spike = program.spike_state();
    for (index, expected) in spike.registers.iter().enumerate() {
        assert_eq!(machine.cpu().read_gpr(index), *expected, "x{index}");
    }
    for (offset, expected) in spike.memory.iter().enumerate() {
        let address = assemble::LOAD_ADDRESS + offset as u64;
        assert_eq!(
            machine
                .bus_mut()
                .read_u8(address, MemoryType::Cacheable)
                .unwrap(),
            *expected,
            "memory at {address:#x}"
        );
    }
}

fn register(machine: &Machine<Memory>, name: &str) -> u64 {
    let index = ABI_NAMES
        .iter()
        .position(|abi| *abi == name)
        .expect("known ABI register name");
    machine.cpu().read_gpr(index)
}
