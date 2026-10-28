# Bug 0012: Widening FP conversions skip rounding-field validation

| Field | Value |
|---|---|
| Found | 2026-10-28, code review of commit `7beff96` |
| Fixed | 2026-10-28 |
| Affected | `src/machine.rs:1573-1607` (`execute_fp_convert`), widening arms at `:1584-1592` |

## Summary

`execute_fp_convert` resolves the rounding mode only in the narrowing arms:

```rust
(FpFormat::Single, FpFormat::Half) => {
    let mode = self.rounding_mode(rounding, word)?;
    ...
}
```

The widening conversions — Half→Single, Half→Double, and Single→Double — never
call `rounding_mode`, so their `rm` field is not validated:

```rust
(FpFormat::Half, FpFormat::Single) => {
    let result = fp16::to_single(self.read_f16(rs1));
    ...
}
```

Reserved static encodings (`rm = 5` or `6`) and invalid dynamic values in
`frm` therefore execute successfully instead of raising an
illegal-instruction exception.

## Specification requirement

Unprivileged ISA, §6.1.2 (FCSR and rounding modes), local `riscv-spec.html`,
anchors `norm:roundingmode_dynamic` / `norm:roundingmode_rsv`:

> Floating-point operations use either a static rounding mode encoded in the
> instruction, or a dynamic rounding mode held in frm. … Some instructions,
> including widening conversions, have the rm field but are nevertheless
> mathematically unaffected by the rounding mode; software should set their rm
> field to RNE (000) but implementations must treat the rm field as usual (in
> particular, with regard to decoding legal vs. reserved encodings).

Against the code: because the widening arms skip `rounding_mode`, the
implementation does not treat the `rm` field as usual for exactly the
instructions the spec calls out.

## Reproduction

- `fcvt.s.h fd, fs` encoded with static `rm = 5` (reserved) must trap;
  currently it converts and writes `fd`.
- Same for `fcvt.d.h` and `fcvt.d.s` with `rm = 5` or `6`.
- With dynamic `rm = 7` and `frm` holding `5`, the widening conversion must
  trap; currently it succeeds.

Expected: `Exception(CAUSE_ILLEGAL_INSTRUCTION = 0x2)` with the instruction
word in `mtval`; `rd` and the PC unchanged. Spike already behaves this way.

## Impact

Malformed encodings execute instead of trapping, so the simulator diverges
from Spike and the ISA for compliance/negative tests. The hole applies to all
three widening paths and to both reserved static modes and reserved dynamic
`frm` values.

## Why it escaped

Unit and differential tests use valid rounding fields for widening
conversions; ACT4 valid-operation tests do not exercise reserved `rm`
encodings, so the missing validation is unobservable there.

## Suggested fix

Resolve the mode once before the match so every conversion validates it, then
use it in the narrowing arms and ignore it in the widening arms:

```rust
self.require_fp_enabled(word)?;
let mode = self.rounding_mode(rounding, word)?;
match (from, to) {
    // widening arms ignore `mode`
    ...
}
```

This also covers invalid dynamic `frm` values through the existing
`rounding_mode` helper; no new constants or magic numbers are needed.

## Regression tests

- `fcvt.s.h`, `fcvt.d.h`, and `fcvt.d.s` with static `rm = 5` and `rm = 6`
  each trap with cause `0x2`.
- Dynamic `rm = 7` with `frm = 5` traps for each widening conversion.
- A valid static mode (e.g. `rup`) still produces the exact widening result
  and raises no flags.
- Existing narrowing-conversion tests remain unchanged.

## Prevention

Validate the rounding field centrally for every FP instruction whose encoding
carries `rm`, and add a table-driven negative test over all format-conversion
pairs and reserved mode values.

## Fix

`execute_fp_convert` now resolves and validates the rounding mode once, before
dispatching on the source and destination formats. Narrowing conversions use
that mode for rounding; widening conversions continue to ignore it
mathematically while still rejecting reserved encodings.

## Regression tests

- `widening_fp_conversions_validate_rounding_mode` covers Half→Single,
  Half→Double, and Single→Double with both reserved static modes and an invalid
  dynamic `frm`.
- `widening_fp_conversions_ignore_valid_rounding_mode` verifies all three
  widening paths remain exact, write the expected result, and raise no flags
  when given a valid non-default mode.

## Verification

- `cargo test`
- `cargo clippy --all-targets -- -D warnings`
- `cargo test --test compliance riscv_tests_gate -- --ignored --nocapture`
- ACT4 `Zfh`, `Zfhmin`, `ZfhminD`, and `ZfhD` suites
