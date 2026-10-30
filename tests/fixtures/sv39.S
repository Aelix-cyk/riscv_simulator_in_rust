# Exercises Sv39 translation and a delegated load-page fault.
	.text
	.globl _start
	.include "clear-registers.inc"
_start:
	clear_gprs
	la	t0, machine_trap
	csrw	mtvec, t0
	la	t0, supervisor_trap
	csrw	stvec, t0
	li	t0, 0x2000             # Delegate load-page faults.
	csrw	medeleg, t0

	la	t1, root_table
	la	t2, alias_level1
	srli	t2, t2, 12
	slli	t2, t2, 10
	ori	t2, t2, 1
	sd	t2, 8(t1)

	la	t1, alias_level1
	la	t2, alias_level0
	srli	t2, t2, 12
	slli	t2, t2, 10
	ori	t2, t2, 1
	sd	t2, 0(t1)

	la	t1, alias_level0
	la	t2, data_page
	srli	t2, t2, 12
	slli	t2, t2, 10
	li	t3, 0xcf               # V | R | W | X | A | D.
	or	t2, t2, t3
	sd	t2, 0(t1)

	la	t1, root_table
	li	t2, 0x80000            # DRAM_BASE >> 12.
	slli	t2, t2, 10
	or	t2, t2, t3
	sd	t2, 16(t1)

	la	t1, root_table
	srli	t1, t1, 12
	li	t2, 8
	slli	t2, t2, 60
	or	t1, t1, t2
	csrw	satp, t1
	sfence.vma

	la	t1, supervisor_entry
	csrw	mepc, t1
	li	t1, 0x800              # mstatus.MPP = S.
	csrw	mstatus, t1
	li	t3, -1
	csrw	pmpaddr0, t3
	li	t3, 0x1f               # NAPOT, read/write/execute.
	csrw	pmpcfg0, t3
	mret

	.balign 4
machine_trap:
	li	a7, -1
	j	halt

supervisor_entry:
	li	t1, 0x40000000
	li	t2, 123
	sd	t2, 0(t1)
	ld	s0, 0(t1)

	li	t1, 0x40001000
	ld	s1, 0(t1)             # Faults because VPN[0] = 1 is unmapped.
	li	s4, 456

	.balign 4
halt:
	jal	x0, halt

	.balign 4
supervisor_trap:
	csrr	s2, scause
	csrr	s3, sepc
	addi	s3, s3, 4
	csrw	sepc, s3
	sret

	.data
	.balign 4096
root_table:
	.zero	4096
alias_level1:
	.zero	4096
alias_level0:
	.zero	4096
data_page:
	.dword	0
