# ADR 002 — Conformance target is RVA20S64

**Status:** accepted · **Date:** 2026-09-24 · **Confidence:** high on RVA20S64 as the target,
medium on the exact mandatory extension list (see Consequences)

## Problem

The simulator must eventually boot an OS, which requires supervisor mode and virtual memory.
An ad-hoc "RV64I, user mode only" scope would have to be re-opened for every system feature. A
named profile fixes the surface once and makes conformance a checkable claim instead of an
opinion.

## Options

1. **RV64I-only, user mode.** Smallest verifiable surface, fastest first trace diff; cannot boot
   an OS, and each later addition re-litigates scope.
2. **RVA20U64.** Adds M/A/F/D/C and counters, stays user-mode only; still cannot boot an OS.
3. **RVA20S64 (chosen).** Everything in RVA20 plus supervisor mode and Sv39 virtual memory.
   This is the minimum profile family Linux targets, so "boots an OS" becomes a reachable goal
   rather than a rewrite.

## Decision

The simulator targets the **RVA20S64** application-processor profile, single hart, little-endian.
Delivery is sliced: profile conformance is the end state, not v0.

## Tradeoffs

- The surface grows from tens of instructions to the full RVA20 set plus trap, CSR, and page-table
  semantics — the hardest parts to specify and to verify.
- F/D force a decision between hand-written IEEE 754 semantics and a proven softfloat library;
  the rare cases (NaN boxing, flags, rounding modes) are where hand-rolled implementations fail.
- S-mode makes the differ harder: Spike's M-mode boot path, PMP, and counter behaviour all become
  observable, so the "same binary, same options" rule needs `--priv=msu` and an explicit ISA string.

## Consequences

- Slice plan, corpora, and open questions: `docs/notes/rva20s64-scope.md`.
- The ISA string used against Spike must be pinned, not defaulted.
- Claiming *conformance* needs `riscv-arch-test`, which is not present locally; without it the
  strongest honest claim is trace equivalence with Spike over the available corpora.
