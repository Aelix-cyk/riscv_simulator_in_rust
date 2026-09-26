# Report 002 — a correction applied to the freeze was itself wrong

Date: 2026-09-26 · Author: verifier · Subject: `docs/verification/s1-freeze.md` §5.5

## What happened

`docs/notes/s1-freeze-review.md` (reviewer: assistant) approved the freeze with two wording
corrections. Correction 2 replaced §5.5's justification with: *"Its log therefore ends on a later
iteration of the same store instruction, not on a different instruction."*

That sentence is false for 15 of the 71 binaries. It is also the sentence written to protect the
truncation rule from a future reader, so it fails at the one job it had.

## Measurement

For each of the 71 reference logs: the commit index of the last store to `tohost` with a nonzero
value, versus the index of the last commit in the log.

| Commits after the last tohost store | Binaries |
|---|---|
| 0 — the log ends on the store | 56 |
| 1 — ends on `auipc t5, 0x1` | 14 |
| 3 — `rv64mi-p-illegal`, ends on the `j` back to the loop head | 1 |

Reproduce with the loop in §2 of the freeze: count commit records, find the last
`mem 0x0000000080001000 0x00000001` (or `…2000…` for `ld_st`/`ma_data`), subtract.

The rule in §5.5 is unaffected — truncate at the **first** nonzero tohost store — and the
implementer must still halt there. Only the old wording, which spoke of Spike "breaking off at a
different instruction", was closer to the truth for those 15 logs. §5.5 now states both facts:
the loop keeps spinning, and where the log stops is not fixed either.

## Why this is recorded

The review was an independent re-measurement and it reproduced every numeric claim it checked
(§1 of that note). The failure was not in the measuring but in the generalising: one binary
(`rv64ui-p-add`) was checked, where the log does end on the store, and the conclusion was written
as if it held for the corpus. The freeze's own §1 standard — counts are measured, not assumed —
applies to corrections as much as to the original text.

No architect decision is needed: §5.5's rule is unchanged, only its justification is corrected.
