# RVA22S64 scope and slices

Companion to ADR 005 (which supersedes ADR 002). One slice = one architect-approved freeze, then
one implement, then one verify cycle.

## Slices, in build order

| # | Slice | Corpus on disk | Why here |
|---|---|---|---|
| S1 | RV64I + Zicsr + Zifencei + Zicntr, M/U modes and traps | `rv64ui-p-*` (108), `rv64mi-p-*` (34) | Everything else assumes correct decode, CSRs, and traps |
| S2 | M | `rv64um-p-*` (26) | Self-contained, cheap, unblocks a real compiler target |
| S3 | Zaamo + Zalrsc | `rv64ua-p-*` (44) | Needs a reservation set, and Za64rs bounds it to 64 bytes |
| S4 | Zba + Zbb + Zbs | `rv64uzba` (16), `rv64uzbb` (48), `rv64uzbs` (16) | Self-contained bit manipulation |
| S5 | Zca + Zcd (compressed) | `rv64uc-p-*` (2) — thin | 16-bit decode changes the fetch loop |
| S6 | F + Zfhmin + ZicsrF | `rv64uf` (22), `rv64uzfh` (22) | FLEN=64, NaN boxing, rounding modes, `fflags` |
| S7 | D | `rv64ud-p-*` (24) | Same machinery; vendored softfloat (ADR 004) |
| S8 | S-mode: Sv39, Svbare, Svade, Svpbmt, Svinval, Sstvecd, Sstvala, PMP, interrupts, Zicclsm, Zicbom/Zicboz/Zicbop | `rv64si-p-*` (14), `rv64mzicbo-p-*` | The real cost; A/D bits, `satp` flushes, fetch faults |
| S9 | Boot path: M-mode firmware, DTB, hand-off to S-mode | none — OpenSBI source now present | "OS booting" lands here, not before |

v0 remains **S1–S2** per ADR 003.

## Pinned oracle invocation (executed successfully 2026-09-24)

    /opt/riscv/bin/spike \
      --isa=rv64imafdc_zicsr_zifencei_zicntr_zihpm_zicclsm_zihintpause_zicbom_zicboz_zicbop_zca_zcd_zba_zbb_zbs_zfhmin_zaamo_zalrsc_svade_svpbmt_svinval \
      --priv=msu -l --log-commits --log=<ref>.log <binary>

Verified on `rv64ui-p-add`: exit 0, 12064 commit-log lines, first line
`core   0: 0x0000000000001000 (0x00000297) auipc t0, 0x0`.
Spike's own default is `rv64imafdc_zicntr_zihpm`; the pinned string exists to stop silent drift.

Oracle identity, which every verification report must record alongside the ISA string:
`Spike RISC-V ISA Simulator 1.1.1-dev`, source tree `riscv-isa-sim.git` at commit `8fc5ab03`.

### Extensions Spike rejects from the ISA string

`za64rs`, `zic64b`, `sstvecd`, `sstvala`, `ssu64xl` are rejected as unsupported names. The
certification plan marks them mandatory, so they are *behavioural* mandates with no encoding and
no Spike switch: plain trace diffing cannot detect a violation. Each needs a hand-written test
(`Sstvala` = `stval` always written on trap, `Sstvecd` = `stvec` base/alignment behaviour,
`Za64rs`/`Zic64b` = 64-byte reservation and cache-block sizes, `Ssu64xl` = user XLEN 64).

## What RVA22S64 adds over RVA20S64

Zba/Zbb/Zbs, Zicbom/Zicboz/Zicbop cache-block operations, Zicclsm (misaligned accesses to main
memory must work, not trap), Za64rs/Zic64b size constraints, Zfhmin half-precision, and the
Svade/Svpbmt/Svinval/Sstvecd/Sstvala behavioural set. The last group is the easiest to miss and
the hardest to prove.

## Local toolchain inventory (verified 2026-09-24)

| Path | What it gives us |
|---|---|
| `/home/aelix/tools/riscv-tests` | Per-slice `-p-` corpora with `.dump` files — the working oracle |
| `/home/aelix/tools/riscv-arch-test` | Certification test plans, coverpoints, Spike/SAIL/Imperas configs |
| `/home/aelix/tools/opensbi` | M-mode firmware source for the S9 boot path |
| `/home/aelix/tools/riscv-gnu-toolchain` | Compiler and binutils |

## Verification strategy

- Differential against Spike, trace-exact, per slice; a slice is green only on its whole corpus.
- Conformance evidence comes from `riscv-arch-test` coverpoints for the RVA22S64 profile columns;
  the profile's own CRD is still not on this machine, so the extension list above is quoted from
  the coverage matrix, not from the ratified profile document.
- Determinism rules apply: no host time, no allocation addresses, fixed counter model.

## Product note

Booting Linux also needs machine-mode firmware providing SBI calls. OpenSBI source is now on
disk, but cross-compiling it and choosing a platform port is unestimated work belonging to S9.
