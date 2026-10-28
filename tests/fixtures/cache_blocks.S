# Cache-block management, zero, prefetch, and pause hints.
	.text
	.globl _start
	.include "clear-registers.inc"
_start:
	clear_gprs
	li	x5, 0x80000100
	li	x6, 0x0123456789abcdef
	sd	x6, 0(x5)
	sd	x6, 8(x5)
	sd	x6, 16(x5)
	sd	x6, 24(x5)
	sd	x6, 32(x5)
	sd	x6, 40(x5)
	sd	x6, 48(x5)
	sd	x6, 56(x5)
	cbo.zero	(x5)
	cbo.clean	(x5)
	cbo.flush	(x5)
	cbo.inval	(x5)
	prefetch.i	0(x5)
	prefetch.r	0(x5)
	prefetch.w	0(x5)
	pause
	ld	x7, 8(x5)
	ld	x8, 56(x5)
halt:
	jal	x0, halt
