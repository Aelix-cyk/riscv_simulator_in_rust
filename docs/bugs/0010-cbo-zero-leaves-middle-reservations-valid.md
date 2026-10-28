# Bug 0010: CBO.ZERO leaves reservations in the middle of the block valid

| Field | Value |
|---|---|
| Found | 2026-10-28, code review of commit `c45e658` |
| Fixed | 2026-10-28 |
| Affected | `src/machine.rs:1540` (CBO.ZERO), helper at `src/machine.rs:1149-1156` |

## Summary

`invalidate_reservation` only compares the first and last 8-byte reservation
sets covered by a write:

```rust
let first = Self::reservation_set(address);
let last = Self::reservation_set(address.saturating_add(width - 1));
if self.reservation == Some(first) || self.reservation == Some(last) {
    self.reservation = None;
}
```

For an ordinary access (width ≤ 8) this is sufficient. CBO.ZERO calls it with
`CACHE_BLOCK_BYTES` (64), so reservations on the six middle doublewords are
never compared and survive the full-block store:

```rust
self.invalidate_reservation(block, CACHE_BLOCK_BYTES);
```

## Specification requirement

Unprivileged ISA, §4.20.8.4 (CBO.ZERO), local `riscv-spec.html`:

> A CBO.ZERO instruction performs stores of zeros to the full set of bytes
> corresponding to the cache block that contains the address specified in rs1.

and §5.4 (`Za64rs`):

> The Za64rs extension requires that the reservation sets used by the
> instructions in the Zalrsc extension be contiguous, naturally aligned, and at
> most 64 bytes in size.

A 64-byte CBO.ZERO covers every possible reservation set in the block. Leaving
a reservation valid after the block was written contradicts both the
instruction's store behaviour and the helper's contract ("Clears a reservation
touched by a successful ordinary write"), which every ordinary store relies on.

## Reproduction

1. `lr.d rd, 24(a0)` reserves the doubleword at `block + 24` (a middle set).
2. `cbo.zero (a0)` zeros the aligned 64-byte block.
3. `sc.d rd2, ...` to `block + 24` succeeds instead of failing.

Expected: `rd2 == 1` (SC fails). Actual: `rd2 == 0`.

## Impact

LR/SC sequences can succeed after CBO.ZERO has overwritten the reserved
location, so the simulator reports the wrong value for synchronization code
that mixes cache-block zeroing with atomics. Behaviour differs between the
first/last and middle doublewords of the same block.

## Why it escaped

Atomic tests cover ordinary loads/stores invalidating reservations, but no test
performs CBO.ZERO between an LR and SC; CBO.ZERO tests only check zeroing and
permissions.

## Suggested fix

Make invalidation range-based: reserve between the first and last covered set
inclusive, not just the two endpoints:

```rust
let first = Self::reservation_set(address);
let last = Self::reservation_set(address.saturating_add(width - 1));
if self.reservation.is_some_and(|set| set >= first && set <= last) {
    self.reservation = None;
}
```

Keep the overflow-safe `saturating_add` and cover the full 64-byte block.

## Regression tests

- LR.D → CBO.ZERO → SC.D for every one of the eight doubleword offsets in a
  block; each SC must fail.
- A reservation outside the zeroed block must survive.
- Ordinary byte/halfword/word/doubleword stores keep their current
  invalidation behaviour.

## Prevention

Test every width passed to `invalidate_reservation` (1 through 64 bytes) with
reservations in the first, middle, and last sets.

## Fix

`invalidate_reservation` now clears the reservation when its aligned set lies
anywhere between the first and last sets covered by a write, rather than
comparing only the two endpoint sets.

## Regression tests

- `cache_block_zero_invalidates_every_reservation_set_in_the_block` walks
  LR.D → CBO.ZERO → SC.D across all eight doublewords.
- `cache_block_zero_preserves_reservations_outside_the_block` verifies that an
  adjacent reservation remains valid.
- Existing ordinary store and atomic invalidation tests continue to pass.

## Verification

- `cargo test`
- `cargo clippy --all-targets -- -D warnings`
- `cargo test --test compliance riscv_tests_gate -- --ignored --nocapture`
- `ACT4_EXTENSIONS=Zicbom,Zicbop,Zicboz,Zihintpause cargo test --test compliance act4_gate -- --ignored --nocapture`
