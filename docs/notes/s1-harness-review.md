# Review of docs/verification/s1-harness-design.md

Reviewer: assistant · Date: 2026-09-26 · Verdict: **approve the shape**, with five additions.
One item needs your sign-off as a baseline change (§2), which the design itself flags.

## 1. Claims re-measured independently — all reproduce

| Claim in the design | Re-measured |
|---|---|
| `rv64ui-p-add`: 5511 records, first nonzero tohost store at record 516, expected stream 511 | identical ✓ |
| `rv64mi-p-illegal`: 5365 records, first halt store at 372, expected stream 367 | identical ✓ |
| Three trap records are immediately preceded by `mret`/`sret`, so "previous record's privilege" does not recover a trap's privilege | 3 found: two with `mret` executed at priv 3, one with `sret` at priv 1 ✓ |
| The raw glob returns 142 entries for 71 binaries | ✓ (`.dump` companions dropped) |

The trap-privilege argument is sound, and the counter-examples are exactly where the design says
they are. Deriving privilege inside the differ would make the differ reimplement trap-return
semantics, which ADR 006 already named as the risk to avoid.

## 2. The §4 correction — recommend you approve it

Dropping `p<priv>` from the trap record loses nothing that the corpus checks: the trap *name*
already distinguishes `user_ecall`, `supervisor_ecall` and `machine_ecall`, and the privilege is
still exercised by the handler's own commit records and by the mode seen when execution returns.
Keeping the field would force the differ to guess, which is worse than not comparing it.

Consequence to record when the change lands: `docs/verification/s1-surface.md` goes to **v2** with
a numbered change note, per the versioning rule adopted today.

## 3. Five additions before the harness is written

1. **Test the normalizer itself.** ADR 006 makes the differ trusted code. Add fixtures per rewrite
   rule — the two-line trap form, a line with a CSR annotation, a line with both a register write
   and a `mem`, the boot-ROM prefix, and a log with no halt store — as ordinary unit tests beside
   the diff.
2. **Guard against vacuous green.** Assert that the glob yields exactly 71 binaries, that both
   sides produced a non-empty record stream, and that the pinned configuration (spike path, ISA
   string, flags) is the one in §2. A test set that silently shrinks must be red, not green.
3. **Determinism compares streams, not counts.** P10 compares record counts on a sample; counts
   miss reordered or substituted records. The oracle costs well under a second for all 71
   binaries (measured: ~0.7 s for the whole set with logs, not the seven seconds the design
   assumes), so the acceptance run can compare full streams twice over the whole test set, and the
   implementer's local run can sample.
4. **Temp logs outside the repo.** Put the oracle logs under `target/` or a temp dir and clean up,
   so a failed run cannot leave 71 stale logs in the tree.
5. **Echo the pinned configuration in failure output.** Triage rule from the plan: a divergence is
   either a configuration mismatch or a bug. Without the config line in the report, the reader
   cannot tell which without a re-run.

## 4. Not objected to

- Black-box through the CLI, no internals: correct, and it is what makes the implementer's
  self-check safe.
- No cached golden logs: right call while the pinned configuration can still move.
- A missing oracle fails the test rather than skipping: exactly the failure mode a gate must not have.
- The 60 s per-binary timeout: a judgment call, generous but harmless.
