# Bug 0021: Unsupported trigger CSRs made the breakpoint fixture fail

| Field | Value |
|---|---|
| Found | 2026-10-29, platform bring-up review |
| Fixed | 2026-10-29 |
| Affected | `src/csr.rs`, `tests/compliance/expected_failures.tsv` |

## Summary

The legacy `rv64mi-p-breakpoint` test probes the optional debug-trigger CSRs.
Because the simulator rejected every trigger CSR as an unsupported CSR,
software could not distinguish a hart with no debug triggers from a hart that
omitted the CSRs entirely. The resulting illegal-instruction trap failed the
fixture's capability probing.

## Specification requirement

The debug specification makes trigger hardware optional. A hart with no
implemented triggers must expose enough CSR behavior for software to determine
that no triggers are available, rather than trapping indefinitely while
probing individual trigger fields.

## Repro

```sh
COMPLIANCE_FILTER=breakpoint \
  cargo test --test compliance riscv_tests_gate -- --ignored --nocapture
```

Before the fix, the test was listed as a known failure in
`tests/compliance/expected_failures.tsv`.

## Fix

`tcontrol`, `tselect`, `tdata1`, `tdata2`, and `tdata3` are now M-mode CSRs that
accept WARL-style writes and read as zero. Zero `tselect` and zero trigger data
correctly report that the hart has no debug triggers.

## Regression test

`debug_trigger_csrs_report_that_no_triggers_exist` verifies the read/write
behavior. The filtered legacy compliance test now passes without an expected
failure entry.
