# Every RV64M multiply instruction, with signed and unsigned high halves.
	.text
	.globl _start
	.include "clear-registers.inc"
_start:
	clear_gprs
	li	x5, 0x8000000000000000
	li	x6, 2
	mul	x7, x5, x6
	mulh	x8, x5, x6
	mulhsu	x9, x5, x6
	mulhu	x10, x5, x6

	addi	x11, zero, -7
	addi	x12, zero, 3
	mul	x13, x11, x12
	mulw	x14, x11, x12

	li	x15, 0x0000000180000002
	addi	x16, zero, -1
	mulw	x17, x15, x16

	li	x18, 0x1100
	csrr	x19, misa
	and	x19, x19, x18
halt:
	jal	x0, halt
