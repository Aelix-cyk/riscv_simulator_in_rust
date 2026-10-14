# Negative immediates, backward branch/jump targets, and illegal encodings.
# Used by tests/decode.rs for sign-extension and trap coverage.
	.text
	.globl _start
_start:
	addi	a0, zero, -1         # sign-extended negative immediate
	addi	a1, zero, -2048      # smallest 12-bit signed immediate
	beq	a0, a1, _start       # backward branch
	jal	zero, _start         # backward jump
	.word	0x00000000           # illegal: opcode zero
	.word	0xffffffff           # illegal: all ones
