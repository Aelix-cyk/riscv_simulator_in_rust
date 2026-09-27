# S1 differential harness — design sketch (approval gate before code)

Owner: verifier. Zone: `tests/**`. Status: **approved 2026-09-27 and built** — the harness is
`tests/s1_differential.rs`, and the first red run is recorded in
`docs/verification/reports/001-s1.md`. The architect fixed two constraints on 2026-09-26
(`docs/notes/s1-plan.md`, work order 001): the harness must fail as an **assertion**, not a compile
error, and it must drive the simulator as a subprocess rather than through a library API that does
not exist yet.

Every byte quoted below comes from a real run of the pinned oracle on 2026-09-26, and every count
is measured.

## 1. Shape

    tests/s1_differential.rs        one #[ignore]d integration test per binary, 71 in total

      test set list (§1 of the baseline)
        │
        ├─> spike --isa=… --priv=msu --triggers=0 -l --log-commits --log=<tmp>  →  normalize (§5)
        └─> cargo run --release -- <elf> --trace                                →  already in form
        │
        └─> compare record by record ──> equal? next binary
                                    └─> no ──> assert!() with the first divergence

## 2. The differential step, in order

**P1 — Build once.** `cargo build --release` before the first comparison. Rebuilding inside a loop
that runs 71 times turns a two-second step into a slow one and muddies which binary was under test
when something failed.

**P2 — Enumerate the test set.** Glob `/home/aelix/tools/riscv-tests/isa/rv64ui-p-*` and
`rv64mi-p-*`, dropping the `.dump` companions (54 + 17 = 71 binaries; the raw glob returns 142
entries). The list is data, not code: a test set that shrinks silently is worse than a red test.

**P3 — Run the oracle.**

    /opt/riscv/bin/spike --isa=<pinned> --priv=msu --triggers=0 -l --log-commits \
        --log=<tmp>/<binary>.log <binary>

Fail the test if spike is missing or exits nonzero, and keep its exit status — comparing it is the
second half of the gate and is easy to forget.

**P4 — Read the `tohost` address** from the ELF symbol table before comparing: it is `0x80001000`
for 69 binaries and `0x80002000` for `rv64ui-p-ld_st` and `rv64ui-p-ma_data`. This is how the
differ knows which store ends the program.

**P5 — Normalize the reference** by the closed list in §5 of the baseline: keep commit and trap
records, drop the boot ROM, fold the trap continuation line, rewrite tokens, truncate after the
first nonzero store to `tohost`. Nothing else.

**P6 — Run the simulator.** `cargo run --release -- <elf> --trace`, capturing stdout and the exit
status, with a 60 s timeout per binary. A hung simulator must fail as a hang, not block the suite.

**P7 — Normalize ours — by asserting, not rewriting.** Our output should need none of §5's
rewrites; if it does, our format has drifted from §4. Assert that instead of rewriting, because a
rewrite that runs on both sides can cancel a real difference.

**P8 — Compare** record by record, in order. On the first difference, stop and report (§6).

**P9 — Compare exit status** using the table in §3 of the baseline.

**P10 — Determinism.** Re-run a sample and assert identical record counts, catching hash-order and
uninitialised-state flakiness that one pass cannot see.

## 3. Worked example — the first five records are the ROM, not the test

`rv64ui-p-add`, raw reference log, first 14 lines:

    core   0: 0x0000000000001000 (0x00000297) auipc   t0, 0x0
    core   0: 3 0x0000000000001000 (0x00000297) x5  0x0000000000001000
    core   0: 0x0000000000001004 (0x02028593) addi    a1, t0, 32
    core   0: 3 0x0000000000001004 (0x02028593) x11 0x0000000000001020
    core   0: 0x0000000000001008 (0xf1402573) csrr    a0, mhartid
    core   0: 3 0x0000000000001008 (0xf1402573) x10 0x0000000000000000
    core   0: 0x000000000000100c (0x0182b283) ld      t0, 24(t0)
    core   0: 3 0x000000000000100c (0x0182b283) x5  0x0000000080000000 mem 0x0000000000001018
    core   0: 0x0000000000001010 (0x00028067) jr      t0
    core   0: 3 0x0000000000001010 (0x00028067)
    core   0: >>>>  $xrv64i2p1_m2p0_a2p1_f2p2_d2p2_zicsr2p0_zifencei2p0_zmmul1p0_zaamo1p0_zalrsc1p0
    core   0: 0x0000000080000000 (0x0500006f) j       pc + 0x50
    core   0: 3 0x0000000080000000 (0x0500006f)
    core   0: 0x0000000080000050 (0x00000093) li      ra, 0

