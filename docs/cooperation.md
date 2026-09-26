# Cooperation Mechanism (v0)

One mechanism, three parts: an artifact-mediated pull queue, an interface freeze, and a
test-first baton. Nothing travels between agents except files in this repo.

## 1. Why files, not chat

Agents lose context, run in parallel, and share one filesystem. Chat is unlogged and vanishes;
artifacts survive compaction, can be diffed, and let a fifth agent join later. The cost is one
round trip per cycle — acceptable for a simulator whose gates are retirement-level comparisons
against the oracle.

## 2. The cycle

1. **Architect** sets intent; **assistant** writes `docs/work-orders/NNN-<slug>.md` with goal,
   owned files, acceptance command, and the frozen interfaces it depends on.
2. **Verifier** proposes the observable surface for that unit (signatures, CLI, trace format) in
   `docs/verification/`; the **architect** approves it; the verifier then lands a failing
   Spike-diff harness. The implementer does not start until approval lands (ADR 001).
3. **Implementer** takes the baton, edits only owned files, and iterates until the harness
   passes locally.
4. **Verifier** re-runs independently, writes `docs/verification/reports/NNN-<slug>.md`, and
   either accepts or returns the first divergence with evidence.
5. **Assistant** updates status, drafts any ADR, and asks the architect for the next decision.

Steps 2 and 3 can overlap across *different* work orders — the verifier may build the next
harness while the implementer finishes the current one. Never within the same work order.

## 3. Work order anatomy

```
ID / slug / status: proposed | frozen (architect-approved surface) | implementing | verifying | accepted | blocked
Goal: one sentence, observable outcome
Owned files: explicit list (one writer per file, always)
Depends on: frozen interfaces and accepted ADRs
Acceptance: exact command(s) + what green means
Evidence: filled by implementer and verifier, not by the author of the goal
```

## 4. Rules that keep it efficient

- **One writer per file.** The ownership map is the lock; the shared filesystem has no locking.
- **No agent-to-agent negotiation.** Disagreement becomes a line in the report and an item for
  the assistant.
- **Interface changes go through the verifier;** pass criteria live in `docs/verification/`, and
  changes to them need the architect's sign-off.
- **Zones, not trust.** Implementer writes `docs/implementation/`, verifier writes
  `docs/verification/`. Anything outside your zone needs a one-line permission request, recorded
  in the work order and expiring with it.
- **Baton, not parallel edits.** The baton is visible as the work-order status field.
- **Batch by module.** One cycle covers a coherent unit (decode, ALU, memory, trap), not one method.
  Fewer, larger cycles beat many tiny round trips.
- **Escalate after two failures.** Third attempt is a signal the interface or the work order is wrong.

## 5. Definition of done

Per the criteria frozen in `docs/verification/`: trace-level equivalence with Spike first, exit
status second, whole `rv64ui-p-*` corpus before trusting either. A work order is accepted only
when the verifier has run the acceptance command on the merged state and recorded the output.

## 6. Failure modes this is designed against

| Failure | Mechanism that catches it |
|---|---|
| Implementer and verifier invent different trace formats | Interface freeze, step 2 before step 3 |
| Both "pass" because the test was weakened | Pass criteria owned and recorded by the verifier; architect approves changes |
| Two agents edit one file | Ownership map, one writer per file |
| Silent scope creep | Work order goal is one sentence and observable |
| Context loss mid-task | Everything needed to resume is in the work order and report |

## 7. Alternatives, and when they win

- **Free-form chat delegation** — faster while the design is genuinely unknown; loses the audit
  trail and breaks down past two agents. Use for spikes only, then write the finding into a work order.
- **Fully synchronous review of every commit** — highest confidence, lowest throughput; worth it
  for the trap/CSR boundary, wasteful for an ALU opcode.
