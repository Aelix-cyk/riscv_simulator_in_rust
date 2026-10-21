# Every standard RV64A AMO, with AQ/RL variants exercised in the trace.
	.text
	.globl _start
	.include "clear-registers.inc"
_start:
	clear_gprs
	la	t0, scratch
	li	t1, 10
	sd	t1, 0(t0)
	li	t2, 5

	amoswap.w	t3, t2, (t0)
	amoadd.w.aq	t4, t2, (t0)
	amoxor.w.rl	t5, t2, (t0)
	amoor.w	t6, t2, (t0)
	amoand.w	a0, t2, (t0)
	amomin.w	a1, t2, (t0)
	amomax.w	a2, t2, (t0)
	amominu.w	a3, t2, (t0)
	amomaxu.w	a4, t2, (t0)

	li	s2, 0x0000000100000002
	sd	s2, 0(t0)
	li	s3, 5

	amoswap.d.aqrl	s4, s3, (t0)
	amoadd.d	s5, s3, (t0)
	amoxor.d	s6, s3, (t0)
	amoor.d	s7, s3, (t0)
	amoand.d	s8, s3, (t0)
	amomin.d	s9, s3, (t0)
	amomax.d	s10, s3, (t0)
	amominu.d	s11, s3, (t0)
	amomaxu.d	t3, s3, (t0)
halt:
	jal	x0, halt

	.data
	.align	3
scratch:
	.zero	8
