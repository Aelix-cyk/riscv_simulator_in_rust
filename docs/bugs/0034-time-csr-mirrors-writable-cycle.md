# Bug 0034: `time` CSR mirrors writable cycle state

| Field | Value |
|---|---|
| Found | 2026-11-02, PMU-enabled Linux boot diagnosis |
| Affected | `src/csr.rs`, `src/machine.rs` |
| Status | Fixed |
| Fixed | 2026-11-02 |

## Summary

The simulator returned the writable `mcycle` value for both the `cycle` and
`time` CSRs. OpenSBI's PMU implementation writes hardware counter CSRs while
Linux registers perf events, including writes through `mcycle`. Those writes
could move `time` backward while CLINT `mtime` continued forward.

Linux then programmed an SBI timer from the stale `time` value. The requested
deadline was already in the past relative to `mtime`, causing a continuous
machine-timer interrupt storm.

## Specification requirement

Zicntr defines `time` as a read-only real-time counter and `mcycle` as a
writable cycle counter. They share a timebase but are not the same architectural
counter: resetting or preloading `mcycle` must not rewind real time.

## Reproduction

1. Enable `CONFIG_PERF_EVENTS` and `CONFIG_RISCV_PMU_SBI` in the pinned Linux
   fixture.
2. Boot through OpenSBI and allow Linux to initialize `riscv-pmu-sbi`.
3. Observe repeated machine timer traps, with Linux restarting the same
   `strlen` loop after only a few instructions.

Before the fix, a 220-million-step diagnostic recorded 396,477 timer traps at
the same `strlen` instruction and no further console output after PMU
initialization.

## Impact

Linux cannot make progress when PMU initialization writes a hardware counter.
The interrupt storm also prevents userspace from starting.

## Fix

`Csrs` now has a separate monotonic `time` counter.

- `cycle` still aliases `mcycle`.
- `time` is read-only and no longer follows writes to `mcycle`.
- Every platform cycle advances both `time` and CLINT `mtime`.
- A write to `mcycle` still advances platform time without incrementing the
  newly written cycle value.

The Linux fixture now enables `CONFIG_PERF_EVENTS` and
`CONFIG_RISCV_PMU_SBI`.

## Regression tests

- `csr::tests::time_is_independent_of_writable_cycle_state`
- `machine::tests::counter_write_and_trap_advance_cycles_once`
- `linux_boot::linux_reaches_an_smode_userspace_init` requires Linux SBI PMU
  initialization before `/init`.

## Verification

The PMU-enabled Linux gate reaches `/init`, prints `ARVSR-LINUX-OK`, and powers
off cleanly.
