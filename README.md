# riscv_simulator_in_rust

aelix's RISC-V Simulator in Rust (aRVSR)

## Status

A 64-bit hart that executes RV64IMAFDCB with M/U privilege plus Zicsr and Zicntr —
the integer ALU operations, all integer multiply/divide and atomic instructions,
the Zba/Zbb/Zbs bit-manipulation instructions,
single- and double-precision floating point, Zca/Zcd compressed instructions,
branches, jumps, loads and stores,
`fence`/`fence.i`, the six CSR read/modify/write forms, and deterministic cycle,
time, instruction, and three event-selectable hardware performance counters.
Synchronous exceptions, including `ecall` and `ebreak`, enter a machine-mode
handler through `mtvec`. Implemented CSRs include WARL `misa`
(RV64IMAFDCBU), `fcsr`/`fflags`/`frm`, machine identity/status/trap/delegation
registers, and counter controls. HPM counters select retired instructions,
loads, stores, and branches/jumps through one-hot event masks. The target
profile is RVA22S64, built up progressively: see `GLOSSARY.md` for terminology,
`docs/impl-choices/` for the requirements and the choices the specification
leaves open.

## Usage

```sh
cargo run -- --binary path/to/program.bin [--trace] [--max-steps <N>]
```

The program is loaded at `DRAM_BASE` (`0x8000_0000`) and runs until it reaches the
halt self-loop, `jal x0, 0`, or takes `--max-steps` steps (default 10 000). On halt
it prints the 32 general-purpose registers by ABI name, all FPR bit patterns,
decoded FCSR fields, and the step count.
`--trace` prints each decoded instruction first, one per line.

Exit codes: `0` halted, `2` step limit or bad input. Exceptions do not terminate the
run; an invalid or missing trap handler eventually reaches the step limit.

## Building a program to run

The simulator takes a flat binary, so assemble and link a program, then strip it to
its loadable bytes. Fixtures use `tests/fixtures/link.ld`, which places the image at
`DRAM_BASE` with no leading gap:

```sh
$RISCV/bin/riscv64-unknown-elf-gcc -march=rv64imafc_zifencei_zicsr_zicntr -mabi=lp64 -nostdlib \
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
state — all GPRs, FPRs, FCSR, and the loaded memory — so no binaries are
committed. The decoder and trace text are additionally checked line by line
against `objdump`.

External compliance gates are explicit because they require separate checkouts
and generate many programs:

```sh
export RISCV_TESTS_DIR="$HOME/tools/riscv-tests"
export RISCV_ARCH_TEST_DIR="$HOME/tools/riscv-arch-test"
export ACT4_EXTENSIONS=D      # optional target filter
cargo test --test compliance -- --ignored --nocapture
```

The ACT4 gate requires `mise`, `uv`, UDB, and `spike` on `PATH`. It generates
project-local ACT4 ELFs under `target/compliance/act4` by default. Known
unsupported features and deferred bugs live in
`tests/compliance/expected_failures.tsv`; listed passes or unlisted failures
fail the gate. The legacy gate includes `rv64ui`, `rv64um`, `rv64ua`, `rv64uc`,
the three B families (`rv64uzba`, `rv64uzbb`, `rv64uzbs`), `rv64mi`, `rv64uf`,
and `rv64ud`; the ACT4 configuration covers B bit manipulation plus F/D
arithmetic, flags, rounding, conversions, and transfer instructions.

In this development environment, `uv` is not directly on the project `PATH`.
It is provided through `mise` from the external architecture-test checkout:

```sh
(cd "$RISCV_ARCH_TEST_DIR" && mise exec -- uv --version)
```

The current `mise` environment provides `uv 0.11.33`.

## Development tooling

- CLI agent: `codex`
- Model: `deepseek-flash`
