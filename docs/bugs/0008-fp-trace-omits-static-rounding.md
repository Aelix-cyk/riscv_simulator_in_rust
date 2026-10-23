# Bug 0008: Trace output omits static floating-point rounding modes

| Field | Value |
|---|---|
| Found | 2026-10-23, code review of commit `8fb74cf` |
| Fixed | 2026-10-23 |
| Affected | `src/decode.rs:1605-1609` (`FpBinary`, `FpSqrt`), `:1611-1625` (`FpFused`), `:1636-1659` (`FpToInt`, `IntToFp`) |

## Summary

Each FP instruction variant carries a decoded `rounding` field, but the
renderer destructures it with `..` and drops it:

```rust
Instruction::FpBinary {
    op, rd, rs1, rs2, ..
} => fp_reg_op(fp_binary_mnemonic(op), rd, rs1, rs2),
```

Static forms therefore lose their mode suffix. For example
`fmul.s ft2,ft0,ft1,rup` renders as `fmul.s ft2,ft0,ft1`, disagreeing with
`objdump` and with the decoded encoding. `fsqrt`, the fused multiply-add
family, `fcvt.w.s`/`fcvt.l.s`, and `fcvt.s.w`/`fcvt.s.l` have the same
omission. Dynamic rounding (`rm = 111`) correctly has no suffix, and operations
without an `rm` operand (`fmin`, `fmax`, `fsgnj*`) must not gain one.

## Specification requirement

Unprivileged ISA, §6.1.2 (FCSR and rounding modes), local `riscv-spec.html`,
anchor `norm:roundingmode_dynamic`:

> Floating-point operations use either a static rounding mode encoded in the
> instruction, or a dynamic rounding mode held in frm. Rounding modes are
> encoded as shown in Table 26. A value of 111 in the instruction's rm field
> selects the dynamic rounding mode held in frm.

The static encoding is part of the instruction, so a disassembly/trace that
omits it is lossy. The differential oracle is `objdump`
(`tests/render.rs`, `tests/support/assemble.rs`).

## Impact

Traces no longer show which static rounding mode produced a result, making
rounding-dependent flag and value differences hard to diagnose. Renderer
differential tests miss it because `tests/fixtures/f.S` uses only the default
dynamic forms.

## Suggested fix

Add one rounding-suffix helper that maps the encoded mode to
`,rne`, `,rtz`, `,rdn`, `,rup`, `,rmm`, and returns an empty string for
`DYNAMIC_ROUNDING` (and rejects reserved encodings, as decode already does).
Use named constants rather than bare literals. Apply it to:

- `FpBinary` only for operations with an `rm` operand (add, subtract,
  multiply, divide) — not `fmin`/`fmax`/`fsgnj*`.
- `FpSqrt`.
- `FpFused`.
- `FpToInt` and `IntToFp`.

## Fix

Added a shared rounding-suffix helper with named encodings for RNE, RTZ, RDN,
RUP, RMM, and Dynamic modes. Trace rendering now appends the mode after the
operand list on every instruction with an architectural `rm` operand.
Sign-injection and min/max operations do not gain a suffix.

## Regression tests

Add fixture lines (and objdump comparison) covering a static mode on each arm:
`fmul.s ...,rup`, `fsqrt.s ...,rtz`, `fmadd.s ...,rdn`, `fcvt.w.s ...,rmm`,
and `fcvt.s.w ...,rne`. Also assert that dynamic forms and `fmin`/`fsgnj`
render without a suffix.

## Verification

- `cargo test --test render`
- `cargo test --test f_run`
- `cargo test`

## Prevention

Exercise all representable `rm` values in the renderer differential test and
keep mode-to-text mapping in one shared helper used by traces and tests.
