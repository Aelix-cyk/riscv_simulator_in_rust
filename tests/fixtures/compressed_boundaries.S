# Places a standard 32-bit instruction at a two-byte, non-four-byte boundary,
# then switches back to compressed instructions on each side.
	.text
	.globl _start
	.include "clear-registers.inc"
_start:
	clear_gprs
	c.nop
	addi	x5, x0, 40
	c.addi	x5, 2
	c.addi	x6, 1
	jal	x0, halt
halt:
	jal	x0, halt
