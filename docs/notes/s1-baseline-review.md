# Review of docs/verification/s1-surface.md

Reviewer: assistant · Date: 2026-09-26 · Verdict: **approve with two wording corrections**
One scope consequence is flagged for the architect in §3.

## 1. Claims independently re-measured — all reproduce

| Claim in the baseline | Re-measured |
|---|---|
| 71 binaries, 54 `rv64ui-p` + 17 `rv64mi-p` | 71 ✓ |
| Test-set log totals: 836,465 lines; 381,827 disassembly; 72,727 symbol | identical ✓ (also 381,673 commit, 155 trap records) |
| Spike spins in `write_tohost` after the halt store | 1000 `write_tohost` symbol hits and 1000 tohost stores in `rv64ui-p-add` ✓ |
| `tohost` is `0x80001000` for 69 binaries, `0x80002000` for two | 69 / 2 ✓ (`ld_st`, `ma_data`); hardcoding passes 69/71 |
| CSR writes logged per write, not per change | `csrs mtvec, t0` with t0=0 still logs `c773_mtvec 0x80000004` ✓ |
| `mret` logs an implicit `csr768_mstatus` write | ✓ |
| CSR annotation keyed by backing CSR (`sstatus` → `csr768_mstatus`) | ✓ |
| Trap inventory `illegal_instruction` 81, `user_ecall` 56, `machine_ecall` 14, `supervisor_ecall` 1, `instruction_address_misaligned` 1, `breakpoint` 1, `interrupt#1` 1 | identical ✓ |
| `pmpaddr` mask, `misa`/`marchid` values, `mip = 0x82`, `mstatus` `0xA00000000` | ✓ (independently confirmed earlier) |

## 2. Corrections required before approval

1. **§8 “Implicit writes are logged” is ambiguous and will cause divergence.** Only `mret` logs an
   implicit `csr768_mstatus` write; **trap entry logs nothing** — no `mepc`, `mcause` or `mstatus`
   annotation appears around the exception lines. An implementer who reads §8 literally emits CSR
   records on trap entry and diverges on all 155 trap records. State both halves explicitly.
2. **§5.5's justification is inaccurate, though its rule is right.** Spike does not “break off at a
   different instruction”: its log ends at the *same* store instruction, on roughly the 1000th
   iteration of the `write_tohost` loop. Fix the sentence so nobody later “corrects” the truncation
   rule to match it.

Also worth adding to §8, since it is a trap detail the implementer will otherwise guess:
`ebreak` sets `tval = epc` (measured `0x800001a4`), unlike `illegal_instruction`, whose `tval` is
the encoding. **Declined by the architect on 2026-09-26 — deliberately left unapplied; do not
re-raise it as a defect.** The fact stays verifiable from `rv64mi-p-sbreak`.

## 3. Scope consequence for the architect

§8 is correct that S-mode is in S1, and this corrects an earlier verbal summary of mine that said
the test-set runs in U-mode. `rv64mi-p-illegal` executes 30 commits at `priv = 1`, uses `sret` three
times, writes `sepc`/`scounteren`/`sstatus`, executes `sfence.vma` and `wfi` in S-mode, and takes
one `supervisor_ecall`. That is real M3/M4 work: S-mode CSR views, `sret`, and traps taken from
S-mode. The plan note should say M/U/**S** modes for S1 rather than M/U.

## 4. On the two decisions in §9

- **F1 (keep CSR writes in the trace): endorse.** It converts WARL masking, `mret`'s implicit
  write and the U/S views of `mstatus` from unverifiable into ordinary diffs. The price is real:
  per-write logging with post-WARL values, which is M3 work, and the baseline already says so.
- **F2 (own compact record form): endorse.** Trading a small, testable rewrite table for a
  trace a human can read is the right side of that trade.

## 5. Not re-measured

The per-binary distribution “999 iterations in 15 binaries, 1000 in the other 56” — one binary was
confirmed at 1000; the split across the rest was not recounted.
