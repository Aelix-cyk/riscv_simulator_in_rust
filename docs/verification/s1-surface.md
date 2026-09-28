# S1 surface — baseline v3

**Status:** baselined v3 (v1 approved 2026-09-26; change notes 1 and 2 approved 2026-09-27;
ADR 001, ADR 006)
**Owner:** verifier · **Date:** 2026-09-26, change notes added 2026-09-27 · **Baseline:** v3 — a
change after this point gets a new numbered note below and raises the version to v4.
**Confidence:** high on the line inventory and normalization (measured), and high on both §9
decisions now that they are taken

## Change notes

**1 — 2026-09-27: the trap record loses `p<priv>` (v1 → v2).** Spike's exception line does not
carry the privilege the trap was taken from, and the neighbouring records cannot recover it:
`mret`/`sret` changes the privilege for the *next* instruction while its own record shows the mode
it *executed* in. Measured in `rv64mi-p-illegal`: three trap records are immediately preceded by a
trap-return commit — two `mret` executed in M-mode, one `sret` executed in S-mode — so in those
three the previous record's privilege is provably not the trap's. Recovering the value would mean
reimplementing trap-return privilege semantics inside the differ, the risk ADR 006 already names.
Section §4 is the only text that changes; the privilege at a trap stays exercised through the
handler's commit records and through the mode seen when execution returns. Evidence and worked
example: `docs/verification/s1-harness-design.md` §4.

**2 — 2026-09-27: §5 rule 4 also covers the trap name (v2 → v3).** §4's trap names are
`illegal_instruction` and `interrupt#<code>`, but the oracle prints `trap_illegal_instruction` and
`interrupt #1`. Rule 4 named the privilege, CSR and `mem` rewrites and not this one, which left the
normalizer's mapping an unlisted rewrite. §4 — the observable surface — is unchanged; §5.4 now
states the mapping. Found while writing the harness, when real log lines could not reach §4's
grammar without it: `tests/s1_differential.rs`, fixture `FIXTURE_TRAP`.

Everything below is derived from running the pinned oracle over the whole S1 test set on
2026-09-26: 71/71 binaries exit 0, producing 836,465 log lines. Counts quoted here are measured,
not assumed. The raw measurement is reproducible with the command in §2.

---

## 1. Test set and the gate

71 binaries from `$RISCV_TESTS_DIR/isa`: 54 `rv64ui-p-*`, 17 `rv64mi-p-*`.

A binary passes when, after the normalization in §5, the simulator's record sequence equals
Spike's and its exit status equals Spike's. S1 is accepted when all 71 pass twice in a row, run as
the separate step of §3.1 by the verifier, with the implementer's `cargo test` green.

Trace equality is necessary, not sufficient: §7 lists what this gate cannot see.

## 2. Oracle, pinned

    spike \
      --isa=rv64imafdc_zicsr_zifencei_zicntr_zihpm_zicclsm_zihintpause_zicbom_zicboz_zicbop_zca_zcd_zba_zbb_zbs_zfhmin_zaamo_zalrsc_svade_svpbmt_svinval \
      --priv=msu --triggers=0 -l --log-commits --log=<ref>.log <binary>

Spike 1.1.1-dev, `riscv-isa-sim.git` @ `8fc5ab03`. Measured: exit 0 on all 71 binaries.
Changing any flag here invalidates this baseline; the `zicclsm` precedent (misaligned data access
commits with it, traps without it) is exactly why the string is written out in full rather than
left at Spike's default.

## 3. Simulator surface

    cargo run --release -- <elf> --trace      # normalized records on stdout
    cargo test                                # unit tests only: no Spike, no test set

Exit status is the riscv-tests verdict, taken from the value stored to `tohost`:

| Halting store value | Meaning | Exit |
|---|---|---|
| `1` | pass | `0` |
| `(n << 1) \| 1`, `n > 0` | test `n` failed | `n` |
| — | ELF could not be loaded | `2` |

`--trace` writes only the records of §4, one per line, in commit order. Everything the simulator
prints is deterministic: no timestamps, no host addresses, no map iteration order.

### 3.1 The differential step

    cargo test --test s1_differential -- --ignored --test-threads=1

