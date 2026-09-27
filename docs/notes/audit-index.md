# Audit index — removed documents

Owner: assistant. Files removed from the tree are still readable in git history:

    git show <commit>:<path>

| Removed path | Removed | Last commits containing it | What it held | Where its conclusions live now |
|---|---|---|---|---|
| `docs/verification/s1-freeze.md` | 2026-09-27 | `0b452cc` (redirect stub) · `9927095` (full text) | The pre-baseline freeze: oracle invocation, records, normalization list, initial state, CSR table, limits | `docs/verification/s1-surface.md` (baseline v3) |
| `docs/notes/s1-baseline-review.md` | 2026-09-27 | `0b452cc` · `1456d9e` · `1384a1f` (as `s1-freeze-review.md`) | Independent re-measurement of the freeze and the two corrections it required | Surface change notes 01 and 02; work order 001 |
| `docs/notes/s1-harness-review.md` | 2026-09-27 | `28f3099` | Review of the harness design and the five additions it required | Work order 001 (approval line); implemented in `tests/s1_differential.rs` |

## Known dead pointers

- `docs/adr/006-s1-verification-policy.md` line 4 names `docs/verification/s1-freeze.md`. Accepted
  ADRs are append-only, so this index carries the pointer instead: the redirect stub is at
  `0b452cc`, and the full document it superseded is at `9927095`.
- `docs/verification/s1-harness-design.md` line 200 names `docs/notes/s1-harness-review.md`. The
  five additions it summarises are restated in work order 001; the verifier may re-point that line.
