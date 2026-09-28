# Work order 001 — S1 base slice

**Status:** baselined (architect approved `docs/verification/s1-surface.md` on 2026-09-26, ADR 006)
**Owner:** implementer (`src/**`), verifier (`tests/**`, `docs/verification/**`)
**Branch:** `codex/s1-isa-freeze` · **Plan:** `docs/notes/s1-plan.md`

## Goal

Run a riscv-tests pseudo-ELF to completion and emit a retirement trace whose retirement events
match Spike's, after the normalization the baseline defines, on all 71 S1 binaries: 54 `rv64ui-p-*`
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
any change to the list in the baseline.

## Depends on

`docs/verification/s1-surface.md` — the verifier drafts the observable surface (CLI, trace line,
difference rules, machine state, CSR table, trap and interrupt behaviour), the architect approves
it, and only then does this work order become `baselined` and implementation start.

Satisfied 2026-09-26: the baseline is approved and this order is `baselined`. The remaining start
condition is the verifier's failing harness (architect, 2026-09-26): **M1 does not begin until the
Spike-diff harness is in `tests/`**, so the first line of simulator code already has a red test
waiting for it.

Harness shape, so the red state is useful: it must fail as an **assertion**, not as a compile
error. Drive the simulator as a subprocess (`cargo run --release -- <elf> --trace`) rather than
through a library API that does not exist yet, so a stub `main.rs` produces "first divergence at
record 0" instead of breaking `cargo test` for every other test.

## Method order (architect approves each method before it is written)

| # | Method | Deliverable |
|---|---|---|
| M1 | skeleton | CLI, ELF loader, sparse 4 KiB page memory, halt protocol |
| M2 | decode | pure `decode(word) -> Insn`, illegal-encoding detection |
| M3 | hart/CSR/trap | hart state, CSR WARL table, trap entry, `mret`, U-mode, interrupts |
| M4 | execute | RV64I semantics via dispatch table |
| M5 | trace | emitter, exit codes, differential loop until the test set is green |

## Zones

Implementer writes `src/**` and `docs/implementation/**`; verifier writes `tests/**` and
`docs/verification/**`; the assistant writes work orders, ADRs and notes. Anything outside your
zone needs a one-line permission request recorded here.

Permission granted 2026-09-26 by the architect: the assistant may edit
`docs/verification/s1-surface.md` to apply the two corrections from
`docs/notes/audit-index.md` (the review itself removed 2026-09-27) and to flip its status line. Expires when this work order is
accepted.

Permission granted 2026-09-26 by the architect: the assistant may sweep the term “corpus” →
“test set” across `docs/verification/**` and the living notes, leaving accepted ADRs untouched.
Expires when this work order is accepted.

Approved 2026-09-27 by the architect: `docs/verification/s1-surface.md` §4 drops `p<priv>` from
the trap record, so the field reads `trap <name> epc=0x<value> [tval=0x<value>]`. The verifier
applies it as **change note 01** and raises the baseline to **v2**; reason on record: Spike's
exception line carries no privilege, and inferring it from the previous record is unsound because
`mret`/`sret` change mode for the next instruction (measured: three such traps in
`rv64mi-p-illegal`).

Approved 2026-09-27 by the architect: the harness design in
`docs/verification/s1-harness-design.md`, together with the five additions it took from the
harness review (`docs/notes/audit-index.md` records where that note now lives). This authorizes the verifier to write
`tests/s1_differential.rs` and run it against the stub `src/main.rs`, where it must fail as
assertions on all 71 binaries. **That red run is the last start condition for M1.** The design
assumes the v2 trap record, so change note 01 lands before the harness runs.

Permission granted 2026-09-27 by the architect: the assistant may re-point the reference to the
harness review in `docs/verification/s1-harness-design.md` §10 at the audit index, since the note
it named was removed. Expires when this work order is accepted.

## Method approved — M1, 2026-09-27

The architect approved **M1 (skeleton: CLI, ELF loader, sparse 4 KiB page memory, halt protocol)**.
The implementer may write `src/**` for M1 and nothing further.

M1's acceptance is not a green harness: the differential step stays red until M5, and a red run
during M1 is the expected state, not a defect. M1 is done when:

- `cargo build --release` is clean and the unit suite is green;
- the CLI resolves its arguments, and a missing path or a non-ELF file exits 2 with one stderr
  line and nothing on stdout;
- all 71 test-set images load, with a unit test asserting each `PT_LOAD` segment lands where the
  ELF says and each `tohost` symbol is found (69 at `0x80001000`, two at `0x80002000`);
- memory unit tests cover page allocation, misaligned reads and writes, and unmapped access
  returning a fault rather than panicking.

Scope fence: no decoder, no CSR table, no instruction semantics, and no execution loop in M1.
Those are M2–M4, and each needs its own approval before it is written.

Method specification signed off 2026-09-28: `docs/work-orders/001-m1-skeleton.md` is the M1
specification, approved section by section. Its §9 obligation 6 — process-level CLI checks with no
oracle and no trace — is the verifier's, and can go green during M1 while the differential step
stays red.

## Evidence

- Implementer: files touched, commands run, failures seen, one line per method. May run the
  differential step locally (`cargo test --test s1_differential -- --ignored --test-threads=1`)
  as a self-check and record the first divergence they fixed; acceptance is still the verifier's
  run on the merged state.
- Verifier: `docs/verification/reports/001-s1.md` with the test set result and any first divergence.
