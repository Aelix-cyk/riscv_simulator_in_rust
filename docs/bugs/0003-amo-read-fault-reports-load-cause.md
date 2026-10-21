# Bug 0003: AMO read faults report the Load access cause

| Field | Value |
|---|---|
| Found | 2026-10-21, code review of commit `2781866` |
| Affected | `src/machine.rs`, `execute_amo_w` (line 746) and `execute_amo_d` (line 770) |

## Summary

Both AMO helpers map a failed read phase to `CAUSE_LOAD_ACCESS`:

```rust
let old = self
    .bus
    .read_u64(address)
    .map_err(|_| Exception::new(inst::CAUSE_LOAD_ACCESS, address))?;
```

AMOs are store/AMO operations. Per the privileged spec, an AMO raises a
store/AMO access fault whether the read or the write phase fails; spike reports
cause `0x7` for an out-of-range `amoadd.d`.

## Specification requirement

Privileged ISA, §3.1.15 (`mcause`), local `riscv-spec.html`, anchor
`norm:mcause_exccode_ld_ldrsv`:

> Note that load and load-reserved instructions generate load exceptions,
> whereas store, store-conditional, and AMO instructions generate store/AMO
> exceptions.

The `mcause` table defines the codes this sentence refers to: code `0x5` is
“Load access fault”; code `0x7` is “Store/AMO access fault”. The page-fault
analogue in the same volume states:

> AMOs never raise load page-fault exceptions. Since any unreadable page is
> also unwritable, attempting to perform an AMO on an unreadable page always
> raises a store page-fault exception.

Against the code: `src/machine.rs:746` (`execute_amo_w`) and `src/machine.rs:770`
(`execute_amo_d`) map the read-phase bus error to `CAUSE_LOAD_ACCESS`, even
though the trapping instruction is an AMO. The write phases (lines 750 and
774) correctly use `CAUSE_STORE_ACCESS`; the read phases must match them. The
LR paths at lines 385 and 395 correctly stay on `CAUSE_LOAD_ACCESS`.

## Expected

An aligned `amoadd.w`/`amoadd.d` to an inaccessible address raises
`Exception(CAUSE_STORE_ACCESS = 0x7, tval = address)`.

## Actual

The read fault is reported as `Exception(CAUSE_LOAD_ACCESS = 0x5, tval =
address)`.

## Impact

Trap handlers observe the wrong `mcause`, which breaks spec conformance and
any handler that distinguishes load from store faults. The mismatch is visible
in spike differential testing.

## Why it escaped

Tests cover alignment faults and successful AMOs, but no AMO whose read phase
falls outside memory, so the cause constant in that branch is never asserted.

## Suggested fix

Map both read-phase bus errors in `execute_amo_w` and `execute_amo_d` to
`CAUSE_STORE_ACCESS`. Leave `lr.w`/`lr.d` as `CAUSE_LOAD_ACCESS`, since LR is a
load. Consider one shared helper that derives the cause from the atomic
operation rather than repeating it per phase.

## Regression tests

- `amoadd_w_read_fault_reports_store_access`
- `amoadd_d_read_fault_reports_store_access`

Assert cause `0x7` and the faulting address, and cross-check against `$SPIKE`.

## Prevention

Encode the spec's fault-cause table as tests over every atomic operation and
both memory phases; add faulting atomics to the spike differential suite.
