# Bug 0005: Zero-valued CSRRS/CSRRC sources bypass the read-only CSR check

| Field | Value |
|---|---|
| Found | 2026-10-22, code review of commit `3878163` |
| Fixed | 2026-10-22 |
| Affected | `src/machine.rs:626-641` (`Csrrs`/`Csrrc`), check at `src/csr.rs:122-124` |

## Summary

`execute_inner` decides whether CSRRS/CSRRC writes by inspecting the source
register's **value**:

```rust
let source = self.gpr(rs1);
if source != 0 {
    self.write_csr(csr, old | source, word)?;
}
```

Zicsr defines the no-write form by the register **index**: only `rs1 == x0`
suppresses the write. A register other than `x0` that happens to hold zero is
still an explicit CSR write, so it must perform the full-write side effects and
must raise an illegal-instruction exception on a read-only CSR.
`csr_write_target` already encodes this correctly with `rs1 != 0`
(`src/machine.rs:876-877`).

## Specification requirement

Unprivileged ISA, §4.2.1 (CSR Instructions), local `riscv-spec.html`, anchor
`norm:csrrs_csrrc_rs1_x0`:

> For both CSRRS and CSRRC, if rs1=x0, then the instruction will not write to
> the CSR at all, and so shall not cause any of the side effects that might
> otherwise occur on a CSR write, nor raise illegal-instruction exceptions on
> accesses to read-only CSRs. … Note that if rs1 specifies a register other
> than x0, and that register holds a zero value, the instruction will not
> action any attendant per-field side effects, but will action any side effects
> caused by writing to the entire CSR.

Anchor `norm:Zicsr_illegal_acc`:

> Attempts to write a read-only CSR address raise illegal-instruction
> exceptions.

Zicntr (§4.3) makes the counters read-only and calls out the canonical read
form:

> The counter pseudoinstructions are mapped to the read-only CSRRS rd, counter,
> x0 canonical form, but the other read-only CSR instruction forms … are also
> legal ways to read these CSRs.

Against the code: `src/machine.rs:629` and `:637` test `source != 0`, so
`csrrs rd, cycle, x1` with `x1 == 0` performs no write check and retires
successfully, in M-mode or in U-mode when `mcounteren.CY` is set.

## Expected

An explicit write to a read-only CSR (`cycle`, `time`, `instret`) raises
`Exception(CAUSE_ILLEGAL_INSTRUCTION = 0x2)` with the instruction word in
`mtval`; `rd` and the PC are unchanged.

## Actual

No trap. `rd` receives the old counter value and the instruction retires.

## Why it escaped

Tests cover CSRRS/CSRRC with nonzero values and the `rs1 = x0` read form, but
not the "register other than x0 holds zero" case, nor read-only write attempts
from U-mode with counter access enabled.

## Fix

Branch on the register index, matching `csr_write_target`:

```rust
if rs1 != X0_INDEX {
    self.write_csr(csr, old | source, word)?;
}
```

for both CSRRS and CSRRC. Keep CSRRsi/CSRRCI on `zimm != 0`, which is the
correct uimm-zero rule. Introduce a named `u8` x0 index (or derive one from
`cpu::X0`) rather than a bare literal, and reuse the same predicate in
`csr_write_target`.

## Regression tests

- `csrrs_zero_valued_source_to_read_only_csr_traps` (`cycle`).
- `csrrc_zero_valued_source_to_read_only_csr_traps` (`instret`).
- U-mode with `mcounteren.CY` set: `rs1 = x0` reads succeed; `rs1 != x0` with a
  zero-valued register traps with cause `0x2`.
- Assert `rd` and the PC are untouched on the trapping paths.

## Prevention

Test the matrix {`rs1 = x0`, `rs1 != x0` holding 0, nonzero value} ×
{read-only CSR, writable CSR} for CSRRS/CSRRC, and keep the write-target
predicate in one helper shared by execution and counter bookkeeping.

## Verification

`cargo test` and `cargo clippy --all-targets -- -D warnings` pass with the new
read-only and U-mode regressions.
