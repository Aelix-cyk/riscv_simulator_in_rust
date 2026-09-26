# Work order 001 — verifier review

Date: 2026-09-26 · Reviewer: verifier · Result: **not blocked, but not startable yet**

Reviewed `docs/work-orders/001-s1-base.md` against the repo and the oracle. Three findings.

## 1. Corpus claims are accurate — verified

Counting the built binaries in `/home/aelix/tools/riscv-tests/isa`:

| Pattern | Count claimed | Count observed |
|---|---|---|
| `rv64ui-p-*` | 54 | 54 |
| `rv64mi-p-*` | 17 | 17 |
| total (S1 gate) | 71 | 71 |

## 2. Pinned oracle configuration is reproducible — verified

Running the exact invocation from `README.md` on `rv64ui-p-add` exits 0 and writes 12064 log
lines, matching the figure recorded when it was pinned. The full 71-binary S1 corpus was then
run under that same configuration:

    PASS=71 FAIL=0

So the goal is achievable and the gate is not vacuous. Spike resolves every S1 binary; nothing
in the corpus needs an extension outside the pinned ISA string.

## 3. The gate is not yet open — the freeze is the missing artifact

Work order 001 depends on `docs/verification/s1-freeze.md`, which does not exist. The work order
is therefore correctly self-described as `proposed`, and the implementer must not start.
The baton is with the verifier, not the implementer.

## Scope flag for the architect

The 17 `rv64mi-p-*` binaries are not I-extension work. They cover breakpoints (`-breakpoint`,
`-sbreak`), illegal instruction trapping (`-illegal`), misaligned load/store traps
(`-ld/-lh/-lw/-sd/-sh/-sw-misaligned`), PMP CSRs (`-pmpaddr`), `mstatus`/`mcsr` behaviour, and
the `zicntr` time counter (`-zicntr`), plus `-scall`/`-ma_fetch`/`-instret_overflow`.

That is a defensible S1 scope — it is the smallest corpus that forces M3 (hart, CSR, trap entry,
`mret`, U-mode) to exist — but it means S1 already requires a working privileged trap path, not
just RV64I semantics. If S1 is meant to be the *small* slice that proves the differential harness
end to end, 54 `rv64ui-p-*` alone would do that, and the 17 `rv64mi-p-*` binaries could be S2.

## What the verifier needs from the architect

1. Whether S1's acceptance corpus stays at 71 or drops to the 54 `rv64ui-p-*` binaries.
2. Approval of the drafted `docs/verification/s1-freeze.md` once it lands (ADR 001 requires the
   architect's sign-off before the work order becomes `frozen`).

Confidence: high on findings 1-2 (directly measured). Medium on the scope flag — it is a
judgment about slice sizing, not a measurement.
