# Bug 0019: CBO translation faults report the block base instead of `rs1`

| Field | Value |
|---|---|
| Found | 2026-10-28, code review of commit `0909b57` |
| Fixed | 2026-10-28 |
| Affected | `src/machine.rs:1809-1824` (`bus_check_write`) |

## Summary

`bus_check_write` walks byte by byte:

```rust
let physical = self.translate_address(virtual_address, access)?;
self.bus
    .check_write(physical, 1)
    .map_err(|_| self.access_exception(access, fault_address))?;
```

Only `Bus::check_write` failures are remapped to `fault_address`. A
`translate_address` failure propagates with `virtual_address`, which is an
individual block byte (the aligned block base for the first iteration, or a
later byte), not the original `rs1` value. CBO instructions must report the
untranslated effective address in `stval`/`mtval`.

## Specification requirement

Unprivileged ISA, Zicbom, §4.20.3.2 (Traps), local `riscv-spec.html`:

> When a page-fault, guest-page-fault, or access-fault exception is taken, the
> relevant *tval CSR is written with the faulting effective address (i.e. the
> value of rs1).

The per-instruction descriptions for CBO.CLEAN/FLUSH/INVAL repeat that the
faulting virtual address is the value in `rs1`, not the block base.

## Reproduction

1. Use `rs1 = page_end - 16` so the aligned 64-byte block spans two pages.
2. Map the first page but leave the second unmapped.
3. Execute `cbo.flush (rs1)`.

Expected: fault `tval == rs1`.
Actual: `tval` is `rs1 & !63` (or a later byte), because the translation
failure escapes before `fault_address` is applied.

The overflow path at line 1817 also uses `address` rather than
`fault_address`.

## Impact

Trap handlers and compliance tests see the wrong faulting address for
unaligned CBO operations and for blocks split across mappings. CBO.ZERO has
the same issue for its preflight translation.

## Suggested fix

Attach `fault_address` to every CBO error path:

- map translation exceptions to an exception with the same cause and
  `tval = fault_address`;
- use `fault_address` for the overflow case;
- keep the existing `check_write` remap.

Alternatively, pass the effective address down to a CBO-specific translation
helper so it is used for every byte's faults.

## Regression tests

- Aligned and unaligned `rs1` CBO faults report exactly `rs1`.
- A block spanning an unmapped second page reports `rs1`, not the page byte.
- Both M-mode (`mtval`) and delegated S-mode (`stval`) paths are covered.
- CBO.ZERO uses the same address on permission and translation faults.

## Prevention

Centralize CBO fault construction and test split-mapping and unaligned cases.

## Fix

`bus_check_write` now remaps translation exceptions, address-overflow faults,
and bus-check failures to the original effective address supplied by the CBO
instruction. Multi-byte preflight translation therefore reports `rs1` in
`mtval`/`stval` regardless of which byte discovers the fault.

## Regression tests

- `cache_block_translation_faults_report_the_original_rs1_address` covers all
  four CBO instructions through an M-mode trap and a delegated S-mode trap with
  an unaligned `rs1`.

## Verification

- `cargo test`
- `cargo clippy --all-targets -- -D warnings`
- Legacy `riscv-tests` compliance gate
- ACT4 Sv/Svade/Svbare/Ssccptr gate
