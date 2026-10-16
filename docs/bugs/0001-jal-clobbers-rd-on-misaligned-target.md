# Bug 0001: `jal` clobbers `rd` when the jump target is misaligned

| Field | Value |
|---|---|
| Found | 2026-10-16, code review of commit `115e3c7` |
| Fixed | 2026-10-16 |
| Affected | `src/machine.rs`, `Machine::execute`, `Instruction::Jal` arm (`HEAD` lines 287-288) |

## Summary

The `Jal` arm linked `rd` before checking that the computed target is aligned:

```rust
self.write(rd, next_pc);
next_pc = Self::checked_target(pc.wrapping_add_signed(offset))?;
```

When the target was misaligned, `execute` returned
`Err(Trap::InstructionAddressMisaligned)` with `rd` already overwritten, even
though `checked_target` is documented as trapping "before any state changes".
`Jalr` and the branches already checked the target before writing, so `jal`
was the only control transfer that modified architectural state on a trap.

## Impact

A precise trap must leave architectural state untouched. With the old order,
`jal ra, +2` (`0x002000ef`, reachable by any J-type with `imm[1]` set) left
`ra = pc + 4`, so a trap handler resuming the instruction would observe a
corrupted link register. The deviation also contradicted the stated invariant
and the behaviour of every other jump/branch.

## Reproduction

Unit level, PC at 0:

```rust
machine.execute(Instruction::Jal { rd: 5, offset: 2 })
// -> Err(Trap::InstructionAddressMisaligned)
// bug:   read_gpr(5) == 4
// fixed: read_gpr(5) == 7 (untouched), pc() == 0
```

## Why it escaped

`jal` had tests for successful jumps and for the `jal x0, 0` halt convention,
but no test for a trapping jump; `jalr` and the branches did have misaligned
target tests, and those covered `rd`/PC side effects only implicitly until
this fix strengthened them.

## Fix

Compute and check the target first, then link (mirrors `Jalr`):

```rust
let target = Self::checked_target(pc.wrapping_add_signed(offset))?;
self.write(rd, next_pc);
next_pc = target;
```

## Regression tests

- `jal_to_a_misaligned_target_traps` (new): `rd` must not change, PC must not move.
- `jalr_to_a_misaligned_target_traps` (strengthened): now also asserts `rd` is
  untouched, keeping the two jump paths in lockstep.

## Verification

`cargo test`: 53 unit tests and 1 integration test pass (fix applied).
`cargo clippy --all-targets`: clean.

## Prevention

When a trap path is added, assert the *absence* of state change (`rd`, PC)
alongside the trap cause, as these tests now do. Mutation testing
(`cargo-mutants`) over `machine.rs` would also flag reordering mutants here.