The step is a tool for both roles and a verdict for exactly one (architect, 2026-09-26):

- **Implementer:** may run it locally at any time as a self-check. It judges only the baselined
  surface below, so running it cannot weaken it; a green local run is evidence for the work order,
  never acceptance, and it changes nothing about the git rule.
- **Verifier:** acceptance is the verifier's independent run on the merged state, recorded twice
  with any first divergence in `docs/verification/reports/001-s1.md`.
- It stays `#[ignore]`d, so `cargo test` never picks it up: a green unit suite cannot be mistaken
  for acceptance, and the oracle run stays out of the fast edit loop.
- The step owns the oracle invocation (§2), the normalization (§5), the test-set list, and the
  first-divergence report. Nothing about how it judges lives in `src/**`.

## 4. Records

Two record kinds. Angle brackets are placeholders; literal text is literal. Hex digits are
lowercase; `pc`/`value`/`addr` are 16 digits, `insn` is 8.

### Commit record — one per retired instruction

    0x<pc> 0x<insn> p<priv> [x<n>=0x<value>] [csr<num>_<name>=0x<value>] [mem 0x<addr>[=0x<value>]]

    priv      0 = U, 1 = S, 3 = M
    x<n>      at most one per record; never emitted for n = 0
    csr<num>_<name>  one per CSR write, including writes that change nothing
    mem       load:  `mem 0x<addr>` (address only)
              store: `mem 0x<addr>=0x<value>`

Field order is register write, then CSR write, then memory; the order of CSR and memory is
unconstrained in S1 because no S1 record has both (measured: 0 occurrences). A record always
carries `pc`, `insn` and `p<priv>`; the optional fields may all be absent (e.g. a branch).

### Trap record — one per taken trap, replacing that instruction's commit record

    trap <name> epc=0x<value> [tval=0x<value>]

    name   illegal_instruction | instruction_address_misaligned | breakpoint
           | user_ecall | supervisor_ecall | machine_ecall | interrupt#<code>
    tval   present for illegal_instruction, instruction_address_misaligned, breakpoint;
           absent for the three ecall traps and for interrupts

The trapping instruction gets a trap record and no commit record.

The trap record carries no privilege field: see change note 1. The privilege is observable in the
commit records that follow the trap, not in the trap record itself.

## 5. Normalization — the complete list

Applied to the reference log only; the simulator's output already has this shape. Nothing else is
permitted. Widening this list to hide a divergence is not an option; a change here needs the
architect's sign-off.

1. **Keep** only commit records and trap records. Discard Spike's disassembly lines (`381,827`),
   its symbol lines (`72,727`), and the ISA banner.
2. **Drop the boot ROM.** Spike's log opens with 5 committed instructions at PCs
   `0x1000…0x1010` (355 records over the test set) that jump to the ELF entry. The simulator has no
   ROM; §6 pins the register state the ROM leaves behind.
3. **Fold** the trap continuation line (`core 0:           tval 0x…`) into the trap record.
4. **Token rewriting:** Spike's privilege field `<priv>` → `p<priv>`; its trap name `trap_<name>` →
   `<name>`, and `interrupt #<code>` → `interrupt#<code>`; its CSR annotation
   `c<num>_<name> 0x<value>` → `csr<num>_<name>=0x<value>`; its load annotation `mem 0x<addr>` and
   store annotation `mem 0x<addr> 0x<value>` → the `mem` forms in §4.
5. **Truncate the reference** immediately after the **first** commit that stores a nonzero value to
   `tohost`. Every iteration of Spike's `write_tohost` loop stores a nonzero value, and Spike
   notices the halt only on its next device poll, so its log runs roughly 999 further iterations
   (measured: 999 in 15 binaries, 1000 in the other 56) before stopping. Where that log *ends* is
   also not the halt store: measured, the last record is the store itself in 56 binaries, one
   commit later in 14 (`auipc t5, 0x1`), and three commits later in `rv64mi-p-illegal`. The
   simulator halts on the first such store; the differ truncates the reference there and ignores
   everything after it.

## 6. Initial state at the ELF entry point

