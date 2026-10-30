# Bug 0031: Locked TOR entry leaves its lower-bound `pmpaddr` writable

| Field | Value |
|---|---|
| Found | 2026-10-30, code review of commit `f57da37` |
| Affected | `src/csr.rs:374-377` (`pmpaddr` write path) |
| Status | Fixed |
| Fixed | 2026-10-30 |

## Summary

Writes to `pmpaddr[i]` are suppressed only when entry `i` itself is locked:

```rust
if !self.pmp_entry_locked(index) {
    self.pmpaddr[index] = value & PMP_ADDRESS_MASK;
}
```

For a locked TOR entry `i`, `pmpaddr[i-1]` supplies its lower bound, so writes
to that register must also be ignored. Currently entry 1 can be locked as TOR
while M-mode rewrites `pmpaddr0`, moving or expanding the supposedly locked
region.

## Specification requirement

Privileged ISA, §3.7.1.2 (Locking and Privilege Mode), local
`riscv-spec.html`, anchor `norm:pmp_l_bit_write_protection`:

> If PMP entry i is locked, writes to pmpicfg and pmpaddri are ignored.
> Additionally, if PMP entry i is locked and pmpicfg.A is set to TOR, writes to
> pmpaddri-1 are ignored.

Against the code: only the same-index lock is considered.

## Reproduction

1. Configure entry 1 as TOR with lower bound `pmpaddr0` and upper bound
   `pmpaddr1`, then lock entry 1.
2. Write a new `pmpaddr0` from M-mode.
3. Read `pmpaddr0` and re-test access.

Expected: `pmpaddr0` unchanged; the locked region still matches the original
range.
Actual: the new value is accepted and the region moves/expands.

## Impact

Locked PMP regions can be resized or relocated after lock, defeating the
isolation guarantee OpenSBI and M-mode software rely on.

## Why it escaped

Tests lock and then attempt to rewrite the same index; no test rewrites the
preceding address register of a locked TOR entry.

## Suggested fix

Ignore a `pmpaddr` write when either:

- the same-index entry is locked; or
- the next entry exists, is locked, and has `A = TOR`.

Keep the write mask for all accepted writes, and use named constants for the
TOR/lock fields.

## Regression tests

- Locked TOR entry 1 protects both `pmpaddr1` and `pmpaddr0`.
- An unlocked TOR entry 1 still allows `pmpaddr0` writes.
- A locked NAPOT/NA4 entry 1 does not over-protect `pmpaddr0`.
- Re-test access ranges after attempted writes.

## Prevention

Test lock side effects for every addressing mode (OFF/TOR/NA4/NAPOT) and both
the same-index and TOR-lower-bound registers.

## Fix

`pmpaddr[i]` writes now use `pmp_address_writable`, which rejects writes when
entry `i` is locked or when entry `i + 1` is a locked TOR entry. This protects
the lower bound without affecting locked NA4/NAPOT entries.

## Regression tests

- `csr::tests::locked_tor_entry_protects_its_lower_bound_address`

## Verification

`cargo test` and the full ACT4 gate pass.
