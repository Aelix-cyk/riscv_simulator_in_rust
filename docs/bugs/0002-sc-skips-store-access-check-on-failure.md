# Bug 0002: SC skips the store-access check when it fails

| Field | Value |
|---|---|
| Found | 2026-10-21, code review of commit `2781866` |
| Affected | `src/machine.rs`, `ScW`/`ScD` arms (lines 399-426) |

## Summary

Both SC arms validate alignment, take the reservation, and branch on whether it
matched. Only the success branch touches the bus:

```rust
let success = self.reservation.take() == Some(Self::reservation_set(address));
if success {
    self.bus
        .write_u32(address, source as u32)
        .map_err(|_| Exception::new(inst::CAUSE_STORE_ACCESS, address))?;
    self.write(rd, 0);
} else {
    self.write(rd, 1);
}
```

RISC-V requires the store/AMO access check to happen before SC retires,
regardless of whether the reservation is valid. A failed SC to inaccessible
memory must fault, not report failure.

## Specification requirement

Unprivileged ISA, §5.2 (`Zalrsc`), local `riscv-spec.html`, anchor
`norm:sc_retire_permission`:

> No SC.W instruction shall retire unless it passes memory permission checks,
> but it is UNSPECIFIED whether any side effects of implicit address
> translation and protection memory accesses (such as setting a page-table
> entry D bit) occur on a failed SC.W.

and anchor `norm:sc_failed_as_store`:

> For the purposes of memory protection, a failed SC.W may be treated like a
> store.

Privileged ISA, §3.1.15 (`mcause`), anchor
`norm:mcause_exccode_ld_ldrsv`:

> load and load-reserved instructions generate load exceptions, whereas store,
> store-conditional, and AMO instructions generate store/AMO exceptions.

Against the code: `src/machine.rs:399-412` (`ScW`) and `413-426` (`ScD`) only
call `write_u32`/`write_u64`—the only store-permission check—inside
`if success`. The `else` branch immediately retires with `rd = 1`, so the
instruction passes no memory permission check at all, contradicting
`norm:sc_retire_permission`.

## Expected

An aligned `sc.w`/`sc.d` with no matching reservation to an inaccessible
address (e.g. `DRAM_BASE - 8`) raises
`Exception(CAUSE_STORE_ACCESS = 0x7, tval = address)`; `rd` and the PC are
unchanged.

## Actual

The instruction succeeds, writes `1` to `rd`, and advances the PC without
touching memory.

## Impact

Software that probes address accessibility with SC, or that relies on store
faults from SC, observes the wrong behaviour; the simulator disagrees with
spike. A failed SC becomes indistinguishable from an SC that should have
trapped.

## Why it escaped

Tests cover a failed SC against valid memory and a successful SC, but never a
failed SC to an inaccessible address, so the missing check is unobservable in
the suite.

## Suggested fix

Check store accessibility before retiring, without mutating memory. Add a
non-mutating bus probe (e.g. `check_write(address, width) -> Result<(), Trap>`)
and call it in both SC arms before branching on `success`, mapping failure to
`CAUSE_STORE_ACCESS` with the address as `tval`. The actual store stays
conditional on the reservation.

## Regression tests

- `sc_w_failure_to_unmapped_address_traps`
- `sc_d_failure_to_unmapped_address_traps`

Each asserts the cause (`0x7`), `tval`, untouched `rd`, and an unmoved PC, and
should be cross-checked against `$SPIKE`.

## Prevention

Audit every A-extension path against the spec's fault-cause table, and add
faulting-atomic cases to the spike differential suite.
