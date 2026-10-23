# Bug 0007: Spurious underflow when a result rounds up to the minimum normal

| Field | Value |
|---|---|
| Found | 2026-10-23, code review of commit `8fb74cf` |
| Fixed | 2026-10-23 |
| Affected | `src/fp.rs:874-883`, `tiny_after_rounding`; shared by add, divide, and fused operations |

## Summary

The rounding routine classifies a result that lands exactly on the minimum
normal (`0x00800000`) as tiny when rounding is directed outward:

```rust
let directed_outward = matches!(mode, RoundingMode::Up) && !value.negative
    || matches!(mode, RoundingMode::Down) && value.negative && half_order.is_lt();
let tiny_after_rounding = exact_exponent + 1 < MIN_NORMAL_EXPONENT
    || significand < HIDDEN_BIT
    || significand == HIDDEN_BIT && directed_outward;
```

RISC-V detects tininess *after* rounding. The rounded magnitude
`2^-126` is the smallest normal value, not a tiny value, so underflow must not
be raised even though the exact product was just below it.

## Specification requirement

Unprivileged ISA, §6.1.4 (Subnormal Arithmetic), local `riscv-spec.html`,
anchor `norm:ieee_std_tininess`:

> In the parlance of IEEE 754-2008, tininess is detected after rounding.

The same section notes:

> Detecting tininess after rounding results in fewer spurious underflow
> signals.

Against the code: the final term at `src/fp.rs:880` marks
`significand == HIDDEN_BIT` (exactly the minimum normal) as tiny whenever the
rounding direction is outward, contradicting the after-rounding rule.

## Reproduction

`fmul.s` with rounding mode `rup`:

- `rs1 = 0x00800001` (minimum normal plus one ULP)
- `rs2 = 0x3f7ffffe` (largest float below 1.0 minus one ULP)
- exact product is just below `2^-126`
- rounded result: `0x00800000` (minimum normal)

Expected `fflags`: `FLAG_NX` (`0x1`) only.
Actual: `FLAG_NX | FLAG_UF` (`0x3`); ACT4 expects only `NX`.

## Impact

Every operation that shares this rounding routine — add, subtract, multiply,
divide, square root, and fused multiply-add — can raise a spurious underflow
exception near the normal boundary. Trap handlers and ACT4 flag checks see
`UF` where only `NX` is architecturally required.

## Why it escaped

The unit tests cover a genuinely tiny nearest-even result, but not an outward
rounding that lands exactly on `MIN_NORMAL`; the F fixture contains no static
rounding modes or boundary operands.

## Suggested fix

Derive tininess from the final rounded magnitude: a result equal to
`MIN_NORMAL` is not tiny. Remove the
`significand == HIDDEN_BIT && directed_outward` term (and the now-unused
`directed_outward` computation), or replace the predicate with one that tests
the final value against the minimum normal. Keep `UF` for genuinely tiny
inexact results.

## Fix

Tininess is now determined by rounding the exact result with unbounded exponent
range to 24-bit precision and comparing that rounded magnitude with
`MIN_NORMAL`. This follows IEEE 754's after-rounding definition rather than
guessing from the final packed result or directed-rounding direction.

The helper accounts for `tiny_subtract` crossing a power-of-two boundary. That
keeps genuinely tiny cases—such as an unbounded rounded result equal to the
maximum subnormal—raising `UF`, while a result that rounds to minimum normal
raises only `NX`.

## Regression tests

- `mul(0x00800001, 0x3f7ffffe, RoundingMode::Up)` returns bits
  `0x00800000` with exactly `FLAG_NX`.
- Divide and fused cases verify that genuinely tiny unbounded results still
  raise `UF`, including negative directed rounding.
- Boundary sweep around `MIN_NORMAL` for all five rounding modes.
- Re-run the F ACT4 gate.

## Verification

- `cargo test`
- `cargo clippy --all-targets -- -D warnings`
- `cargo test --test compliance act4_gate -- --ignored --nocapture`

## Prevention

Add min-normal boundary vectors for each operation family and rounding mode,
and compare flag results against Spike/TestFloat rather than only final bits.
