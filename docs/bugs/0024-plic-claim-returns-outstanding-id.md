# Bug 0024: PLIC claim returns an already-claimed source and misassembles reads

| Field | Value |
|---|---|
| Found | 2026-10-29, code review of commit `b5f1a1e` |
| Fixed | 2026-10-29 |
| Affected | `src/platform.rs:467-471` (`Plic::claim`), claim read path at `:410-414` |

## Summary

`claim` returns the stored ID whenever the context already owns a claim:

```rust
fn claim(&mut self, context: usize, uart_pending: bool) -> u32 {
    if self.claimed[context] != PLIC_NO_SOURCE {
        return self.claimed[context];
    }
    ...
}
```

An independent second read of the claim/complete register therefore reports the
same interrupt again before completion. In addition, the Bus default
`read_u32` assembles four `read_u8` calls; each byte read invokes `claim`
again, so one 32-bit claim can return `0x01010101` instead of `0x00000001`.

## Specification requirement

RISC-V PLIC Specification v1.0.0, §7 (Claim/Complete): a claim read returns the
ID of the highest-priority pending interrupt, or 0 if none is pending. Once a
source is claimed, its gateway stops forwarding requests and a further claim
before completion returns 0.

Against the code: an outstanding claim is returned repeatedly, and the
byte-wise assembly multiplies it across the four bytes.

## Reproduction

1. Enable the UART source and make it pending.
2. Read the claim register once → 1.
3. Read it again before writing completion.

Expected: second read returns 0.
Actual: second read returns 1.

Repeat via a 32-bit Bus read: the assembled value is `0x01010101` rather than
`0x1`.

## Impact

Software can process the same UART interrupt twice and can complete a source
that was never newly claimed. Interrupt storms and lost completion ordering are
possible.

## Why it escaped

Tests claim once and complete immediately; no duplicate-claim or word-sized
claim read is asserted.

## Suggested fix

- `claim` returns `PLIC_NO_SOURCE` when `claimed[context] != PLIC_NO_SOURCE`.
- Override `Platform::read_u32` (or special-case the claim register) so one
  32-bit read performs exactly one claim and returns the ID in the low byte with
  upper bytes zero.
- Keep completion writes clearing `claimed`.

## Regression tests

- Two claim reads before completion → `(1, 0)`.
- One 32-bit claim read → `0x1`, not `0x01010101`.
- Completion allows the next pending source to be claimed.
- Claims in separate contexts remain independent.

## Prevention

Treat claim as a 32-bit side-effecting register, test byte/half/word accesses,
and model outstanding claims explicitly.

## Fix

An outstanding claim now causes subsequent claim reads to return zero. Only
the low byte of the claim register initiates a claim; upper bytes return zero,
so the default word-sized Bus read assembles `0x0000_0001`. Completion clears
the context's outstanding claim and lets another enabled context claim the
still-pending source.

## Regression tests

- `plic_routes_uart_interrupts_to_enabled_contexts`
