# Distinct register numbers for field assertions, plus a legal M-extension
# encoding the decoder does not support yet. Used by tests/decode.rs.
	.text
	.globl _start
_start:
	addi	x5, x6, 7
	add	x7, x8, x9
	beq	x10, x11, fwd
	sub	x7, x8, x9
fwd:
	xor	x7, x8, x9
	andi	x7, x8, 3
	bne	x10, x11, end
	jal	x1, end
	lw	x5, 0(x6)
	mul	x7, x8, x9          # legal M extension, not decoded yet
end:
	jal	x0, end
