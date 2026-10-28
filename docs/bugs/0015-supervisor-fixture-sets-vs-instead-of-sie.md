# Bug 0015: Supervisor fixture writes `sstatus.VS` instead of `sstatus.SIE`

| Field | Value |
|---|---|
| Found | 2026-10-28, code review of commit `a3c6052` |
| Fixed | 2026-10-28 |
| Affected | `tests/fixtures/supervisor.S:29` |

## Summary

The fixture claims to set SIE but uses `0x200`:

```asm
li t0, 0x200              # sstatus.SIE.
csrw sstatus, t0
```

`0x200` is bit 9, the low bit of `sstatus.VS` (bits 10:9), not `SIE` (bit 1).
`VS` is read-only zero in this configuration, so the fixture never enables
supervisor interrupts even though it later reads and compares `sstatus`.

## Specification requirement

Privileged ISA, §4.1.1 (`sstatus`), local `riscv-spec.html`:

> The SIE bit enables or disables all interrupts in supervisor mode. When SIE
> is clear, interrupts are not taken while in supervisor mode.

`SIE` is bit 1 of `sstatus` (Figure 45); the intended value is `0x2`, not
`0x200`.

## Impact

The Spike-oracle fixture passes because both simulators ignore the VS write;
the intended SIE path is untested. Trap-entry behavior (SPIE ← SIE, SIE ← 0)
can regress without this fixture noticing.

## Why it escaped

The comment asserted the intent, and the read-back value was not compared
against `0x2`; Spike agrees on the ignored VS write, so the differential test
stayed green.

## Suggested fix

Use `0x2` for SIE:

```asm
li t0, 0x2                # sstatus.SIE.
csrw sstatus, t0
```

Optionally assert the read-back equals `0x2` before the trap.

## Regression tests

- `csrr s1, sstatus` after the write equals `0x2` (or SIE is set).
- Take a delegated trap with SIE set and verify `SPIE = 1`, `SIE = 0` on entry,
  and SIE restored on SRET.
- Keep the spike comparison.

## Prevention

Use named bit values/constants in assembly fixtures and assert read-back values,
not only comments.

## Fix

The fixture now writes `0x2` to `sstatus.SIE`, checks the immediate read-back,
and verifies SIE is clear while SPIE is set on entry to both delegated traps.
The differential test also asserts the expected GPR sentinels independently of
Spike, preventing a shared failure path from masking the bug.

## Regression tests

- `supervisor_fixture_matches_spike`

## Verification

- `cargo test --test supervisor_run`
- Full `cargo test`