Rules 1, 2 and 4 of §5 reduce those 14 lines to a single record — line 13, the commit at the ELF
entry. Every following disassembly/commit pair then contributes exactly one more:

    0x0000000080000000 0x0500006f p3
    0x0000000080000050 0x00000093 p3 x1=0x0000000000000000
    0x0000000080000054 0x00000113 p3 x2=0x0000000000000000

What happened: ten ROM lines disappear (five disassembly lines with no commit record, five
records), the ISA banner disappears, and `x1  0x0` loses its alignment padding to become a field.
The simulator has no ROM, so its stream starts at the ELF entry — which is why §6 of the baseline
pins `x5`, `x10` and `x11` to the values the ROM leaves behind.

## 4. Worked example — a trap record, and one field the oracle will not give us

`rv64mi-p-illegal`, raw log lines 84–91:

    core   0: 3 0x00000000800000dc (0x01028293) x5  0x00000000800000e8
    core   0: 0x00000000800000e0 (0x30529073) csrw    mtvec, t0
    core   0: 3 0x00000000800000e0 (0x30529073) c773_mtvec 0x00000000800000e8
    core   0: 0x00000000800000e4 (0x74445073) csrwi   mnstatus, 8
    core   0: exception trap_illegal_instruction, epc 0x00000000800000e4
    core   0:           tval 0x0000000074445073
    core   0: 0x00000000800000e8 (0x00000297) auipc   t0, 0x0
    core   0: 3 0x00000000800000e8 (0x00000297) x5  0x00000000800000e8

Normalized, with rules 1, 3 and 4 applied:

    0x00000000800000dc 0x01028293 p3 x5=0x00000000800000e8
    0x00000000800000e0 0x30529073 p3 csr773_mtvec=0x00000000800000e8
    trap illegal_instruction epc=0x00000000800000e4 tval=0x0000000074445073
    0x00000000800000e8 0x00000297 p3 x5=0x00000000800000e8

Three things to notice. The `csrwi mnstatus, 8` at `0x…e4` has a disassembly line but **no commit
record**, because it trapped; its encoding survives only inside `tval`. `csrw mtvec, t0` keeps a
CSR field even though nothing read mtvec back. And the trap record above carries **no privilege
field**, which is a correction to §4 of the baseline:

Spike's exception line does not say which privilege the trap was taken from, and the neighbouring
records cannot recover it. The rule "use the previous record's privilege" is unsound, because
`mret`/`sret` changes the privilege for the *next* instruction while its own record carries the
privilege it *executed* at. Measured: three trap records in `rv64mi-p-illegal` are immediately
preceded by a trap-return commit — two by an `mret` executed in M-mode, one by an `sret` executed
in S-mode — so in those three the previous record's privilege is provably not the trap's. Deriving
it properly means
reimplementing trap-return privilege transitions inside the differ, which is the "the differ
becomes trusted code" risk ADR 006 already named.

**Proposed correction to §4:** drop `p<priv>` from the trap record, giving
`trap <name> epc=0x<value> [tval=0x<value>]`. The privilege at a trap is still exercised — through
the handler's own commit records, and through the privilege shown when execution returns.

## 5. Worked example — where the run stops

`rv64ui-p-add` is 5511 records long (5509 commits + 2 traps). The first store of a nonzero value to
`tohost` is record **516**:

    core   0: 0x0000000080000040 (0xfc3f2223) sw      gp, -60(t5)
    core   0: 3 0x0000000080000040 (0xfc3f2223) mem 0x0000000080001000 0x00000001

Everything from record 517 on — 4995 records of the same three instructions cycling until Spike's
device poll notices — disappears at rule 5. The reference trace the differ compares is therefore
511 records long (516 − 5 ROM records), and the expected simulator stream ends on that store.

