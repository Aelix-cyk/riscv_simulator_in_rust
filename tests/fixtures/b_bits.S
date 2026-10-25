# Zbb rotations and byte operations, followed by every Zbs form.
	.text
	.globl _start
	.include "clear-registers.inc"
_start:
	clear_gprs
	li	x5, 0x8123456789abcdef
	li	x6, 7
	rol	x8, x5, x6
	rolw	x9, x5, x6
	ror	x10, x5, x6
	rori	x11, x5, 7
	roriw	x12, x5, 7
	rorw	x13, x5, x6
	orc.b	x14, x5
	rev8	x15, x5
	bset	x16, x5, x6
	bclr	x17, x5, x6
	binv	x18, x5, x6
	bext	x19, x5, x6
	bseti	x20, x5, 40
	bclri	x21, x5, 4
	binvi	x22, x5, 12
	bexti	x23, x5, 3
halt:
	jal	x0, halt
