# Bug 0006: `execute()` retires instructions without advancing `mcycle`

| Field | Value |
|---|---|
| Found | 2026-10-22, code review of commit `3878163` |
| Fixed | 2026-10-22 |
| Affected | `src/machine.rs:203-218` (`execute_with_decoded`), `:155-158` (`execute`), `:134-138` (`step`) |

## Summary

`execute_with_decoded` is shared by `step` and the public `Machine::execute`
and increments `minstret` on every successful retirement:

```rust
let writes_instret =
    csr_write_target(decoded.instruction()) == Some(inst::CSR_MINSTRET);
if !writes_instret {
    self.cpu.csrs_mut().retire_instruction();
}
```

`mcycle` is advanced only in `step` (lines 134-138). Calling `execute`
directly therefore retires an instruction while `mcycle` stays at zero, so the
same instruction produces different counter state depending on the entry
point.

## Specification context

Zicntr (§4.3) defines `mcycle` as the count of clock cycles and `minstret` as
the count of instructions retired. The ISA does not prescribe this crate's
API, but the simulator models one `step` as one cycle; a retired instruction
that consumes no cycle is internally inconsistent. Whichever model is chosen
must apply to both entry points:

- advance `mcycle` and `minstret` consistently from a shared path, or
- document that direct `execute` excludes both counters (and then stop
  incrementing `minstret` there).

## Expected

After `execute(addi …)` from a fresh machine, `mcycle == 1` and
`minstret == 1`, matching one `step` of the same instruction.

## Actual

`minstret == 1`, `mcycle == 0`. An instruction that writes `mcycle` via a CSR
instruction also leaves the two paths unequal.

## Impact

Direct-API users, unit tests, and benchmarks observe `cycle`/`time`/`mcycle`
frozen while `instret`/`minstret` advance, so elapsed-cycles and IPC-style
measurements are wrong. Compliance tests use `step`, which hides the
inconsistency.

## Why it escaped

Counter tests drive the machine through `step`/`run`; tests that call
`execute` assert instruction semantics but never compare the counter state
against the `step` path.

## Fix

Advance the cycle inside the shared execution path: after `execute_inner`
returns (success or trap), call `advance_cycle` unless the decoded instruction
targets `mcycle`, mirroring `step`'s `writes_cycle` check. Keep the
fetch-failure cycle advance in `step`, since `execute` has no fetch phase.
`advance_cycle` already honours `mcountinhibit.CY`.

## Regression tests

- `execute_advances_mcycle_and_minstret`: fresh machine plus one `execute`
  yields `(mcycle, minstret) == (1, 1)`.
- Same instruction through `step` and `execute` produces identical counters.
- A CSRRW to `mcycle` does not double-advance the cycle in either path.
- A trapping `execute` advances `mcycle` but not `minstret`, matching `step`.

## Prevention

Keep counter updates in one shared retirement path and exercise both public
entry points in counter tests.

## Verification

`cargo test` and `cargo clippy --all-targets -- -D warnings` pass with new
`execute`/`step` parity, counter-write, and trapping-path regressions.
