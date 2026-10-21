# Bug 0004: `c.lui` renders negative immediates as 64-bit values

| Field | Value |
|---|---|
| Found | 2026-10-21, code review of commit `a2f2d72` |
| Fixed | 2026-10-21 |
| Affected | `src/decode.rs:1175-1177`, `CompressedInstruction::render`, `Lui` arm (line 1176) |

## Summary

The compressed renderer prints the U-immediate with a raw sign-preserving
shift:

```rust
Self::Lui { rd, imm, .. } => {
    format!("c.lui {},{:#x}", abi_name(rd), imm >> U_IMMEDIATE_SHIFT)
}
```

`imm` is a sign-extended `i64`, so for a negative `nzimm` the shifted value
stays negative and `{:#x}` prints its 64-bit two's-complement form instead of
the 20-bit U-immediate field. The standard `lui` renderer at line 1685 already
does this correctly with `u_field(imm)`.

## Specification requirement

Unprivileged ISA, Zca extension, local `riscv-spec.html`, anchor
`norm:c-lui_op`:

> C.LUI loads the non-zero 6-bit immediate field into bits 17-12 of the
> destination register, clears the bottom 12 bits, and sign-extends bit 17 into
> all higher bits of the destination. It expands into LUI rd, imm.

The operand is therefore the same 20-bit U-immediate field (`imm[31:12]`) that
`u_field` extracts for `lui`; the disassembly convention prints that field as
hex.

## Reproduction

```rust
render(0x7081)
// expected: "c.lui ra,0xfffe0"
// actual:   "c.lui ra,0xffffffffffffffe0"
```

## Impact

`--trace` and `render(word)` disagree with `objdump` for every negative `c.lui`
immediate, i.e. half of the immediate space. Trace-diff tests and users
comparing disassembly see spurious mismatches. Execution semantics are
unaffected; only rendering is wrong.

## Why it escaped

The only `c.lui` fixture is `c.lui x18, 1` (`tests/fixtures/compressed.S:32`).
For positive immediates, `imm >> U_IMMEDIATE_SHIFT` and
`u_field(imm)` are equal, so the differential renderer test never sees the
negative case.

## Fix

Mirror the standard `Lui` renderer:

```rust
Self::Lui { rd, imm, .. } => {
    format!("c.lui {},{:#x}", abi_name(rd), u_field(imm))
}
```

`u_field` already masks with the named `U_FIELD_MASK` constant, so no new magic
numbers are needed.

## Regression tests

- `compressed_immediates_and_control_flow_decode` asserts
  `render(0x7081) == "c.lui ra,0xfffe0"`.
- The compressed fixture covers both `nzimm = -32` and `nzimm = +31`, and the
  existing `objdump` renderer differential test compares both traces.

## Verification

`cargo test`, `cargo clippy --all-targets -- -D warnings`, the `rv64uc`
riscv-tests gate, and the ACT4 Zca gate pass.

## Prevention

Exercise the full `c.lui` immediate field in the renderer differential test,
not just one positive encoding, and assert that compressed and standard
U-immediate renderers produce the same hex field for the same value.
