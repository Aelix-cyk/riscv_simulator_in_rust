# riscv_simulator_in_rust

aelix's RISC-V Simulator in Rust (aRVSR)

## Status

A 64-bit hart that executes all of RV64I plus Zicsr — the integer ALU operations,
the six conditional branches, `lui`/`auipc`/`jal`/`jalr`, every load and store,
`fence`/`fence.i`, and the six CSR read/modify/write forms. Synchronous exceptions,
including `ecall` and `ebreak`, enter a machine-mode handler through `mtvec`; the
implemented CSRs are `mstatus`, `mtvec`, `mscratch`, `mepc`, `mcause`, and `mtval`.
The target profile is RVA22S64, built up progressively: see `GLOSSARY.md` for
terminology, `docs/impl-choices/` for the requirements and the choices the
specification leaves open, and `docs/notes/` for working notes.

## Usage

```sh
cargo run -- --binary path/to/program.bin [--trace] [--max-steps <N>]
```

The program is loaded at `DRAM_BASE` (`0x8000_0000`) and runs until it reaches the
halt self-loop, `jal x0, 0`, or takes `--max-steps` steps (default 10 000). On halt
it prints the 32 general-purpose registers by ABI name and the step count.
`--trace` prints each decoded instruction first, one per line.

Exit codes: `0` halted, `2` step limit or bad input. Exceptions do not terminate the
run; an invalid or missing trap handler eventually reaches the step limit.

## Building a program to run

The simulator takes a flat binary, so assemble and link a program, then strip it to
its loadable bytes. Fixtures use `tests/fixtures/link.ld`, which places the image at
`DRAM_BASE` with no leading gap:

```sh
$RISCV/bin/riscv64-unknown-elf-gcc -march=rv64im_zifencei_zicsr -mabi=lp64 -nostdlib \
    -I tests/fixtures -Wl,-T,tests/fixtures/link.ld \
    -o /tmp/loop.elf tests/fixtures/loop.S
$RISCV/bin/riscv64-unknown-elf-objcopy -O binary /tmp/loop.elf /tmp/loop.bin
cargo run -- --binary /tmp/loop.bin --trace
```

## Tests

```sh
cargo test                     # needs $RISCV and $SPIKE
cargo clippy --all-targets
cargo bench --bench decode     # decode throughput, optional CORPUS=fields
cargo bench --bench execute    # execution throughput
```

Fixtures live in `tests/fixtures/`; `tests/support/assemble.rs` assembles them at
run time with the local toolchain and queries `spike` for the expected architectural
state — all 32 registers and the loaded memory — so no binaries are committed. The
decoder and trace text are additionally checked line by line against `objdump`.
