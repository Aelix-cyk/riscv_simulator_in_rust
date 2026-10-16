//! Execution throughput on real fixture instructions:
//! `cargo bench --bench execute`.

#[path = "../tests/support/assemble.rs"]
mod assemble;

use std::hint::black_box;
use std::time::Instant;

use riscv_simulator_in_rust::bus::Bus;
use riscv_simulator_in_rust::decode::{Instruction, decode};
use riscv_simulator_in_rust::machine::Machine;
use riscv_simulator_in_rust::memory::{DRAM_BASE, Memory};

/// Instructions executed per measurement.
const ITERATIONS: u64 = 20_000_000;

fn main() {
    let program = assemble::assemble("alu.S");
    let words: Vec<u32> = program
        .binary
        .chunks_exact(4)
        .map(|chunk| u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect();
    let instructions: Vec<Instruction> =
        words.iter().filter_map(|word| decode(*word).ok()).collect();
    println!("corpus: {} instructions", instructions.len());

    let mut machine = Machine::new(Memory::with_size(program.memory_len()));
    machine.cpu_mut().set_pc(DRAM_BASE);
    machine
        .bus_mut()
        .load_bytes(DRAM_BASE, &program.binary)
        .unwrap();

    for instruction in &instructions {
        machine.execute(*instruction).unwrap();
    }

    report("execute", || {
        for index in 0..ITERATIONS {
            let instruction = black_box(instructions[index as usize % instructions.len()]);
            machine.execute(instruction).unwrap();
        }
    });

    report("decode + execute", || {
        for index in 0..ITERATIONS {
            let word = black_box(words[index as usize % words.len()]);
            if let Ok(instruction) = decode(word) {
                machine.execute(instruction).unwrap();
            }
        }
    });
}

/// Times `body` and prints its throughput.
fn report(label: &str, body: impl FnOnce()) {
    let start = Instant::now();
    body();
    let elapsed = start.elapsed();
    println!(
        "{label}: {elapsed:?}, {:.1} Mops/s",
        ITERATIONS as f64 / elapsed.as_secs_f64() / 1e6
    );
}
