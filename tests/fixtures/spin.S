# A counted loop with no halt self-loop: used to test the step limit.
	.text
	.globl _start
	.include "clear-registers.inc"
_start:
	clear_gprs
	addi	x5, x5, 1
	jal	x0, _start
