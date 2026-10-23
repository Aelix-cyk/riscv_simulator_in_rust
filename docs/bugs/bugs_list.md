# Bug list

| Bug | Status |
|---|---|
| [0001 — jal clobbers rd on a misaligned jump target](0001-jal-clobbers-rd-on-misaligned-target.md) | Fixed |
| [0002 — SC skips the store-access check when it fails](0002-sc-skips-store-access-check-on-failure.md) | Fixed |
| [0003 — AMO read faults report the load-access cause](0003-amo-read-fault-reports-load-cause.md) | Fixed |
| [0004 — c.lui renders negative immediates as 64-bit values](0004-c-lui-negative-immediate-render.md) | Fixed |
| [0005 — zero-valued CSRRS/CSRRC sources bypass the read-only CSR check](0005-zero-csr-source-bypasses-read-only-check.md) | Fixed |
| [0006 — execute() retires instructions without advancing mcycle](0006-execute-retires-without-advancing-mcycle.md) | Fixed |
| [0007 — spurious underflow when a result rounds up to the minimum normal](0007-spurious-underflow-at-min-normal-boundary.md) | Fixed |
| [0008 — trace output omits static floating-point rounding modes](0008-fp-trace-omits-static-rounding.md) | Fixed |
