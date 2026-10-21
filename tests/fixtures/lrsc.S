# LR/SC success, failure, invalidation, and failed-SC memory preservation.
	.text
	.globl _start
	.include "clear-registers.inc"
_start:
	clear_gprs
	la	t0, scratch
	addi	s0, t0, 8
	li	t1, 0x1122334455667788
	sd	t1, 0(t0)

	lr.d	t2, 0(t0)
	addi	t3, t2, 1
	sc.d	t4, t3, 0(t0)
	ld	t5, 0(t0)

	sd	t1, 0(t0)
	sc.d	t6, t3, 0(t0)
	ld	a0, 0(t0)

	lr.d	a3, 0(t0)
	sc.d	a4, t3, 0(s0)
	sc.d	a5, t3, 0(t0)

	li	t1, 0x123456789abcdef0
	sd	t1, 0(s0)
	lr.w	t2, 0(s0)
	addi	t3, t2, 1
	sc.w	t4, t3, 0(s0)
	lw	t5, 0(s0)
	sc.w	a7, t3, 0(s0)
halt:
	jal	x0, halt

	.data
	.align	3
scratch:
	.zero	16
