# Bug 0013: `mip`/`sip` writes drop writable STIP and SEIP bits

| Field | Value |
|---|---|
| Found | 2026-10-28, code review of commit `a3c6052` |
| Fixed | 2026-10-28 |
| Affected | `src/csr.rs:94` (`MIP_WRITABLE`), write paths at `:275-276` |

## Summary

The pending-interrupt write mask allows only SSIP:

```rust
const MIP_WRITABLE: u64 = INTERRUPT_SSIP;
```

With S-mode implemented and neither `stimecmp` nor an external interrupt
controller present — the configured platform — `mip.STIP` and `mip.SEIP` are
software-writable. A M-mode write such as `0x220` (STIP | SEIP) is masked to
zero and reads back incorrectly.

The S-mode alias path also replaces the whole register:

```rust
inst::CSR_SIP => self.mip = value & self.mideleg & MIP_WRITABLE,
```

which erases every pending bit outside the computed mask, including M-level
MSIP/MTIP/MEIP.

## Specification requirement

Privileged ISA, §3.1.9 (`mip` and `mie`), local `riscv-spec.html`, anchors
`norm:mip_seip_acc`, `norm:mip_stip_no_stimecmp_acc`:

> If supervisor mode is implemented, bits mip.SEIP and mie.SEIE are the
> interrupt-pending and interrupt-enable bits for supervisor-level external
> interrupts. SEIP is writable in mip, and may be written by M-mode software to
> indicate to S-mode that an external interrupt is pending.

> If the stimecmp register is not implemented, STIP is writable in mip, and may
> be written by M-mode software to deliver timer interrupts to S-mode.

Against the code: `MIP_WRITABLE` omits both bits, and the SIP path does not
preserve unrelated pending bits.

## Expected

- M-mode write of `0x220` to `mip` stores STIP and SEIP; a read returns them
  (plus any unaffected pending bits).
- A write to `sip` affects only delegated, software-writable pending bits and
  leaves MSIP/MTIP/MEIP untouched.

## Actual

- `mip` reads back without STIP/SEIP; `0x220` is discarded.
- `sip` writes replace `mip` wholesale with the masked value, clearing
  non-delegated pending bits.

## Why it escaped

Interrupt tests exercise SSIP and delegation but never write STIP/SEIP through
`mip`, nor check that a `sip` write preserves M-level pending state.

## Suggested fix

Define separate masks for the M-view and S-view writes, then use
read-modify-write:

```rust
const MIP_WRITABLE: u64 =
    INTERRUPT_SSIP | INTERRUPT_STIP | INTERRUPT_SEIP;
// SIP: MIP_WRITABLE & mideleg, per visible delegation
self.mip = (self.mip & !mask) | (value & mask);
```

Keep platform-specific exclusions (stimecmp / external controller) explicit if
those devices are added later.

## Regression tests

- M-mode `csrw mip, 0x220` reads back both bits.
- A `sip` write with MSIP/MTIP/MEIP pending preserves those bits.
- Non-delegated SIP bits remain read-only zero.
- STIP/SEIP delivery to S-mode works after the write.

## Prevention

Add a table-driven writable-bit test per CSR view (`mip` vs `sip`) instead of
testing only SSIP.

## Fix

`mip` now retains software-writable SSIP, STIP, and SEIP. Both `mip` and `sip`
use read-modify-write updates, preserving pending bits outside each view's
writable mask. Delegated, enabled supervisor pending bits are now delivered at
instruction boundaries with architectural interrupt priority.

## Regression tests

- `mip_and_sip_write_only_writable_pending_bits_and_preserve_the_rest`
- `enabled_delegated_supervisor_interrupts_use_architectural_priority`
- `software_pending_supervisor_timer_interrupt_enters_supervisor_handler`

## Verification

- `cargo test`
- `cargo clippy --all-targets -- -D warnings`
- Legacy `rv64si` compliance gate
- ACT4 S-mode compliance gate
