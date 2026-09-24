# ADR 005 — Retarget to RVA22S64 (supersedes ADR 002)

**Status:** accepted · **Date:** 2026-09-24 · **Supersedes:** ADR 002 · **Confidence:** high

## Problem

ADR 002 chose RVA20S64 as the conformance target. After fetching `riscv-arch-test`, the
certification coverage matrix covers **RVI20, MC100, RVA22S64, RVB23S64, RVA23S64** — RVA20 is
absent, because RVA22 superseded it. A target with no test plan cannot be certified; the claim
would rest on our own reading of a specification document that is not even on this machine.

## Options

1. **Keep RVA20S64, fetch the profiles spec, write our own checklist.** Faithful to the original
   decision, and produces a conformance claim backed by nothing but our own interpretation.
2. **Retarget to RVA22S64 (chosen).** A ratified, superset-of-RVA20 profile with coverpoints,
   test plans, and Spike configs already on disk, and the profile Linux-class systems target.
3. **Retarget to RVA23S64.** Even better evidence, but mandatory vector and hypervisor extensions
   multiply scope far beyond "boot an OS".

## Decision

The conformance target is **RVA22S64**, single hart, little-endian. ADR 002's profile choice is
superseded; its reasoning about *why a named profile* remains valid and is not restated here.

## Tradeoffs

- RVA22S64 is not free on top of RVA20S64: it adds Zba/Zbb/Zbs, Zicbom/Zicboz/Zicbop, Zicclsm
  (native misaligned access to main memory), Za64rs and Zic64b constraints, and Zfhmin.
- Zfhmin is the quiet one — half-precision conversion and move instructions land in the same
  cycle as F, so the S6 work order grows.
- Five of the mandated behaviours (`Za64rs`, `Zic64b`, `Sstvecd`, `Sstvala`, `Ssu64xl`) have no
  Spike ISA-string switch; they are behaviours, not encodings, so plain trace diffing will not
  catch a violation. They need explicit tests.

## Consequences

- Slice plan and verified corpora: `docs/notes/rva22s64-scope.md` (renamed from the RVA20 note).
- The pinned Spike invocation is recorded there and was executed successfully on 2026-09-24.
- ADR 003's v0 scope (slices S1–S2) is unchanged; only the slice labels after S2 shift.