`rv64mi-p-illegal` behaves the same way with different numbers: 5365 records, first halt store at
record 372, so 367 expected records.

## 6. Failure output, pinned

On the first divergence the test prints the binary, the record index, both records, and the three
records preceding them from both sides:

    rv64mi-p-illegal: first divergence at record 42 (of 367 expected)

      ours   record 42: 0x00000000800000e0 0x30529073 p3 csr773_mtvec=0x00000000800000e8
      spike  record 42: 0x00000000800000e0 0x30529073 p3

      preceding (ours)                          preceding (spike)
      39  0x…00d4 0x…  p3 x5=0x…               39  0x…00d4 0x…  p3 x5=0x…
      40  0x…00d8 0x…  p3                      40  0x…00d8 0x…  p3
      41  0x…00dc 0x…  p3 x5=0x…               41  0x…00dc 0x…  p3 x5=0x…

That is enough to tell "we log a CSR write and Spike does not" from "we computed the CSR value
wrongly", without a re-run.

## 7. Decisions inside the sketch

**Black box through the CLI.** The harness sees the binary, its stdout and its exit code, never
`src/**`. That is what makes the implementer's self-check safe: the harness cannot be written
against internals it is supposed to judge independently. The cost is coarser failure messages —
"record 42 differs", not "`decode` returned `None`" — so unit tests inside `src/**` stay the
implementer's tool for localizing.

**No cached golden logs.** Spike runs the whole test set in 0.8 s (measured three times in a row:
0.78, 0.77, 0.77 s; the first, cold run of a session cost 7.1 s, which is affordable too) and
writes 49 MB of logs doing it. Caching would buy nothing and would commit files that go stale the
moment the pinned configuration changes.

**A missing or failing oracle fails the test.** No skip-if-absent: a gate that excuses itself when
the environment is wrong reports green on a machine that verified nothing.

## 8. Not in this harness

- Decoder and CSR unit tests: the implementer's, in `src/**`, allowed to see internals.
- Timing or performance assertions: a slow simulator is not incorrect, and timing flakes on shared
  machines.
- `riscv-arch-test` coverpoints: §7 of the baseline already records that the test set is a floor.

## 9. What approval authorizes

Writing `tests/s1_differential.rs` and running it against the stub `src/main.rs`, where it must fail
on all 71 binaries as assertions. That red run is the deliverable and the remaining start condition
for M1. The §4 correction described above is a change to the baselined surface and needs the
architect's sign-off separately.

## 10. Additions from the review of 2026-09-26 — accepted

`docs/notes/s1-harness-review.md` §3 listed five additions. All five are accepted and folded in
here; none change the shape of §1.

1. **Test the normalizer itself.** ADR 006 makes the differ trusted code, so each rewrite rule gets
   a fixture: the two-line trap form, a line with a CSR annotation, a line with both a register
   write and a `mem`, the boot-ROM prefix, and a log with no halt store. Fixtures are copied
   verbatim from a real log and record which binary and line range they came from, so a fixture
   that encodes a misreading stays traceable to its source.
2. **Guard against vacuous green.** Assert the glob yields exactly 71 binaries, that both sides
   produced a non-empty stream, and that the pinned configuration matches §2 of the baseline. The
   expected count is a named constant that changes only with a baseline change, so a shrinking test
   set turns red instead of quiet.
3. **Determinism compares streams, not counts.** Two full passes over the whole test set at
   acceptance, not a sampled count comparison — see the corrected timing above, which makes this
   affordable. Sampling stays available for the implementer's local loop.
4. **Temp logs outside the repo,** under `target/` (already ignored) or a temp dir, cleaned up
   afterwards. One full run is 49 MB; a failed run must not leave 71 stale logs in the tree.
5. **Echo the pinned configuration in failure output,** so the triage rule — configuration
   mismatch or simulator bug — can be applied without a re-run.

One addition of my own, from the measurement rather than the review: rule 5 of §5 gets a fixture
for the case that actually occurs — a reference whose records **continue past** the halt store
(measured: the log ends on the store in 56 binaries, one commit later in 14, three commits later in
`rv64mi-p-illegal`). The review's fixture list covers a log with *no* halt store, which never
happens in this test set, and misses this one, which happens in 15 of 71.
