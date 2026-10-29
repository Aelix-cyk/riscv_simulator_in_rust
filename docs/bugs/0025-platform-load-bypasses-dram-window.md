# Bug 0025: Platform image loads bypass the effective DRAM window

| Field | Value |
|---|---|
| Found | 2026-10-29, code review of commit `b5f1a1e` |
| Fixed | 2026-10-29 |
| Affected | `src/platform.rs:238-254` (`load_bytes`), bus checks at `:207` and `:226` |

## Summary

`Platform::load_bytes` validates against the backing `Memory` length:

```rust
if offset
    .checked_add(length)
    .is_none_or(|end| end > self.memory.len())
{
    return Err(Trap::AccessFault);
}
```

but `read_u8`/`write_u8` expose only the fixed `DRAM_SIZE` window. If
`Platform::new` receives a larger `Memory` (created with
`Memory::with_size`), a binary can load beyond the mapped DRAM window and then
fault on its first fetch or access.

## Expected

The loader and bus must agree on one effective DRAM region. A load outside
`[DRAM_BASE, DRAM_BASE + DRAM_SIZE)` fails immediately with `AccessFault` and
leaves memory unchanged.

## Actual

The load succeeds for any address below `memory.len()`, even when the bus would
reject the same address.

## Impact

Invalid images are silently accepted; the failure surfaces later as a
confusing fetch or load access fault, after program state has begun to change.

## Why it escaped

Tests construct `Memory::new()` (exactly 1 MiB) and only load inside the
window, so the two bounds coincide.

## Suggested fix

Use one effective DRAM bound in the loader and bus, e.g.
`DRAM_BASE + DRAM_SIZE`, with checked `u64` arithmetic. Alternatively, make the
bus expose the full supplied memory length — but pick one model and apply it
everywhere.

## Regression tests

- `Memory::with_size(2 * DRAM_SIZE)` plus a load at
  `DRAM_BASE + DRAM_SIZE` fails before mutating memory.
- Loads inside the window still succeed.
- A fetch/read at the first byte outside the window faults consistently with
  the loader's decision.

## Prevention

Centralize DRAM address checks in one helper used by `load_bytes`, reads,
writes, and fetch translation tests.

## Fix

`Platform::dram_offset` now enforces one effective DRAM size:
`min(DRAM_SIZE, backing_memory.len())`. Reads, writes, and image loads all use
it, and image loads preflight the complete span before mutating memory.

## Regression tests

- `platform_loader_and_bus_share_the_effective_dram_window`
