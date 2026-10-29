# Bug 0026: `Ssu64xl` is documented as not implemented despite UXLEN=64

| Field | Value |
|---|---|
| Found | 2026-10-29, code review of commit `993dc02` |
| Fixed | 2026-10-29 |
| Affected | `docs/impl-choices/rva22s64-implementation-choices.md:81`, `docs/impl-choices/implement_specific_choices_list.md:129` |

## Summary

The traceability matrix marks Ssu64xl as permanently excluded:

> RVA22S64 option | Ssu64xl | Not implemented | Permanently excluded

and §8 repeats it in the "permanently **Not implemented**" list. However, the
implementation hardwires 64-bit SXLEN/UXLEN in `mstatus`:

```rust
const MSTATUS_UXL_SHIFT: u32 = 32;
const MSTATUS_UXL_64: u64 = 2;
const MSTATUS_UXL: u64 = MSTATUS_UXL_64 << MSTATUS_UXL_SHIFT;
const MSTATUS_SXL_SHIFT: u32 = 34;
const MSTATUS_SXL_64: u64 = 2;
const MSTATUS_SXL: u64 = MSTATUS_SXL_64 << MSTATUS_SXL_SHIFT;
```

and the ACT4 configuration declares `UXLEN: [64]` and `SXLEN: [64]`
(`tests/compliance/act4/arvsr-rv64imafdcb.yaml:70,76`).

## Specification requirement

Privileged ISA, §8.2 (`Ssu64xl`), local `riscv-spec.html`, anchor `ssu64xl`:

> If the Ssu64xl extension is implemented, then sstatus.UXL must be capable of
> holding the value 2 (i.e., UXLEN=64 must be supported).

The implementation satisfies that condition: `sstatus.UXL`/`mstatus.UXL` read
value 2, and U-mode runs with UXLEN=64.

RVA22S64 lists Ssu64xl among its privileged **options** (§4.2.3), so the
correct traceability label is **Implemented** (a chosen supported option), not
Fixed (that label is reserved for profile mandates).

## Impact

The implementation-choice documentation understates conformance and contradicts
both the code and the ACT4 configuration. Consumers of the traceability matrix
could exclude a supported option from compliance claims.

## Suggested fix

- Change the matrix row to **Implemented**, with a note such as
  "UXLEN=64 hardwired; `sstatus.UXL` reads 2".
- Remove Ssu64xl from the §8 list of permanently unimplemented extensions and
  mention it in the implemented set.
- Leave Sv48, Sv57, Svnapot, Sstc, Sscofpmf, and Zkr excluded.

## Regression tests

- No code change required.
- Optional documentation check: derive the optional-extension statuses from the
  ACT4 configuration (`SXLEN`/`UXLEN`) and assert Ssu64xl is not listed as
  unimplemented.

## Fix

The matrix now marks Ssu64xl as **Implemented**, describing the hardwired
UXLEN=64 behavior. The optional-extension section now lists Ssu64xl separately
as implemented and removes it from the permanent exclusion list.

## Verification

- `sstatus.UXL` and `mstatus.UXL` continue to read `2`.
- The ACT4 configuration continues to declare `UXLEN: [64]` and `SXLEN: [64]`.
- `cargo test`: 219 unit tests and all local integration tests pass.
- `cargo test --test compliance act4_gate -- --ignored --nocapture`: 602 tests
  pass with 1 expected failure.

## Prevention

Update the choice tables in the same commit that changes a hardwired CSR field,
and cross-check the optional-extension list against the ACT4 config.
