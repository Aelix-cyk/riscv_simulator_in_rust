# Bug 0023: `mip`/`sip` read-modify-write latches device SEIP

| Field | Value |
|---|---|
| Found | 2026-10-29, code review of commit `b5f1a1e` |
| Fixed | 2026-10-29 |
| Affected | `src/csr.rs:238` and `:248` (`read_with_privilege`), `src/machine.rs:1002-1021` (CSRRS/CSRRC) |

## Summary

Ordinary reads of `mip`/`sip` return the OR of software-pending bits and the
external controller signal (`pending_mip()`). The CSR read-modify-write
instructions reuse that value as the write base:

```rust
let old = self.require_csr(csr, word)?;
if writes_csr_from_register(rs1) {
    self.write_csr(csr, old | source, word)?;
}
```

When a PLIC source asserts SEIP, an unrelated `csrrs`/`csrrc` on `mip` or
`sip` therefore writes the controller bit into software-writable
`mip.SEIP`. After the PLIC source is claimed and deasserted, the latched
software bit keeps the interrupt pending indefinitely.

## Specification requirement

Privileged ISA, §3.1.9 (`mip`), local `riscv-spec.html`, anchor
`norm:mip_seip_acc`:

> The value of the SEIP bit returned in the rd destination register is the
> logical-OR of the software-writable bit and the interrupt signal from the
> interrupt controller, but the signal from the interrupt controller is not
> used to calculate the value written to SEIP. Only the software-writable SEIP
> bit participates in the read-modify-write sequence of a CSRRS or CSRRC
> instruction.

Against the code: the RMW base comes from `pending_mip()`, which includes the
controller signal, so E is written back as B.

## Reproduction

1. Enable and assert the PLIC UART source (SEIP pending).
2. Execute `csrrs t0, mip, t1` with any nonzero `t1`.
3. Claim and complete the PLIC source.
4. Read `mip.SEIP`.

Expected: SEIP clears after completion.
Actual: SEIP remains set from the latched bit; the supervisor external
interrupt never deasserts.

The same applies to `sip` and to CSRRC.

## Impact

Spurious persistent supervisor external interrupts, duplicate handling, and
inability to clear interrupts without an extra explicit software write.

## Why it escaped

Tests check `mip` reads while device bits are pending, but never issue a CSR
read-modify-write while the external signal is asserted.

## Suggested fix

Distinguish the value written to `rd` from the raw RMW base:

- `rd` receives `pending_mip()` (OR of software and device);
- CSRRS/CSRRC compute the new value from the raw software-writable register
  (`self.mip`), masking per view (`sip` uses `mideleg`).

Add a `read_for_rmw`/`csr_write_base` helper so ordinary reads remain ORed.

## Regression tests

- PLIC pending: `csrrs`/`csrrc` on `mip` and `sip` do not set SEIP.
- `rd` still observes the ORed SEIP.
- After claim/complete, SEIP reads zero.
- Software-set SEIP remains writable and persists independently of the device
  signal.

## Prevention

Model external pending bits separately from software bits and test RMW
instructions with an external signal asserted.

## Fix

CSRRS and CSRRC now read the externally visible value for `rd`, but compute
their write base from the software-writable bits. `mip` uses `self.mip`;
`sip` uses `self.mip & mideleg`. The same helper is used by register and
immediate read-modify-write instructions.

## Regression tests

- `csr_rmw_base_excludes_device_pending_bits`
- `csr_read_modify_write_does_not_latch_device_interrupt_bits`
