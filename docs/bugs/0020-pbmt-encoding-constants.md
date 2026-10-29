# Bug 0020: PBMT encodings are raw magic numbers

| Field | Value |
|---|---|
| Found | 2026-10-29, code review of commit `280652c` |
| Affected | `src/mmu.rs:251` and `:272-276` |
| Status | Fixed |

## Summary

The Svpbmt validity check and memory-type mapping use raw architectural
encodings:

```rust
if pbmt == 3 || (pbmt != 0 && !config.pbmt_enabled) {
    return Err(fault(access, FaultKind::Page));
}
...
memory_type: match pbmt {
    0 => MemoryType::Cacheable,
    1 => MemoryType::NonCacheable,
    2 => MemoryType::Io,
    _ => unreachable!("reserved PBMT value was rejected"),
},
```

The `0/1/2/3` values are the PBMT field encodings, not incidental integers.
AGENTS.md's Coding rule says: "Do not use magic numbers in the code."

## Specification requirement

Privileged/ISA manual, §7.2 (`Svpbmt`), local `riscv-spec.html`, Table 150:

| PBMT value | Meaning |
|---|---|
| 0 | PMA (none / use physical memory attributes) |
| 1 | NC (non-cacheable, idempotent, weakly ordered main memory) |
| 2 | IO (non-cacheable, non-idempotent, strongly ordered I/O) |
| 3 | Reserved for future standard use |

Against the code: those four encodings are duplicated as literals in the
validity check and the mapping, so a future encoding change or extension can
leave the two sites inconsistent.

## Impact

No behavioural bug today; this is a maintainability violation and a risk of
divergence. The `_` arm relies on a previously checked literal `3`, so changing
one site without the other would turn a reserved encoding into an
`unreachable!` panic or silently accept it.

## Suggested fix

Define named constants (or a small `Pbmt` enum with `from_bits`) and use them in
both places:

```rust
const PBMT_PMA: u64 = 0;
const PBMT_NC: u64 = 1;
const PBMT_IO: u64 = 2;
const PBMT_RESERVED: u64 = 3;
```

Then write `pbmt == PBMT_RESERVED` and match on the named constants. Keep
`PTE_PBMT_SHIFT` and `PTE_PBMT_MASK` as the single source for field extraction.

## Regression tests

- All four encodings: PMA → Cacheable, NC → NonCacheable, IO → Io,
  reserved (3) → page fault.
- With `pbmt_enabled = false`, any nonzero encoding still faults.
- Non-leaf PTE with nonzero PBMT still faults.

## Prevention

Encode architectural field values as constants or enums at the point they are
decoded; add a PBMT matrix test so the mapping and validity check cannot drift.

## Resolution

`PBMT_PMA`, `PBMT_NC`, `PBMT_IO`, and `PBMT_RESERVED` now name all PBMT field
encodings. `pbmt_memory_type` is the single decoding path for both validity and
the translated memory type, removing the duplicated literals and the dependent
`unreachable!` arm.

`pbmt_encodings_map_to_memory_types_and_reserve_value_three` covers the complete
encoding matrix. The existing Svpbmt tests continue to cover disabled PBMTE,
reserved leaf encodings, and nonzero encodings on non-leaf PTEs.
