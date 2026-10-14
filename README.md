# riscv_simulator_in_rust

aelix's RISC-V Simulator in Rust (aRVSR)

## Status

Walking skeleton: a 64-bit hart that executes `addi`, `add`, `beq`, and `jal`,
with a test suite checked against `spike`. The target profile is RVA22S64, built up
progressively; see `GLOSSARY.md` for terminology.

## Usage

```sh
cargo run -- --binary path/to/program.bin [--trace] [--max-steps <N>]
```

The program is loaded at `DRAM_BASE` (`0x8000_0000`) and runs until it reaches the
halt self-loop, `jal x0, 0`, traps, or takes `--max-steps` steps (default 10 000).
On halt it prints the 32 general-purpose registers by ABI name and the step count.
`--trace` prints each executed instruction first, one per line.

Exit codes: `0` halted, `1` trap, `2` step limit or bad input.

## Building a program to run

The simulator takes a flat binary, so assemble and link a program, then strip it to
its loadable bytes. Fixtures use `tests/fixtures/link.ld`, which places the image at
`DRAM_BASE` with no leading gap:

```sh
$RISCV/bin/riscv64-unknown-elf-gcc -march=rv64i -mabi=lp64 -nostdlib \
    -I tests/fixtures -Wl,-T,tests/fixtures/link.ld \
    -o /tmp/loop.elf tests/fixtures/loop.S
$RISCV/bin/riscv64-unknown-elf-objcopy -O binary /tmp/loop.elf /tmp/loop.bin
cargo run -- --binary /tmp/loop.bin --trace
```

## Tests

```sh
cargo test                     # needs $RISCV and $SPIKE
cargo clippy --all-targets
```

Fixtures live in `tests/fixtures/`; `tests/support/assemble.rs` assembles them at
run time with the local toolchain and queries `spike` for the expected architectural
state, so no binaries are committed.
