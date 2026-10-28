# Bug 0011: Cache-geometry status contradicts itself across the choice docs

| Field | Value |
|---|---|
| Found | 2026-10-28, code review of commit `c45e658` |
| Fixed | 2026-10-28 |
| Affected | `docs/impl-choices/implement_specific_choices_list.md:33` and `:94`, `docs/impl-choices/rva22s64-implementation-choices.md:88` |

## Summary

The same decision is recorded with three different statuses:

- `implement_specific_choices_list.md:33`: **Decided** — "No cache model;
  architectural cache blocks are 64 bytes and naturally aligned".
- `implement_specific_choices_list.md:94` (section 6): "No cache model —
  **Open**".
- `rva22s64-implementation-choices.md:88` (profile traceability matrix):
  **Undecided**.

The profile matrix also lists Zic64b as **Implemented** at line 38, so the
platform already relies on the decided 64-byte geometry.

## Why this matters

Readers cannot tell whether cache geometry is settled, and the traceability
matrix understates conformance. Future work could reopen a decision that is
already implemented, or claim a profile answer that the matrix still calls
undecided.

## No specification change

This is an internal documentation inconsistency; the architectural choice
(Zic64b: 64-byte, naturally aligned blocks; no cache model) is unchanged.

## Suggested fix

Set all three rows to the same status and wording:

- keep line 33 as **Decided**;
- change the section 6 row (line 94) to **Decided** with the same text;
- change the profile matrix row 88 to **Implemented** (matching Zic64b at line
  38) with the note "No cache model; 64-byte, naturally aligned architectural
  blocks".

## Prevention

Update all choice tables in the same commit that changes a decision. Longer
term, derive the section 6 table and the traceability matrix from one source so
their statuses cannot drift.

## Fix

Both choice tables now describe the same decision: no cache model, with 64-byte,
naturally aligned architectural blocks. The profile matrix records this as
implemented and the section 6 summary records it as decided.

## Verification

- Reviewed all three cache-geometry rows for identical status and meaning.
- `git diff --check`
