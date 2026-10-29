# riscv_simulator_in_rust

aelix's RISC-V Simulator in Rust (aRVSR)

## Status

A 64-bit hart that executes RV64IMAFDCB with M/S/U privilege plus Zicsr and Zicntr —
the integer ALU operations, all integer multiply/divide and atomic instructions,
the Zba/Zbb/Zbs bit-manipulation instructions,
64-byte cache-block management, zero, and prefetch operations, `pause`,
single-, double-, and half-precision floating point, Zca/Zcd compressed instructions,
branches, jumps, loads and stores,
`fence`/`fence.i`, the six CSR read/modify/write forms, and deterministic cycle,
time, instruction, and three event-selectable hardware performance counters.
Synchronous exceptions, including `ecall` and `ebreak`, enter a machine-mode
handler through `mtvec`, or a delegated supervisor handler through `stvec`.
Implemented CSRs include WARL `misa` (RV64IMAFDCBSU), `fcsr`/`fflags`/`frm`,
machine and supervisor identity/status/trap/delegation registers, and counter
controls. `satp` supports Bare and Sv39 modes with a 16-bit ASID. Sv39 uses
4 KiB and superpage mappings, software A/D updates, PBMT memory types, and a
64-entry FIFO TLB. Fine-grained invalidation is provided by Svinval's
`SINVAL.VMA`, `SFENCE.W.INVAL`, and `SFENCE.INVAL.IR`.
HPM counters select retired instructions, loads, stores, and branches/jumps
through one-hot event masks. The target profile is RVA22S64, built up
progressively: see `GLOSSARY.md` for terminology, `docs/impl-choices/` for the
requirements and the choices the specification leaves open.

The CLI runs on a small platform bus with a CLINT timer, NS16550-compatible
UART, PLIC, and an explicit exit device. Machine and supervisor interrupt state
is connected to device interrupt lines, allowing timer and external interrupts
to enter the architectural trap paths.

## Usage

```sh
cargo run -- --binary path/to/program.bin [--trace] [--max-steps <N>] \
    [--dram-base <ADDRESS>] [--dram-size <SIZE>]
```

The program is loaded at the configured DRAM base and runs until it exits through
the platform device, reaches the halt self-loop `jal x0, 0`, or takes
`--max-steps` steps (default 10 000). The default DRAM layout is 128 MiB starting
at `0x8000_0000`. `--dram-base` accepts decimal or hexadecimal addresses;
`--dram-size` accepts bytes or `KiB`, `MiB`, and `GiB` suffixes. Both values
must be 4 KiB-aligned, and the complete range must fit the 57-bit physical
address space without overlapping platform MMIO.

On termination the simulator flushes UART output, prints the 32 general-purpose
registers by ABI name, all FPR bit patterns, decoded FCSR fields, and the step
count. `--trace` prints each decoded instruction first, one per line.

Exit codes: the platform exit status, `0` for a halt self-loop, or `2` for a
step limit or bad input. Exceptions do not terminate the run; an invalid or
missing trap handler eventually reaches the step limit.

## Platform memory map

| Device | Address | Model |
|---|---:|---|
| Syscon | `0x0010_0000` | Write `0x0000_5555` to exit successfully or `(code << 16) \| 0x3333` to exit with `code` |
| CLINT | `0x0200_0000` | Machine software interrupt at offset `0x0`; 64-bit `mtimecmp` at `0x4000`; writable 64-bit `mtime` at `0xbff8` |
| PLIC | `0x0c00_0000` | Priority, pending, enable, threshold, and claim/complete for machine and supervisor contexts |
| UART | `0x1000_0000` | NS16550-style transmit, receive, interrupt-enable, interrupt-ID, and line-status registers |
| DRAM | `0x8000_0000` by default | 128 MiB of little-endian memory by default; base and size are CLI-configurable |

The PLIC exposes one source: UART receive interrupts. The UART receive queue is
available through the `Platform` API for tests and host front ends.

## Building a program to run

The simulator takes a flat binary, so assemble and link a program, then strip it to
its loadable bytes. Fixtures use `tests/fixtures/link.ld`, which places the image at
`DRAM_BASE` with no leading gap:

```sh
$RISCV/bin/riscv64-unknown-elf-gcc -march=rv64imafc_zfh_zicsr_zicntr_zifencei -mabi=lp64 -nostdlib \
    -I tests/fixtures -Wl,-T,tests/fixtures/link.ld \
    -o /tmp/loop.elf tests/fixtures/loop.S
$RISCV/bin/riscv64-unknown-elf-objcopy -O binary /tmp/loop.elf /tmp/loop.bin
cargo run -- --binary /tmp/loop.bin --trace
```

The linker script targets the default DRAM base. When running at a different
`--dram-base`, link the program there if it contains absolute address
assumptions; PC-relative programs can be relocated by changing only the load
base.

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
the three B families (`rv64uzba`, `rv64uzbb`, `rv64uzbs`), `rv64mi`, `rv64si`,
`rv64uf`, `rv64ud`, and `rv64uzfh`; the ACT4 configuration covers B bit
manipulation, F/D/H floating point, M/S/U behavior, and Sv39/Svade translation.

In this development environment, `uv` is not directly on the project `PATH`.
It is provided through `mise` from the external architecture-test checkout:

```sh
(cd "$RISCV_ARCH_TEST_DIR" && mise exec -- uv --version)
```

The current `mise` environment provides `uv 0.11.33`.

## Development tooling

- CLI agent: `codex`
- Model: `deepseek-flash`
