# ADR 004 — F/D semantics come from a vendored proven softfloat, not hand-written IEEE 754

**Status:** accepted · **Date:** 2026-09-24 · **Confidence:** high on the build-vs-buy call,
low on which library (to be chosen at the S6 freeze)

## Problem

RVA20 includes F and D. The hard parts are not add and multiply; they are NaN boxing of f32
values in 64-bit registers, canonical NaN propagation, the five rounding modes, and the
`fflags` exception bits. Hand-written float code fails on exactly those corners, and the
failure appears as a rare divergence in a long corpus run.

## Options

1. **Hand-written IEEE 754 over Rust primitives.** No dependency, full control; silently loses
   RISC-V-specific NaN and flag semantics that Rust's `f64` does not model.
2. **Vendored proven softfloat (chosen).** Correct corner cases from day one; costs an external
   dependency vendored into the tree and a wrapping layer that maps its API onto the ISA.
3. **Rust `f64` plus a compatibility shim.** Cheapest to start, and the shim grows until it is
   the softfloat implementation, written by us, with the original defect profile intact.

## Decision

S6 uses a vendored, proven softfloat implementation rather than hand-written IEEE 754
arithmetic. The specific library is chosen at the S6 freeze, with the architect's approval.

## Tradeoffs

- Correctness arrives earlier, at the cost of an unusual build dependency and a conversion layer
  that is ours to test.
- Hand-written code would keep the tree dependency-free — a real benefit for a self-contained
  teaching project, and a real risk for anything claiming conformance.

## Consequences

- S6's work order must state the library, its version, and the vendoring method before the freeze.
- Host float behaviour never leaks into the trace; every result comes from the vendored model.
