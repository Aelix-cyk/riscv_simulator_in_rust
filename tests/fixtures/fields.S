# Distinct register numbers for field assertions, plus legal RV64I encodings the
# decoder does not support yet. Used by tests/decode.rs.
	.text
	.globl _start
_start:
	addi	x5, x6, 7
	add	x7, x8, x9
	beq	x10, x11, fwd
	sub	x7, x8, x9          # legal, not decoded yet
fwd:
	xor	x7, x8, x9          # legal, not decoded yet
	andi	x7, x8, 3           # legal, not decoded yet
	bne	x10, x11, end       # legal, not decoded yet
	jal	x1, end
end:
	jal	x0, end
