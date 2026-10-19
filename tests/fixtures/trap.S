# Exercises every Zicsr operation plus ecall, ebreak, and mret. The handler
# advances past traps and counts them in mscratch so the final register state
# proves that trap entry and return worked.
	.text
	.globl _start
	.include "clear-registers.inc"
_start:
	clear_gprs
	la	t0, handler
	csrw	mtvec, t0
	csrsi	mstatus, 8          # enable MIE for preservation across traps
	csrwi	mtval, 0            # make ecall's unspecified mtval deterministic

	csrwi	mscratch, 1
	csrrsi	t1, mscratch, 2     # t1 = 1, mscratch = 3
	csrrci	t2, mscratch, 1     # t2 = 3, mscratch = 2
	csrrs	t3, mscratch, zero  # t3 = 2
	csrrw	t4, mscratch, zero  # t4 = 2, mscratch = 0
	csrrc	t5, mscratch, t1    # t5 = 0

	ecall
	ebreak
	csrr	t6, mstatus
	andi	t6, t6, 0x88      # keep implemented MIE/MPIE bits
halt:
	jal	x0, halt

handler:
	csrr	a4, mcause
	csrr	a5, mepc
	addi	a5, a5, 4
	csrw	mepc, a5
	csrr	a3, mtval
	csrr	a6, mscratch
	addi	a6, a6, 1
	csrw	mscratch, a6
	mret