The simulator starts at `e_entry` (0x80000000 in this test set) with `priv = M` and the register
file the ROM left: `x0 = 0`, `x5 = 0x80000000`, `x10 = 0` (mhartid), `x11 = 0x1020` (dtb), all
other GPRs 0. No binary in the test set reads the ROM, the dtb, or memory below `0x80000000` after entry —
measured: every post-entry memory access in all 71 logs lands in `0x80000000…0x80003041`.

Reset CSR values the test set actually reads: `misa = 0x800000000014112f`, `marchid = 5`,
`mvendorid = mimpid = mhartid = 0`, and `mstatus` with every writable field zero and UXL/SXL = 2
(`0x0000000a00000000`). `mtvec` and `satp` reset values are *not* observable in S1: the test
prologue writes both (`0x800000e4`/`0x800000e8` and `0`) before anything reads them.

`tohost` is read from the ELF symbol table, never hardcoded: it is `0x80001000` for 69 binaries
and `0x80002000` for `rv64ui-p-ld_st` and `rv64ui-p-ma_data`. Hardcoding it passes 69/71.

## 7. What this gate does not cover

- CSR values that no S1 binary writes and reads back. §4's CSR field narrows this a lot — 19
  distinct CSRs are written across the test set, including `mstatus` 193 times — but `riscv-arch-test`
  coverpoints are what eventually close the gap, not this test set.
- Counters: no S1 binary reads `cycle`/`time`/`instret` into a non-`x0` register.
- Memory contents after the halt, and any state the halt store leaves behind.
- The failed-test path. All 71 binaries pass, so `exit = n` for `n > 1` is specified here but
  unexercised by the test set; it is covered by a unit test, not by the gate.
- Anything S1's test set does not reach: F/D arithmetic, virtual memory beyond `satp`/`sfence`
  being written, multi-hart, devices.

## 8. Facts the implementer will otherwise get wrong

- The CSR field is keyed by the **backing** CSR, not the mnemonic written. `csrs sstatus, x` logs
  `csr768_mstatus=…`; `sstatus` is a view of `mstatus`.
- Implicit CSR writes are logged **only** for `mret`, which produces `csr768_mstatus=…`. Trap entry
  logs nothing: no `mepc`, `mcause` or `mstatus` record accompanies an exception or interrupt line.
  Emitting CSR records on trap entry diverges on all 155 trap records in the test set.
- CSR writes are logged even when the value does not change — measured: `csrs mtvec, zero` logs
  `csr773_mtvec` with mtvec's existing value. This is a per-write log, not a per-change log.
- The logged CSR value is the **post-WARL stored** value: `csrwi mstatus, 0` logs
  `0x0000000a00000000`, with the read-only UXL/SXL fields forced to 2.
- `mstatus` has no MXL field in RV64 (SD occupies bit 63); `misa` carries MXL at bits 63:62.
- Misaligned **data** access commits; misaligned **fetch** traps
  (`trap_instruction_address_misaligned`). That asymmetry is the `zicclsm` flag's whole effect.
- S-mode is in S1: binaries execute at `priv = 1` and write `sstatus`, `sepc` and
  `scounteren`, so S-mode CSR views and `mret` into S-mode must work.
- Only one interrupt is taken across the test set: `interrupt#1` in `rv64mi-p-illegal`, raised by
  the test arming SSIP in `mie`/`mip`.

## 9. Decisions, taken by the architect on 2026-09-26

Both were approved with this baseline; the rationale and the rejected alternatives are recorded in
ADR 006, and this section only states the outcome.

**F1 — CSR writes stay on the trace line.** §4's `csr<num>_<name>=0x<value>` field is part of the
baselined surface, which supersedes the draft in `docs/notes/s1-plan.md` that stripped Spike's
annotations. Consequence: M3 must log CSR writes as it goes, not defer them to M5.

**F2 — compact rendering; §5 normalizes the reference.** The simulator emits §4's records; §5's
rewrite rules are applied to Spike's log only. Consequence: the differ owns a small rewrite table,
which is itself test code and is tested as such.

**F3 — nothing else.** The method order in work order 001 (M1…M5) is unaffected by this baseline.

This file is baselined. Work order 001 is `baselined`; the implementer starts M1 against this surface.
The verifier's next deliverable is the failing Spike-diff harness, which is what makes M1's "done"
checkable.
