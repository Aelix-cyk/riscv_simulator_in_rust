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
| [0009 — HPM event selectors disagree with the ACT4 declaration](0009-hpm-events-mismatch-act4-config.md) | Fixed |
| [0010 — CBO.ZERO leaves reservations in the middle of the block valid](0010-cbo-zero-leaves-middle-reservations-valid.md) | Fixed |
| [0011 — cache-geometry status contradicts itself across the choice docs](0011-cache-geometry-status-inconsistent.md) | Fixed |
| [0012 — widening FP conversions skip rounding-field validation](0012-widening-fp-conversions-skip-rounding-validation.md) | Fixed |
| [0013 — mip/sip writes drop writable STIP and SEIP bits](0013-mip-writable-mask-omits-supervisor-pending-bits.md) | Fixed |
| [0014 — direct execute records zero tval for system instructions](0014-direct-execute-tval-zero-for-system-instructions.md) | Fixed |
| [0015 — supervisor fixture sets VS instead of SIE](0015-supervisor-fixture-sets-vs-instead-of-sie.md) | Fixed |
| [0016 — unimplemented-opcode trap documentation ignores delegation](0016-unimplemented-opcode-trap-doc-stale.md) | Fixed |
