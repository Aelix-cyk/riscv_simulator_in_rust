# Exercises the platform timer, UART, trap delivery, and exit device.

	.equ	CLINT_MTIMECMP, 0x02004000
	.equ	MACHINE_TIMER_INTERRUPT, 0x80
	.equ	MSTATUS_MIE, 0x8
	.equ	SYSCON_BASE, 0x00100000
	.equ	SYSCON_PASS, 0x5555
	.equ	UART_BASE, 0x10000000
	.equ	TIMER_DELAY, 1000
	.equ	NEWLINE, 10
	.equ	LETTER_O, 79
	.equ	LETTER_T, 84

	.text
	.globl _start
	.include "clear-registers.inc"
_start:
	clear_gprs
	la	t0, timer_handler
	csrw	mtvec, t0

	li	t0, CLINT_MTIMECMP
	li	t1, TIMER_DELAY
	sd	t1, 0(t0)
	li	t0, MACHINE_TIMER_INTERRUPT
	csrw	mie, t0
	li	t0, MSTATUS_MIE
	csrw	mstatus, t0

	li	t0, UART_BASE
	li	t1, LETTER_T
	sw	t1, 0(t0)

wait_for_timer:
	wfi
	j	wait_for_timer

timer_handler:
	li	a0, 1
	li	t0, UART_BASE
	li	t1, LETTER_O
	sw	t1, 0(t0)
	li	t1, NEWLINE
	sw	t1, 0(t0)

	li	t0, SYSCON_BASE
	li	t1, SYSCON_PASS
	sw	t1, 0(t0)

halt:
	jal	x0, halt
