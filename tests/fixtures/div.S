# Every RV64M divide/remainder instruction, including zero and overflow cases.
	.text
	.globl _start
	.include "clear-registers.inc"
_start:
	clear_gprs
	addi	x5, zero, -7
	addi	x6, zero, 3
	div	x7, x5, x6
	rem	x8, x5, x6
	divu	x9, x5, x6
	remu	x10, x5, x6

	li	x11, 0x8000000000000000
	addi	x12, zero, -1
	div	x13, x11, x12
	rem	x14, x11, x12
	div	x15, x5, zero
	rem	x16, x5, zero

	addi	x18, zero, -7
	addi	x19, zero, 3
	divw	x20, x18, x19
	remw	x21, x18, x19
	divuw	x22, x18, x19
	remuw	x23, x18, x19
	divw	x24, x18, zero
	remw	x25, x18, zero
	divuw	x26, x18, zero
	remuw	x27, x18, zero

	li	x28, 0x0000000080000000
	li	x29, -1
	divw	x30, x28, x29
	remw	x31, x28, x29
halt:
	jal	x0, halt
