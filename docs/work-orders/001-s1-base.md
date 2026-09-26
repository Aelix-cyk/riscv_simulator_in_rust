# Work order 001 — S1 base slice

**Status:** frozen (architect approved `docs/verification/s1-freeze.md` on 2026-09-26, ADR 006)
**Owner:** implementer (`src/**`), verifier (`tests/**`, `docs/verification/**`)
**Branch:** `codex/s1-isa-freeze` · **Plan:** `docs/notes/s1-plan.md`

## Goal

Run a riscv-tests pseudo-ELF to completion and emit a retirement trace whose retirement events
match Spike's, after the normalization the freeze defines, on all 71 S1 binaries: 54 `rv64ui-p-*`
and 17 `rv64mi-p-*`.

## Acceptance

    cargo test
    cargo run --release -- <elf> --trace

Retirement-equivalent post-normalization and exit-status-identical on 71/71 binaries, twice in a
row for determinism.

Two limits, approved by the architect on 2026-09-26:

- The gate is equivalence **after** the documented normalization, never byte-identical stdout.
- Trace equality is necessary but not sufficient: state no test reads stays unverified.

Any divergence is either a pinned-configuration mismatch (fix the configuration) or a simulator
bug (fix the code). Adding a normalization to hide it is not an option; the verifier must approve
any change to the list in the freeze.

## Depends on

`docs/verification/s1-freeze.md` — the verifier drafts the observable surface (CLI, trace line,
difference rules, machine state, CSR table, trap and interrupt behaviour), the architect approves
it, and only then does this work order become `frozen` and implementation start.

## Method order (architect approves each method before it is written)

| # | Method | Deliverable |
|---|---|---|
| M1 | skeleton | CLI, ELF loader, sparse 4 KiB page memory, halt protocol |
| M2 | decode | pure `decode(word) -> Insn`, illegal-encoding detection |
| M3 | hart/CSR/trap | hart state, CSR WARL table, trap entry, `mret`, U-mode, interrupts |
| M4 | execute | RV64I semantics via dispatch table |
| M5 | trace | emitter, exit codes, differential loop until the corpus is green |

## Zones

Implementer writes `src/**` and `docs/implementation/**`; verifier writes `tests/**` and
`docs/verification/**`; the assistant writes work orders, ADRs and notes. Anything outside your
zone needs a one-line permission request recorded here.

Permission granted 2026-09-26 by the architect: the assistant may edit
`docs/verification/s1-freeze.md` to apply the two corrections from
`docs/notes/s1-freeze-review.md` and to flip its status line. Expires when this work order is
accepted.

## Evidence

- Implementer: files touched, commands run, failures seen, one line per method.
- Verifier: `docs/verification/reports/001-s1.md` with the corpus result and any first divergence.
