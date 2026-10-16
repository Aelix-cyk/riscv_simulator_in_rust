# One instruction per implemented ALU operation. Used by tests/alu_run.rs.
	.text
	.globl _start
	.include "clear-registers.inc"
_start:
	clear_gprs
	addi	x5, x0, -1
	addi	x6, x0, 7
	addi	x7, x6, -3          # negative immediate
	slti	x8, x7, 5
	sltiu	x9, x5, 1
	xori	x10, x7, 12
	ori	x11, x6, 8
	andi	x12, x7, 6
	slli	x13, x6, 3
	srli	x14, x7, 1
	srai	x15, x5, 1
	addiw	x16, x5, 1
	slliw	x17, x6, 2
	srliw	x18, x5, 1
	sraiw	x19, x5, 1
	add	x20, x5, x6
	sub	x21, x6, x7
	sll	x22, x6, x7
	slt	x23, x5, x6
	sltu	x24, x5, x6
	xor	x25, x7, x6
	srl	x26, x5, x6
	sra	x27, x5, x6
	or	x28, x6, x7
	and	x29, x6, x7
	addw	x30, x6, x7
	subw	x31, x7, x6
	sllw	x30, x6, x7
	srlw	x31, x5, x6
	sraw	x30, x5, x6
halt:
	jal	x0, halt
