# Bug 0029: PMP checks one byte at a time instead of the whole access

| Field | Value |
|---|---|
| Found | 2026-10-30, code review of commit `f57da37` |
| Affected | `src/machine.rs:1765-1781` (`physical_addresses`), `:1748`, `:1928`, `:1960`; `src/mmu.rs:260` |
| Status | Fixed |
| Fixed | 2026-10-30 |

## Summary

`physical_addresses` translates and checks each byte independently:

```rust
for (offset, slot) in physical.iter_mut().enumerate() {
    ...
    *slot = self.translate_address(virtual_address, access)?;
    if !self.pmp_allows(slot.address, access) {
        return Err(self.access_exception(access, virtual_address));
    }
}
```

PMP does not select an entry per byte. The lowest-numbered entry that matches
**any** byte determines the access, and that entry must cover **all** bytes or
the access fails regardless of its permission bits. Checking bytes separately
lets an aligned 8-byte access spanning two adjacent read-enabled NA4 entries
succeed, even though the first matching entry does not cover the whole access.

The same one-address check is used for page-table reads (`src/mmu.rs:260`), so
an 8-byte PTE is checked only at its first byte. On a denied later byte, the
fault also reports that byte's address instead of the original effective
address.

## Specification requirement

Privileged ISA, §3.7.1.3 (Priority and Matching Logic), local
`riscv-spec.html`, anchors `norm:pmp_entry_priority` and
`norm:pmp_full_match_required`:

> PMP entries are statically prioritized. The lowest-numbered PMP entry that
> matches any byte of a memory operation determines whether that operation
> succeeds or fails. The matching PMP entry must match all bytes of a memory
> operation, or the operation fails, irrespective of the L, R, W, and X bits.

Against the code: each byte may select a different entry, and an access is
never rejected merely because the first matching entry covers only part of it.

## Reproduction

1. Configure entry 0 as an NA4, read-enabled region covering bytes 0-3.
2. Configure entry 1 as an adjacent NA4, read-enabled region covering bytes 4-7.
3. Execute an aligned 8-byte load over bytes 0-7.

Expected: access fault (entry 0 matches the access but does not cover all 8
bytes).
Actual: succeeds, because bytes 0-3 pass entry 0 and bytes 4-7 pass entry 1.

## Impact

PMP can be bypassed by splitting one access across multiple permissive entries.
Page-table entries spanning a PMP boundary are under-checked. Fault addresses
can point at an internal byte rather than the effective address.

## Why it escaped

Tests use single-entry ranges or accesses that fit entirely within one entry;
there is no adjacent-entry or split-boundary case.

## Suggested fix

Replace the one-address predicate with a start/width-aware check:

1. Find the lowest-numbered entry matching any byte in `[start, start+width)`.
2. If none matches, apply the no-match rule.
3. Require the selected entry's range to contain the whole access.
4. Check permissions (and M-mode rules) for the selected entry.

Reuse it for explicit loads/stores/fetch and for implicit 8-byte page-table
reads. Report the original effective address on failure.

## Regression tests

- The two-adjacent-NA4 8-byte load above faults.
- Two adjacent entries covered by one larger entry succeed when appropriate.
- A deny on a later byte reports the original effective address.
- An 8-byte PTE read spanning two entries uses whole-access matching.

## Prevention

Add boundary/overflow tests for PMP ranges and share one width-aware matcher
between explicit accesses and page-table walks.

## Fix

`Csrs::pmp_allows` now accepts a start address and width, selects the first
entry intersecting any byte, and requires that entry to cover the complete
access before applying permissions. `Machine` groups contiguous translated
addresses into physical runs and applies the same matcher to explicit accesses,
writes, cache-block preflight, and complete 8-byte PTE reads. Failures retain
the original effective address in `mtval`/`stval`.

## Regression tests

- `csr::tests::pmp_uses_the_first_matching_entry_for_the_whole_access`
- `machine::tests::pmp_whole_access_match_rejects_split_entry_bypass`
- `act4/priv/SvPMP` and `act4/priv/SvPMPZicbo`

## Verification

`cargo test` and the full ACT4 gate pass.
