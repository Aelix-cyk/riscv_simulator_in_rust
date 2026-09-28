# riscv_simulator_in_rust

A single-hart, little-endian RISC-V simulator written in Rust, targeting OS boot in a later phase.

**Conformance target:** RVA22S64. **Status:** documentation-first — no simulator code yet,
`src/main.rs` is still a stub, and v0 is slices S1–S2 of the plan.

## Principle

Correctness is proven, not asserted. The simulator's retirement trace is compared instruction by
instruction against Spike running the same binary with the same options; any single-byte
difference is a failure.

## Reference simulator

Pinned invocation, verified 2026-09-24 on `rv64ui-p-add` (12064 log lines, exit 0). `spike`
resolves through `$SPIKE`, or through `PATH` when that is unset:

    spike \
      --isa=rv64imafdc_zicsr_zifencei_zicntr_zihpm_zicclsm_zihintpause_zicbom_zicboz_zicbop_zca_zcd_zba_zbb_zbs_zfhmin_zaamo_zalrsc_svade_svpbmt_svinval \
      --priv=msu -l --log-commits --log=<ref>.log <binary>

Build identity: `Spike RISC-V ISA Simulator 1.1.1-dev`, `riscv-isa-sim.git` at `8fc5ab03`.

## Tooling you supply

| Variable | What to point it at |
|---|---|
| `$RISCV_TESTS_DIR` | a `riscv-tests` checkout — per-slice `rv64*-p-*` test sets, the working reference |
| `$RISCV_ARCH_TEST_DIR` | a `riscv-arch-test` checkout — RVA22S64 certification plans and coverpoints |
| `$OPENSBI_DIR` | an `opensbi` checkout — M-mode firmware source for the boot phase |
| `$RISCV_TOOLCHAIN_DIR` | a RISC-V GNU toolchain — compiler and binutils |

## Where things live

| Path | Contents |
|---|---|
| `docs/roles.md` | Who decides, who writes, who verifies — and the write zones |
| `docs/cooperation.md` | The work-order cycle between architect, assistant, implementer, verifier |
| `docs/adr/` | Append-only decision log, one decision per record |
| `docs/notes/rva22s64-scope.md` | The slice plan from S1 to OS boot |
| `docs/verification/`, `docs/implementation/` | Write zones for the verifier and the implementer |

## Build

    cargo build

The CLI and trace surfaces are not baselined yet; the verifier proposes them and the architect
approves before any simulator code is written.
