# riscv_simulator_in_rust

aelix's RISC-V Simulator in Rust (aRVSR)

## Status

A 64-bit hart that executes RV64IMA plus Zicsr — the integer ALU operations, all
integer multiply/divide and atomic instructions, branches, jumps, loads and stores,
`fence`/`fence.i`, and the six CSR read/modify/write forms. Synchronous exceptions,
including `ecall` and `ebreak`, enter a machine-mode handler through `mtvec`.
Implemented CSRs include WARL `misa` (RV64IMA), `mstatus`, `mtvec`, `mscratch`,
`mepc`, `mcause`, and `mtval`. The target profile is RVA22S64, built up
progressively: see `GLOSSARY.md` for terminology, `docs/impl-choices/` for the
requirements and the choices the specification leaves open.

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
$RISCV/bin/riscv64-unknown-elf-gcc -march=rv64ima_zifencei_zicsr -mabi=lp64 -nostdlib \
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

External compliance gates are explicit because they require separate checkouts
and generate many programs:

```sh
export RISCV_TESTS_DIR="$HOME/tools/riscv-tests"
export RISCV_ARCH_TEST_DIR="$HOME/tools/riscv-arch-test"
cargo test --test compliance -- --ignored --nocapture
```

The ACT4 gate requires `mise`, `uv`, UDB, and `spike` on `PATH`. It generates
project-local ACT4 ELFs under `target/compliance/act4` by default. Known
unsupported features and deferred bugs live in
`tests/compliance/expected_failures.tsv`; listed passes or unlisted failures
fail the gate.

## Development tooling

- CLI agent: `codex`
- Model: `deepseek-flash`
