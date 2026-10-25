# Zba plus Zbb logical, count, min/max, and extension instructions.
	.text
	.globl _start
	.include "clear-registers.inc"
_start:
	clear_gprs
	li	x5, 0x0000000080000001
	li	x6, 0x0000000100000003
	add.uw	x8, x5, x6
	sh1add	x9, x5, x6
	sh1add.uw	x10, x5, x6
	sh2add	x11, x5, x6
	sh2add.uw	x12, x5, x6
	sh3add	x13, x5, x6
	sh3add.uw	x14, x5, x6
	slli.uw	x15, x5, 31
	andn	x16, x5, x6
	orn	x17, x5, x6
	xnor	x18, x5, x6
	clz	x19, x5
	clzw	x20, x5
	ctz	x21, x5
	ctzw	x22, x5
	cpop	x23, x5
	cpopw	x24, x5
	max	x25, x5, x6
	maxu	x26, x5, x6
	min	x27, x5, x6
	minu	x28, x5, x6
	sext.b	x29, x5
	sext.h	x30, x5
	zext.h	x31, x5
halt:
	jal	x0, halt
