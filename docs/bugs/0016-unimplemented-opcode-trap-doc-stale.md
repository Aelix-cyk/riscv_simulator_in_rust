# Bug 0016: Unimplemented-opcode documentation ignores S-mode delegation

| Field | Value |
|---|---|
| Found | 2026-10-28, code review of commit `a3c6052` |
| Fixed | 2026-10-28 |
| Affected | `docs/impl-choices/implement_specific_choices_list.md:52` |

## Summary

The choice table states:

> Behaviour on unimplemented opcodes | Illegal-instruction exception, delivered
> through mtvec | Decided

This is now false in general: with S-mode implemented, `medeleg` can delegate an
illegal-instruction exception raised in S or U mode to S-mode, where it is
delivered through `stvec`.

## Specification requirement

Privileged ISA, trap delegation, local `riscv-spec.html`, anchor
`norm:trap_del_S-mode`:

> When a trap is delegated to S-mode, the scause register is written with the
> trap cause; the sepc register is written with the virtual address of the
> instruction that took the trap; the stval register is written with an
> exception-specific datum; the SPP field of mstatus is written with the active
> privilege mode at the time of the trap; the SPIE field of mstatus is written
> with the value of the SIE field at the time of the trap; and the SIE field of
> mstatus is cleared.

The trap vector then comes from `stvec`, not `mtvec`.

## Impact

Readers and test authors can assume all illegal instructions reach `mtvec`,
missing the implemented delegation path and the S-mode state updates.

## Suggested fix

Qualify the row, for example:

> Illegal-instruction exception, delivered through `mtvec`, or through `stvec`
> when delegated to S-mode via `medeleg`.

Apply the same qualification anywhere the table implies M-mode-only delivery.

## Prevention

Update the choice tables in the same commit that adds privilege/delegation
features, and grep the docs for "mtvec" whenever delegation changes.

## Fix

The implementation-choice table now documents delivery through `mtvec` or a
delegated `stvec` path. The RVA22 traceability matrix now records the same
decision.

## Verification

- Documentation grep for unimplemented-opcode trap routing
- `git diff --check`
