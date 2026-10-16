# Each of the six conditions, taken once and not taken once. A taken branch skips
# the instruction after it, so the skipped register stays zero; the not-taken
# group falls through to a success marker, and any misbehaving branch lands on
# the failure marker instead. Used by tests/isa_run.rs and the decode/render suites.
	.text
	.globl _start
	.include "clear-registers.inc"
_start:
	clear_gprs
	addi	x5, x0, 1            # smaller
	addi	x6, x0, 2            # larger
	addi	x7, x0, -1           # 0xffff_ffff_ffff_ffff: signed < 0, unsigned huge

	# Taken cases: each branch skips the marker that follows it.
	beq	x5, x5, 1f
	addi	x20, x0, 99
1:	bne	x5, x6, 2f
	addi	x21, x0, 99
2:	blt	x7, x5, 3f           # signed: -1 < 1
	addi	x22, x0, 99
3:	bge	x5, x7, 4f           # signed: 1 >= -1
	addi	x23, x0, 99
4:	bltu	x5, x6, 5f           # unsigned: 1 < 2
	addi	x24, x0, 99
5:	bgeu	x6, x5, 6f           # unsigned: 2 >= 1
	addi	x25, x0, 99
6:
	# Not-taken cases: every one of these would land on the failure marker.
	beq	x5, x6, fail
	bne	x5, x5, fail
	blt	x5, x7, fail         # signed: 1 < -1 is false
	bge	x7, x5, fail         # signed: -1 >= 1 is false
	bltu	x7, x5, fail         # unsigned: huge < 1 is false
	bgeu	x5, x7, fail         # unsigned: 1 >= huge is false

	addi	x26, x0, 1           # reached only when all six fall through
	jal	x0, halt
fail:
	addi	x26, x0, 511
halt:
	jal	x0, halt
