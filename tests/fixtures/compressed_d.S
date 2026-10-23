# Exercises Zcd compressed double-precision loads and stores.
	.text
	.globl _start
	.include "clear-registers.inc"
_start:
	clear_gprs
	lui	t0, 0x2
	csrw	mstatus, t0

	la	s0, values
	mv	sp, s0
	c.fld	fs0, 0(s0)
	c.fld	fs1, 8(s0)
	fadd.d	fa0, fs0, fs1
	c.fsd	fa0, 16(s0)
	c.fldsp	fs3, 16(sp)
	c.fsdsp	fa0, 24(sp)

halt:
	jal	x0, halt

	.data
	.balign 8
values:
	.dword	0x3ff8000000000000
	.dword	0x4004000000000000
	.dword	0
	.dword	0
