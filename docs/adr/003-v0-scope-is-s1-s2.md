# ADR 003 — v0 delivers slices S1–S2 only

**Status:** accepted · **Date:** 2026-09-24 · **Confidence:** high

## Problem

ADR 002 set RVA20S64 as the conformance target. Taken literally that is nine slices of work,
including F/D floating point and Sv39 virtual memory, before anything can be shown to work.
A target without a first deliverable produces no feedback and no evidence.

## Options

1. **Build toward the profile breadth-first** — every extension partially implemented. Nothing is
   verifiable until the end, and the trace differ reports noise rather than one first divergence.
2. **v0 = S1–S2 (chosen)** — RV64I with Zicsr/Zifencei, M/U modes and traps, then M. Trace-exact
   against Spike on `rv64ui-p-*` and `rv64um-p-*` before anything else is started.
3. **v0 = RV64I only** — smaller still, but S1 already needs trap and CSR machinery for
   `rv64mi-p-*`, so M is nearly free and unblocks a real compiler target sooner.

## Decision

v0 is slices S1–S2 as defined in `docs/notes/rva20s64-scope.md`. Later slices are not started
until the S1–S2 corpora are trace-exact green.

## Tradeoffs

- No floating point, no compressed instructions, no supervisor mode for several cycles; the
  "boots an OS" demo stays out of reach for now, which is the intended cost of a verifiable base.
- Committing to slice order means re-planning if an early slice exposes a bad assumption — cheap,
  because each slice ends with evidence rather than momentum.

## Consequences

- Every other slice is explicitly `proposed`, not `accepted`, until its own work order freezes.
- The verifier's first freeze proposal covers S1's observable surface only.
