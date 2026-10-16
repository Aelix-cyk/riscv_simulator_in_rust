//! Decode throughput on real fixture instructions: `cargo bench --bench decode`.

#[path = "../tests/support/assemble.rs"]
mod assemble;

use std::hint::black_box;
use std::time::Instant;

use riscv_simulator_in_rust::decode::decode;

/// Words decoded per measurement.
const ITERATIONS: u64 = 10_000_000;

fn main() {
    let names: &[&str] = match std::env::var("CORPUS").as_deref() {
        Ok("fields") => &["fields.S"],
        _ => &["alu.S", "fields.S"],
    };
    let words = fixture_words(names);
    println!("corpus: {} ({} words)", names.join("+"), words.len());

    for word in &words {
        black_box(decode(*word).is_ok());
    }

    let start = Instant::now();
    let mut decoded = 0;
    for index in 0..ITERATIONS {
        let word = words[index as usize % words.len()];
        if decode(black_box(word)).is_ok() {
            decoded += 1;
        }
    }
    let elapsed = start.elapsed();

    println!("decoded {decoded} of {ITERATIONS}");
    println!("elapsed: {elapsed:?}");
    println!(
        "throughput: {:.1} Mdecodes/s",
        ITERATIONS as f64 / elapsed.as_secs_f64() / 1e6
    );
}

/// Instruction words of the named fixtures, in memory order.
fn fixture_words(names: &[&str]) -> Vec<u32> {
    let mut words = Vec::new();
    for name in names {
        let program = assemble::assemble(name);
        words.extend(
            program
                .binary
                .chunks_exact(4)
                .map(|chunk| u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]])),
        );
    }
    words
}
