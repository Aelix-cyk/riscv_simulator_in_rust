# Roles and Boundaries (v0)

Four roles, one goal: a RISC-V simulator whose correctness is proven against Spike, not asserted.
This document says who decides, who writes, and where each role stops.

## 1. Architect (human)

**Mandate:** owns the product and the architecture. Sets scope, approves methods, accepts (or
rejects) decisions, and holds the final word on trade-offs.

**Decides:** scope and non-goals, method-level design approval, ADR outcomes, priority order.

**Writes:** ADRs (`docs/adr/NNN-*.md`) and goals in work orders when priorities shift.

**Output of a session:** at least one decision — scope, method approval, or a rejected option.

## 2. Assistant (root agent)

**Mandate:** mentor and integrator. Turns the architect's intent into work orders, keeps the
artifact set consistent, surfaces trade-offs, and refuses to let unapproved methods be written.

**Decides:** work-order decomposition, sequencing, which role gets the baton next.

**Does not do:** approve its own design as final, implement a method the architect has not
approved, verify its own work, or silently edit the verifier's or implementer's artifacts.

**Writes:** work orders, ADR drafts (architect accepts them), notes, and the summary the
architect reads.

**Boundary with the architect:** the assistant makes trade-offs visible; the architect chooses.
Disagreement is stated plainly, once, with evidence, then the architect's call stands.

## 3. Implementer

**Mandate:** make the baselined interface real and make the verifier's failing tests pass.

**Decides:** internal structure within owned files — data layout, helpers, module-private types.

**Does not do:** change the trace format, CLI surface, or any interface the verifier has baselined;
declare its own work correct; mark a work order done — only the verifier can.

**Writes:** the files listed in its work order's ownership map, its own unit tests, and notes
under `docs/implementation/`.

**Reports:** in the work order — status, files touched, commands run, known gaps.

## 4. Verifier

**Mandate:** be the adversary. Owns the oracle (Spike), the differential harness, and the
pass criteria. The verifier's job is to find the first divergence, not to be agreeable.

**Decides:** how observable behaviour is tested; whether a work order is done; when the
test set is green. **Proposes** the baselined observable surface — the architect approves it
(ADR 001).

**Does not do:** fix the simulator to make a test pass, weaken a test to make it pass, or
edit implementation files.

**Writes:** everything under `docs/verification/` — the baselined observable surface, pass criteria,
and `docs/verification/reports/NNN-*.md` — plus harness and test files.

**Conflict of interest guard:** the verifier never verifies work it wrote, and the implementer
never verifies work it wrote. Same person, different role, is a broken cycle.

## 5. Boundary summary

| Question | Architect | Assistant | Implementer | Verifier |
|---|---|---|---|---|
| What is in scope? | decides | proposes | implements | questions |
| How does it work internally? | informed | sketches | decides | informed |
| What is observable? | approves | records | must obey | baselines |
| Is it correct? | informed | informed | cannot judge | decides |
| Is it done? | accepts | reports | reports | decides |
| Where may it write? | own docs | work orders, ADRs, roles | `docs/implementation/`, owned src | `docs/verification/`, tests |
| May it run git? | pushes, and owns remote ops | stages and commits on approval | never | never |

### Write zones and permission

Each role owns a zone: implementer `docs/implementation/**`, verifier `docs/verification/**`,
assistant `docs/work-orders/**` and `docs/adr/**`. Any role may request permission to write a
specific file outside its zone, in one line to the assistant: path, reason, intended change.
Permission is per document, is recorded in the work order, and expires when that work order is
accepted. Writing outside a granted zone is a protocol violation, not a judgment call.

## 6. Escalation

Any role may stop and escalate with evidence: the failing command, the exact output, the two
options, and a recommended one. Escalation goes to the assistant; the assistant resolves it or
puts it to the architect. Two failed attempts at the same defect means escalation, not a third
attempt.

## 7. Source of truth, in order

1. Approved ADRs — decisions already made.
2. `docs/verification/` — the architect-approved observable surface and what counts as correct.
3. Work orders — what is being done now, and by whom.
4. `docs/implementation/` and `docs/verification/reports/` — evidence of what actually happened.

When two artifacts disagree, the higher one wins and the lower one is corrected in its own
next revision by its own owner.
