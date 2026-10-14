# Sums 1..10 into a0, then halts in a self-loop.
# Used by tests/decode.rs and the integration test in tests/loop_run.rs.
	.text
	.globl _start
_start:
	addi	a0, zero, 0          # sum
	addi	a1, zero, 1          # i
	addi	a2, zero, 11         # limit
loop:
	add	a0, a0, a1
	addi	a1, a1, 1
	beq	a1, a2, done
	jal	zero, loop
done:
	jal	zero, done           # halt: self-loop
