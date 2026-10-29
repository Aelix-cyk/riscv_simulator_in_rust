#[path = "support/assemble.rs"]
mod assemble;

use riscv_simulator_in_rust::bus::{Bus, MemoryType};
use riscv_simulator_in_rust::machine::{Machine, RunOutcome};
use riscv_simulator_in_rust::memory::{DRAM_BASE, DRAM_PAGE_BYTES, DramLayout, Memory};
use riscv_simulator_in_rust::platform::Platform;

const STEP_LIMIT: u64 = 2000;
const TIMER_INTERRUPT_CAUSE: u64 = (1 << 63) | 7;
const MACHINE_CAUSE_CSR: u16 = 0x342;
const EXPECTED_CONSOLE_OUTPUT: &[u8] = b"TO\n";
const OLD_DRAM_SIZE: u64 = 1 << 20;
const TEST_DRAM_SIZE: u64 = 2 * OLD_DRAM_SIZE;
const CUSTOM_DRAM_BASE: u64 = 0x9000_0000;
const TEST_VALUE: u64 = 0x1122_3344;

#[test]
fn platform_program_uses_uart_timer_interrupt_and_exit_device() {
    let program = assemble::assemble("platform.S");
    let mut machine = Machine::new(Platform::new(Memory::with_size(DRAM_PAGE_BYTES as usize)));
    machine.cpu_mut().set_pc(assemble::LOAD_ADDRESS);
    machine
        .bus_mut()
        .load_bytes(
            assemble::LOAD_ADDRESS,
            &program.binary,
            MemoryType::Cacheable,
        )
        .unwrap();

    assert_eq!(machine.run(STEP_LIMIT), RunOutcome::Exited);
    assert_eq!(machine.exit_code(), Some(0));
    assert_eq!(machine.cpu().read_gpr(10), 1);
    assert_eq!(
        machine.cpu().read_csr(MACHINE_CAUSE_CSR),
        Some(TIMER_INTERRUPT_CAUSE)
    );
    assert_eq!(
        machine.bus_mut().take_console_output(),
        EXPECTED_CONSOLE_OUTPUT
    );
}

#[test]
fn dram_fixture_runs_above_1_mib_at_default_and_custom_bases() {
    let program = assemble::assemble("dram_growth.S");

    for base in [DRAM_BASE, CUSTOM_DRAM_BASE] {
        let layout = DramLayout::new(base, TEST_DRAM_SIZE).unwrap();
        let mut machine = Machine::new(Platform::with_layout(layout).unwrap());
        machine.cpu_mut().set_pc(base);
        machine
            .bus_mut()
            .load_bytes(base, &program.binary, MemoryType::Cacheable)
            .unwrap();

        assert_eq!(machine.run(STEP_LIMIT), RunOutcome::Exited);
        assert_eq!(machine.exit_code(), Some(0));
        assert_eq!(machine.cpu().read_gpr(10), 1);
        assert_eq!(
            machine
                .bus_mut()
                .read_u64(base + OLD_DRAM_SIZE, MemoryType::Cacheable)
                .unwrap(),
            TEST_VALUE
        );
    }
}
