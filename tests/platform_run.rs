#[path = "support/assemble.rs"]
mod assemble;

use riscv_simulator_in_rust::bus::{Bus, MemoryType};
use riscv_simulator_in_rust::machine::{Machine, RunOutcome};
use riscv_simulator_in_rust::memory::Memory;
use riscv_simulator_in_rust::platform::Platform;

const STEP_LIMIT: u64 = 2000;
const TIMER_INTERRUPT_CAUSE: u64 = (1 << 63) | 7;
const MACHINE_CAUSE_CSR: u16 = 0x342;
const EXPECTED_CONSOLE_OUTPUT: &[u8] = b"TO\n";

#[test]
fn platform_program_uses_uart_timer_interrupt_and_exit_device() {
    let program = assemble::assemble("platform.S");
    let mut machine = Machine::new(Platform::new(Memory::with_size(program.memory_len())));
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
