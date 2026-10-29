# Bug 0022: Machine interrupts are masked in lower privilege modes

| Field | Value |
|---|---|
| Found | 2026-10-29, code review of commit `b5f1a1e` |
| Fixed | 2026-10-29 |
| Affected | `src/csr.rs:519-520` (`Csrs::pending_interrupt`, nondelegated arm) |

## Summary

For a nondelegated interrupt the target is M-mode, but enablement is decided
solely by `mstatus.MIE`:

```rust
} else {
    (PrivilegeMode::Machine, self.mstatus & MSTATUS_MIE != 0)
};
```

Per the privileged ISA, a machine interrupt is globally enabled whenever the
current privilege is lower than M, regardless of MIE; MIE gates delivery only
while already running in M-mode.

## Specification requirement

Privileged ISA, §3.1.9 (`mip`/`mie`), local `riscv-spec.html`, anchor
`norm:intr_mip_mie_op`:

> An interrupt i will trap to M-mode … if all of the following are true: (a)
> either the current privilege mode is M and the MIE bit in the mstatus
> register is set, or the current privilege mode has less privilege than
> M-mode; (b) bit i is set in both mip and mie; and (c) if register mideleg
> exists, bit i is not set in mideleg.

§3.1.6 (`mstatus`), anchor `norm:mstatus_mie_sie_op2`:

> Interrupts for higher-privilege modes, y>x, are always globally enabled
> regardless of the setting of the global yIE bit for the higher-privilege
> mode.

Against the code: MIE=0 suppresses M-target interrupts even when S or U mode is
executing.

## Reproduction

1. Run in S or U mode with `mstatus.MIE = 0`.
2. Set a machine interrupt in both `mip` and `mie` (e.g. MTIP or MEIP).
3. Step the machine.

Expected: trap to M-mode with the interrupt cause.
Actual: `pending_interrupt` returns `None`; the interrupt stays pending and the
hart keeps executing.

## Impact

CLINT timer, PLIC, and machine software interrupts can be lost or indefinitely
delayed whenever a lower-privilege context is entered with MIE clear, defeating
preemption from S/U mode.

## Why it escaped

Interrupt tests enable MIE before delivering and do not exercise a lower
privilege with MIE clear.

## Suggested fix

```rust
let globally_enabled = privilege != PrivilegeMode::Machine
    || self.mstatus & MSTATUS_MIE != 0;
(PrivilegeMode::Machine, globally_enabled)
```

Keep the existing delegated-interrupt logic, which already treats SIE as
ignored from U mode.

## Regression tests

- M-mode with MIE=0 takes no machine interrupt.
- S and U modes with MIE=0 take pending MTIP/MEIP/MSIP.
- Delegated S interrupts keep their SIE/U-mode behaviour.
- Trap entry sets MIE=0 and MPIE to the previous MIE.

## Prevention

Test the full matrix of {current privilege × target privilege × global enable ×
delegation}, not only M-mode delivery.

## Fix

Nondelegated interrupts are now globally enabled whenever the current privilege
is below M-mode. `mstatus.MIE` gates machine interrupts only while the hart is
already executing in M-mode.

## Regression tests

- `machine_interrupts_respect_mie_and_use_architectural_priority`
- `machine_interrupt_preempts_supervisor_mode_with_mie_clear`
- `delegated_interrupts_can_preempt_user_mode_without_sie`
