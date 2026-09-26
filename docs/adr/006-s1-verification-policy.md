# ADR 006 — S1 verification compares retirement events after a closed normalization list

**Status:** accepted · **Date:** 2026-09-26 · **Confidence:** high
Approved with `docs/verification/s1-freeze.md`.

## Problem

"The simulator must match Spike" has no single meaning. Spike's own log contains the boot ROM it
executes before the ELF entry, symbol banners, CSR-write annotations, and a halt path that spins in
the `write_tohost` loop about 1000 times after the store that ends the program. Byte-for-byte
equality is therefore impossible; unchecked normalization is worse, because a differ that silently
drops fields can hide a real bug while the suite stays green.

## Options

1. **Byte-identical output.** Impossible without reimplementing Spike's ROM, banners and halt-poll
   timing, and it would make our stdout meaningless on its own.
2. **Retirement equivalence after a closed normalization list (chosen).** The record sequence must
   match once a small, enumerated set of rewrites is applied to the reference log; the list is
   part of the freeze and cannot be widened without sign-off.
3. **Exit status plus sampled architectural state.** Cheapest and still catches most regressions,
   but it localizes nothing: a failure says "wrong" without saying "where".

## Decision

S1 acceptance is retirement equivalence after the normalization list in the freeze, plus exit-status
equality. Two sub-decisions the architect approved with it:

- **CSR writes stay in the compared record.** Spike's `c<num>_<name> 0x<value>` annotations are
  rewritten, not dropped, so WARL masking, `mret`'s implicit `mstatus` write and the U/S views of
  `mstatus` become ordinary diffs.
- **The simulator emits its own compact record form**, not Spike's text; the rewrite table is small
  and testable.

## Tradeoffs

- The differ becomes trusted code: a wrong normalizer can hide a bug. Mitigated by keeping the list
  closed, in the verifier's file, under the architect's sign-off.
- Spike's WARL details become our specification by default, including quirks such as MTIP reading as
  pending. That is accepted for S1 because the profile mandates the behaviour Spike implements.
- Trace equality remains necessary but not sufficient: state no test reads stays unverified until
  `riscv-arch-test` coverpoints are wired in.

## Consequences

- Work order 001 is frozen against the freeze document; M1 may start.
- Every later slice inherits this shape: pin the oracle, enumerate the normalization, then compare.
- Adding a normalization is a change to the freeze and needs the architect's approval.
