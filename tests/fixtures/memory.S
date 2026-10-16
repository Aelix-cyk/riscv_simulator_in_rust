# Every load and store width in both extension forms. The scratch area lives in
# .data so it is part of the image the spike oracle compares. Misaligned accesses
# are deliberately absent: bare-metal spike raises an exception for them, with no
# handler installed, so they are covered by unit tests instead.
# Used by tests/isa_run.rs and the decode/render suites.
	.text
	.globl _start
	.include "clear-registers.inc"
_start:
	clear_gprs
	la	x5, scratch
	addi	x6, x0, -1           # all ones, to distinguish extension forms

	sw	x6, 0(x5)
	lw	x7, 0(x5)            # sign-extended word: -1
	lwu	x8, 0(x5)            # zero-extended word: 0xffff_ffff

	sh	x6, 4(x5)
	lh	x9, 4(x5)            # sign-extended halfword: -1
	lhu	x10, 4(x5)           # zero-extended halfword: 0xffff

	sb	x6, 8(x5)
	lb	x11, 8(x5)           # sign-extended byte: -1
	lbu	x12, 8(x5)           # zero-extended byte: 0xff

	sd	x6, 16(x5)
	ld	x13, 16(x5)          # doubleword: -1

halt:
	jal	x0, halt

	.data
	.align 3
scratch:
	.zero 32
