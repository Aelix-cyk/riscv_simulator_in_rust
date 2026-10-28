# Bug 0017: Unsupported `satp.MODE` writes clear translation state

| Field | Value |
|---|---|
| Found | 2026-10-28, code review of commit `0909b57` |
| Fixed | 2026-10-28 |
| Affected | `src/csr.rs:376-381` (`satp_value`), assignment at `:324`, test at `:910` |

## Summary

`satp_value` returns zero for any mode other than Bare or Sv39:

```rust
let mode = value & SATP_MODE_MASK;
if mode != 0 && mode != SATP_SV39_MODE << SATP_MODE_SHIFT {
    return 0;
}
```

and `write_with_privilege` assigns that result unconditionally:

```rust
inst::CSR_SATP => self.satp = Self::satp_value(value),
```

A write with an unsupported MODE (for example Sv48 while Sv39 is active)
therefore clears ASID/PPN and silently disables translation instead of leaving
`satp` untouched.

## Specification requirement

Privileged ISA, §4.1.11 (`satp`), local `riscv-spec.html`, anchor
`norm:satp_mode_op_unsupported`:

> Implementations are not required to support all MODE settings, and if satp is
> written with an unsupported MODE, the entire write has no effect; no fields in
> satp are modified.

Against the code: unsupported modes modify every field by replacing `satp`
with zero.

## Reproduction

1. Enter Sv39 with a nonzero ASID and PPN.
2. Write `satp` with MODE = Sv48 (unsupported), keeping other fields distinct.
3. Read `satp`.

Expected: the previous Sv39 value is unchanged.
Actual: `satp == 0` (Bare, ASID/PPN lost).

The unit test at `src/csr.rs:910` currently asserts the wrong `0` result.

## Impact

Software probing supported translation modes (a normal discovery technique)
accidentally disables translation, corrupting subsequent instruction fetches,
loads, and stores.

## Suggested fix

Make `satp_value` return `Option<u64>` (or `None` for unsupported MODE) and
leave `self.satp` unchanged when it returns `None`. Keep writable-field
masking for supported modes. Update the assertion to expect retention of the
previous value.

## Regression tests

- Sv39 → write Sv48 → `satp` unchanged (MODE, ASID, PPN all preserved).
- Sv48/Sv57/other reserved modes are ignored one by one.
- Bare and Sv39 writes still update the implemented fields.

## Prevention

Test every MODE value from the 4-bit field and assert the “write has no
effect” rule for unsupported ones.

## Fix

`satp_value` now returns `None` for unsupported MODE values, and the CSR write
path leaves `satp` unchanged in that case. Supported Bare and Sv39 writes still
apply the architectural writable-field mask.

## Regression tests

- `satp_retains_sv39_mode_asid_and_ppn` sweeps every nonzero four-bit MODE
  other than Sv39 and verifies that each write preserves the complete previous
  `satp` value.

## Verification

- `cargo test`
- `cargo clippy --all-targets -- -D warnings`
- Legacy `riscv-tests` compliance gate
- ACT4 Sv/Svade/Svbare/Ssccptr gate
