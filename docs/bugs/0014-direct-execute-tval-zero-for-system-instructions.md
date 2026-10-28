# Bug 0014: Direct `execute` records zero `mtval`/`stval` for new system instructions

| Field | Value |
|---|---|
| Found | 2026-10-28, code review of commit `a3c6052` |
| Fixed | 2026-10-28 |
| Affected | `src/machine.rs:221-225` (`execute`), `:1940-1962` (`instruction_word`), `:1080`/`:1095`/`:1105` (SRET/WFI/SFENCE.VMA) |

## Summary

`Machine::execute` synthesizes a raw instruction word for direct calls:

```rust
let decoded = DecodedInstruction::standard(instruction, instruction_word(instruction));
```

`instruction_word` re-encodes only the six CSR instructions and returns `0`
for everything else. The illegal-instruction paths for `SRET`, `WFI`, and
`SFENCE.VMA` therefore record `mtval`/`stval` as zero when reached through the
public `execute` API, even though this platform sets
`REPORT_ENCODING_IN_STVAL_ON_ILLEGAL_INSTRUCTION: true`
(`tests/compliance/act4/arvsr-rv64imafdcb.yaml:82`).

## Specification requirement

Privileged ISA, `mtval`/`stval` illegal-instruction behavior, local
`riscv-spec.html`, anchors `norm:mtval_ill_instr_exc_in_low_bits` and
`norm:stval_op_illegal_instr_format`:

> The mtval register can optionally also be used to return the faulting
> instruction bits on an illegal-instruction exception … The value loaded into
> mtval on an illegal-instruction exception is right-justified and all unused
> upper bits are cleared to zero.

The project’s ACT4 configuration selects that option, so the encoding must be
reported consistently. The `step` path already carries the fetched word; only
direct `execute` loses it.

## Reproduction

- U-mode `sret` through `execute`: expected `mtval`/`stval = 0x10200073`.
- U-mode `wfi`: expected `0x10500073`.
- `sfence.vma x0, x0` in a trapping mode: expected `0x12000073`.

Current tests assert zero (`src/machine.rs:3666`, `:3730`), cementing the bug.

## Impact

Unit tests, API users, and future differential tests observe different trap
state depending on whether the instruction came from memory or `execute`.
Delegated traps inherit the same wrong `stval`.

## Why it escaped

`instruction_word` was extended only for CSR instructions (needed by earlier
direct-execute trap tests). No test covered the new privileged system
instructions, and the existing assertions expected zero.

## Suggested fix

Prefer carrying the raw word from decode instead of re-encoding. If the
re-encoder remains, add the fixed encodings:

- `SRET` → `inst::MATCH_SRET`
- `WFI` → `inst::MATCH_WFI`
- `SFENCE.VMA` → base `inst::MATCH_SFENCE_VMA` with `rs1 << 15` and `rs2 << 20`

and update the tests to assert those values.

## Regression tests

- Illegal SRET/WFI/SFENCE.VMA through `execute` assert the exact encoding in
  `mtval` (M-mode trap) and `stval` (delegated S-mode trap).
- A nonzero raw word from fetched encodings remains identical to the direct
  path for the same instruction.

## Prevention

Store the original encoding in `Instruction` or `DecodedInstruction` and never
reconstruct it; assert `mtval`/`stval` for every illegal-instruction path.

## Fix

`instruction_word` now reconstructs fixed encodings for `ecall`, `ebreak`,
`mret`, `sret`, and `wfi`, and reconstructs SFENCE.VMA register fields. Direct
execution therefore reports the same illegal-instruction payload as fetched
instructions.

## Regression tests

- `system_instruction_words_round_trip_through_generated_encodings`
- `privileged_instruction_controls_trap_in_supervisor_mode`
- `delegated_illegal_instruction_reports_encoding_in_stval`

## Verification

- `cargo test`
- `cargo clippy --all-targets -- -D warnings`
- Legacy and ACT4 S-mode compliance gates
