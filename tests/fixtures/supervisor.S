# Exercises M/S/U privilege transitions, delegation, and supervisor CSRs.
	.text
	.globl _start
	.include "clear-registers.inc"
_start:
	clear_gprs
	la	t0, machine_trap
	csrw	mtvec, t0
	la	t0, supervisor_trap
	csrw	stvec, t0
	li	t0, 0x300              # Delegate U and S ecall.
	csrw	medeleg, t0
	csrw	satp, zero
	sfence.vma
	la	t0, supervisor_entry
	csrw	mepc, t0
	li	t0, 0x800              # mstatus.MPP = S.
	csrw	mstatus, t0
	mret

machine_trap:
	li	a7, -1
	j	halt

supervisor_entry:
	li	t0, 0x123
	csrw	sscratch, t0
	csrr	s0, sscratch
	li	t0, 0x2                # sstatus.SIE.
	csrw	sstatus, t0
	csrr	s1, sstatus
	andi	s2, s1, 0x2
	bne	s2, t0, machine_trap
	csrw	satp, zero
	csrr	s3, satp
	ecall

after_supervisor_ecall:
	la	t0, user_entry
	csrw	sepc, t0
	li	t0, 0x100
	csrc	sstatus, t0            # sstatus.SPP = U.
	sret

user_entry:
	ecall

after_user_ecall:
	li	s7, 0x55
	j	halt

supervisor_trap:
	csrr	t1, sstatus
	andi	t1, t1, 0x22           # SIE must be clear and SPIE set.
	li	t2, 0x20
	bne	t1, t2, machine_trap
	csrr	s4, scause
	csrr	s5, sepc
	csrr	s6, stval
	addi	s5, s5, 4
	csrw	sepc, s5
	li	t0, 9
	beq	s4, t0, delegated_supervisor_ecall
	li	s8, 1
	j	return_from_supervisor_trap
delegated_supervisor_ecall:
	li	s9, 1
return_from_supervisor_trap:
	sret

halt:
	jal	x0, halt
