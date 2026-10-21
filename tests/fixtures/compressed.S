# Exercises every non-trapping Zca instruction with natural mixed-width control flow.
# The Spike oracle compares all registers and memory after the halt self-loop.
	.text
	.globl _start
	.include "clear-registers.inc"
_start:
	clear_gprs
	la	x2, scratch
	c.mv	x8, x2
	li	x9, 0x12345678
	c.sw	x9, 0(x8)
	c.lw	x10, 0(x8)
	li	x11, 0x0123456789abcdef
	c.sd	x11, 8(x8)
	c.ld	x12, 8(x8)
	c.addi16sp x2, 16
	c.swsp	x9, 0(x2)
	c.lwsp	x13, 0(x2)
	c.sdsp	x11, 8(x2)
	c.ldsp	x14, 8(x2)
	c.addi4spn x15, sp, 32

	c.li	x16, -3
	c.mv	x17, x16
	c.addi	x16, 2
	c.addiw x16, 1
	c.mv	x15, x16
	c.andi	x15, 15
	c.slli	x15, 3
	c.srli	x15, 1
	c.srai	x15, 1
	c.lui	x18, 1
	c.add	x16, x17
	c.mv	x8, x17
	c.mv	x9, x16
	c.sub	x8, x9
	c.xor	x8, x9
	c.or	x8, x9
	c.and	x8, x9
	c.addw	x8, x9
	c.subw	x8, x9
	c.nop

	c.li	x15, 0
	c.beqz	x15, beqz_ok
	c.li	x21, 9
beqz_ok:
	c.addi	x21, 1
	c.li	x15, 1
	c.bnez	x15, bnez_ok
	c.li	x22, 9
bnez_ok:
	c.addi	x22, 1
	c.j	jump_ok
	c.li	x23, 9
jump_ok:
	c.addi	x23, 1

	la	x24, called
	c.jalr	x24
	c.addi	x25, 1
	c.j	after_call
called:
	c.addi	x26, 1
	c.jr	x1
after_call:
	c.addi	x27, 1

halt:
	jal	x0, halt

	.data
	.align 3
scratch:
	.zero 48
