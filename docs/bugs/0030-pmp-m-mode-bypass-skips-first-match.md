# Bug 0030: M-mode PMP bypass skips the lowest matching entry

| Field | Value |
|---|---|
| Found | 2026-10-30, code review of commit `f57da37` |
| Affected | `src/csr.rs:500-502` (`Csrs::pmp_allows`) |
| Status | Fixed |
| Fixed | 2026-10-30 |

## Summary

While scanning entries, M-mode accesses skip every unlocked entry before
checking its range:

```rust
if privilege == PrivilegeMode::Machine && config & PMP_CFG_LOCK == 0 {
    continue;
}
```

PMP first selects the lowest-numbered entry matching the access. If that entry
is unlocked and the access is from M-mode, the access succeeds; it must not
fall through to a later locked entry.

## Specification requirement

Privileged ISA, §3.7.1.3 (Priority and Matching Logic), local
`riscv-spec.html`, anchors `norm:pmp_entry_priority` and
`norm:pmp_rwx_check`:

> The lowest-numbered PMP entry that matches any byte of a memory operation
> determines whether that operation succeeds or fails.

> If a PMP entry matches all bytes of a memory operation, then the L, R, W, and
> X bits determine whether the operation succeeds or fails. If the L bit is
> clear and the privilege mode of the access is M, the operation succeeds.

Against the code: an unlocked matching entry 0 is ignored for M-mode, so a
later locked entry can deny the access.

## Reproduction

1. Entry 0: unlocked, matching, R/W/X = 0.
2. Entry 1: locked, matching, R/W/X = 0.
3. M-mode load/store over the overlap.

Expected: succeeds — entry 0 is the first match and is unlocked, so M-mode
bypasses its permissions.
Actual: `pmp_allows` skips entry 0 and returns false on entry 1.

## Impact

M-mode (including OpenSBI) can receive spurious access faults when a later
locked entry overlaps an earlier unlocked one. The priority order is violated.

## Why it escaped

Tests exercise only a single locked entry or no-match cases; overlapping
unlocked/locked entries are not covered.

## Suggested fix

Select the first matching entry first, then apply the M-mode rule:

```rust
let Some(entry) = first_match else { return privilege == Machine; };
if privilege == Machine && entry.unlocked {
    return true;
}
entry.permissions_allow(access)
```

Combine this with the whole-access matcher from bug 0029.

## Regression tests

- Unlocked matching entry 0 + locked matching entry 1 → M-mode succeeds.
- Locked matching entry 0 + permissive entry 1 → M-mode uses entry 0 and may
  fault.
- S/U modes still use the first match without the M bypass.
- No matching entry: M succeeds; S/U fails when PMP is implemented.

## Prevention

Separate “select first match” from “authorize selected entry” and test
overlapping entries in both orders.

## Fix

PMP matching now selects the lowest-numbered entry for the whole access before
considering privilege. M-mode bypass is applied only when that selected entry
is unlocked; a locked lower-priority entry can no longer override the selected
entry.

## Regression tests

- `csr::tests::pmp_m_mode_bypasses_only_after_selecting_the_first_match`

## Verification

`cargo test` and the full ACT4 gate pass.
