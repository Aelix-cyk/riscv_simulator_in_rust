# Bug 0018: CBO management uses store permission checks instead of load-or-store, A-only

| Field | Value |
|---|---|
| Found | 2026-10-28, code review of commit `0909b57` |
| Fixed | 2026-10-28 |
| Affected | `src/machine.rs:1600-1610` (`execute_cache_block_management`) |

## Summary

`execute_cache_block_management` translates the whole block as a store:

```rust
self.bus_check_write(block, CACHE_BLOCK_BYTES, Access::Store, address)
```

Under Sv39 this rejects readable but non-writable pages and writable pages
whose dirty bit is clear. Zicbom permits CBO.CLEAN/CBO.FLUSH/CBO.INVAL when
either load or store access is permitted, checks the accessed bit, and does
not check the dirty bit.

## Specification requirement

Unprivileged ISA, Zicbom, §4.20.3.2 (Traps), local `riscv-spec.html`,
anchor `norm:cbm_access`:

> A cache-block management instruction is permitted to access the specified
> cache block whenever a load instruction or store instruction is permitted to
> access the corresponding physical addresses. … During address translation,
> the instruction also checks the accessed bit and may either raise an
> exception or set the bit as required.

and:

> As implied by omission, a cache-block management instruction does not check
> the dirty bit and neither raises an exception nor sets the bit.

By contrast, the same section requires CBO.ZERO to check store permission and
both the accessed and dirty bits.

## Reproduction

With Sv39:

1. Map a page `R=1, W=0, A=1`; execute `cbo.flush` in an allowed mode.
   Expected: succeeds. Actual: store page fault.
2. Map a page `W=1, A=1, D=0`; execute `cbo.clean`.
   Expected: succeeds without setting D. Actual: store page fault (or D
   update).

## Impact

Valid management operations fault on read-only or clean pages, breaking cache
maintenance sequences (especially in S-mode) and diverging from Spike.

## Suggested fix

Introduce a dedicated translation access type for cache-block management that:

- accepts a leaf PTE with `R || W` (subject to U/SUM/privilege rules);
- checks/sets `A` only;
- raises a store page fault when translation denies access and a store access
  fault otherwise.

Keep `Access::Store` for CBO.ZERO so it continues to require write permission
and `A && D`.

## Regression tests

- CBO.FLUSH/CLEAN/INVAL on `R=1, W=0, A=1` succeed.
- Same on `W=1, A=1, D=0` succeed and leave D clear.
- U-mode without permission raises a store page fault.
- CBO.ZERO still faults on `R=1, W=0` and on `W=1, A=1, D=0`.

## Prevention

Model each memory operation with an explicit access class and add a
permission-matrix test over `{load, store, CBO management, CBO zero}`.

## Fix

Translation now has a dedicated `Access::CacheManagement` class. It requires an
Accessed leaf PTE with read or write permission and does not require Dirty.
Translation and bus failures still raise store page-fault or store access-fault
causes as required. `CBO.ZERO` continues to use `Access::Store`, so it requires
write permission and both Accessed and Dirty.

## Regression tests

- `cache_management_accepts_read_or_write_access_without_dirty`
- Existing cache-block and ACT4 Zicbom/Zicboz tests continue to cover
  management operations and the stricter `CBO.ZERO` permission path.

## Verification

- `cargo test`
- `cargo clippy --all-targets -- -D warnings`
- Legacy `riscv-tests` compliance gate
- ACT4 Sv/Svade/Svbare/Ssccptr gate
