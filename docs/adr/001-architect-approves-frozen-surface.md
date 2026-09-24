# ADR 001 — The verifier proposes the frozen observable surface; the architect approves it

**Status:** accepted · **Date:** 2026-09-24 · **Confidence:** high on the gate, medium on the batching

## Problem

The observable surface of the simulator — CLI, trace line format, public signatures — must be
frozen before the implementer writes code, or the implementer and verifier build against
different contracts and the divergence is found late. With the earlier verification-contract
document deleted, no approved source for that surface existed. Someone had to own it.

## Options

1. **Verifier freezes alone.** Fastest, no round trip. The verifier authors the oracle and the
   criteria, so a wrong oracle is never caught until the corpus disagrees — and the implementer
   is then blamed for a spec error.
2. **Verifier proposes, architect approves.** One human decision per freeze. Adds latency, but
   the specification stops being the adversary's private opinion.
3. **Implementer and verifier freeze together.** Symmetric, but turns a decision into a
   negotiation and puts two writers on one artifact.

## Decision

The verifier authors the frozen surface under `docs/verification/` and marks it *proposed*. The
architect approves it. Only then does the work order move to `frozen` and the implementer start.

## Tradeoffs

- Every interface freeze costs one architect decision; batching is expected, so freezes should
  cover a coherent module, not one method.
- The architect is now a dependency in the critical path. A slow approval stalls the implementer,
  which is the price of not verifying against an invented oracle.
- The verifier keeps the pen: the architect approves content, not wording.

## Consequences

- `docs/roles.md` §4 and §7 and `docs/cooperation.md` step 2 carry this rule.
- A freeze that is never approved stays `proposed`; the implementer may not start.
