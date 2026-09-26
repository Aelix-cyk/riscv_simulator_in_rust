# S1 plan (approved 2026-09-24)

Owner of this note: assistant. Execution order and gates live in
`docs/work-orders/001-s1-base.md`; the frozen surface lives in
`docs/verification/s1-freeze.md`.

## Goal

Run a riscv-tests pseudo-ELF to completion and emit a retirement trace whose retirement events
match Spike's on all 71 S1 binaries: 54 `rv64ui-p-*` and 17 `rv64mi-p-*` (the earlier 108/34
counts included `.dump` files).

Success = **retirement equivalence after the documented normalization**, plus exit-status
equality, on 71/71, twice in a row for determinism, with `cargo test` green.

Two corrections the architect approved on 2026-09-26:

1. **Equivalence is post-normalization, not identical stdout.** Spike's boot-ROM prefix, symbol
   lines, CSR-write annotations and host-derived values are normalized away and the list lives in
   the freeze. The gate is never "our bytes equal Spike's bytes".
2. **Trace equality is necessary but not sufficient.** Architectural state no test reads — CSR
   values, `satp`, PMP configuration — stays unverified by the differ. The corpus is a floor for
   S1, not a conformance proof; `riscv-arch-test` coverpoints are what eventually close that gap.

Divergence triage rule: if a difference traces back to an oracle flag (the `zicclsm` case: Spike
without it takes the exception path on misaligned data access, with it commits the access), fix
the pinned configuration. If it traces back to behaviour, it is a bug in the simulator. Never a
third option: silently widening a normalization.

## Oracle configuration

    /opt/riscv/bin/spike \
      --isa=rv64imafdc_zicsr_zifencei_zicntr_zihpm_zicclsm_zihintpause_zicbom_zicboz_zicbop_zca_zcd_zba_zbb_zbs_zfhmin_zaamo_zalrsc_svade_svpbmt_svinval \
      --priv=msu --triggers=0 -l --log-commits --log=<ref>.log <binary>

Spike 1.1.1-dev @ `8fc5ab03`. Environment facts the corpus proved, which the plan must respect:

- The test body runs in **U-mode**: `reset_vector` does `csrwi mstatus,0`, `csrw mepc`, `mret`.
- Every test's setup **deliberately traps** on `mnstatus` (Smrnmi absent) and expects `mtval` =
  the encoding; `INIT_SATP`/`INIT_PMP`/`DELEGATE_NO_TRAPS` write satp, pmpcfg/addr, medeleg, mideleg.
- Misaligned **data** access must work (`ma_data` fails without `zicclsm`); misaligned **fetch** traps.
- `rv64mi-p-illegal` takes a **vectored interrupt #1** after `csrsi mstatus, 8`, because
  `csrwi mip, 2` sets SSIP and `csrwi mie, 2` enables it. `mip` reads `0x82` (MTIP is pending from
  Spike's CLINT model), so the interrupt path is real, not hypothetical.
- `misa = 0x800000000014112F`, `marchid = 5`, `mvendorid = mimpid = mhartid = 0`.
- `pmpaddr` masks to bits [53:0] (`0x003FFFFFFFFFFFFF`); granularity is 4 bytes.
- Counters are never compared in S1: no binary reads `cycle`/`time`/`instret` into a non-x0 register.

## Method order and gates

Each method is approved by the architect before it is written; the implementer owns `src/**`, the
verifier owns `tests/**` and `docs/verification/**`, the assistant owns work orders and this note.

| # | Method | Deliverable | State |
|---|---|---|---|
| M1 | skeleton | CLI, ELF loader, sparse page memory, halt protocol | not started |
| M2 | decode | pure `decode(word) -> Insn`, illegal detection | not started |
| M3 | hart/CSR/trap | hart state, CSR WARL table, trap entry, `mret`, U-mode, interrupts | not started |
| M4 | execute | RV64I semantics via dispatch table | not started |
| M5 | trace | emitter, exit codes, differential loop to green | not started |

## Interfaces

Full detail in `docs/verification/s1-freeze.md`. Summary:

- `simulator <elf> --trace`; exit 0 for `tohost == 1`, `tohost >> 1` otherwise, 2 on loader error.
- Commit line `0x<pc> 0x<enc> p<priv> [xN=0x<val>]... [mem 0x<addr>[=0x<val>]]`, trap line
  `trap <name> p<priv> epc=0x<epc> tval=0x<tval>`.
- Differ drops Spike's ROM prefix and symbol lines, strips `c<dec>_<name> 0x<hex>` annotations,
  folds the two-line exception form into one trap line.
- Sparse 4 KiB pages; unmapped access traps; `tohost` store halts after the line is emitted.

## Test plan

1. Unit: decoder words and illegal encodings, CSR WARL read-backs, misaligned access paths.
2. Differential: `tests/s1_differential.rs` runs the oracle and the simulator per binary, normalizes
   both sides, and reports the first divergent index with both lines.
3. Acceptance: 71/71 trace-identical, exit-status-identical, rerun twice.
4. Negative: `mnstatus` traps with `tval`=encoding, odd-address `jalr` traps, unmapped load traps.

## Current state

- `src/` holds only the `main.rs` stub: the earlier drafts were removed, so the implementer starts
  from a clean room. Nothing is written yet for M1–M5.
- `docs/verification/s1-freeze.md` is approved; work order 001 is therefore `frozen` and M1 may
  start. Policy recorded in ADR 006 — retirement equivalence after a closed normalization list,
  with CSR writes compared rather than dropped.
- Two corrections the review of the freeze requires (trap entry logs no CSR write; §5.5's wording)
  are still outstanding in the verifier's file.
- **Scope correction:** S1 is M/U/**S** modes, not M/U — `rv64mi-p-illegal` runs 30 commits in
  S-mode, uses `sret`, and takes one `supervisor_ecall`.
- **Sequencing (architect, 2026-09-26):** M1 waits for the verifier's failing Spike-diff harness,
  so implementation starts against a red test rather than against nothing. The harness must fail as
  an assertion, not as a compile error — subprocess the stub binary, do not call a library API that
  does not exist yet.
