# Proves that DRAM accesses work across a configurable range beyond 1 MiB.

	.equ	OLD_DRAM_SIZE, 0x00100000
	.equ	SYSCON_BASE, 0x00100000
	.equ	SYSCON_PASS, 0x5555
	.equ	TEST_VALUE, 0x11223344

	.text
	.globl _start
_start:
	la	t0, _start
	li	t1, OLD_DRAM_SIZE
	add	t0, t0, t1
	li	t2, TEST_VALUE
	sd	t2, 0(t0)
	ld	t3, 0(t0)
	bne	t2, t3, fail

	li	a0, 1
	li	t0, SYSCON_BASE
	li	t1, SYSCON_PASS
	sw	t1, 0(t0)
	j	halt

fail:
	li	a0, 0

halt:
	jal	x0, halt
