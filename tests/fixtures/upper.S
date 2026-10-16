# lui, auipc, and jalr, including the la sequence they build. Used by
# tests/isa_run.rs and the decode and render suites.
	.text
	.globl _start
	.include "clear-registers.inc"
_start:
	clear_gprs
	lui	x5, 0x12345          # U-type upper immediate
	auipc	x6, 0x1              # pc + 0x1000
	auipc	x7, 0xfffff          # pc - 0x1000
	la	x8, second           # auipc + addi
	jalr	x9, x8, 0            # link into x9, jump to second
	addi	x11, x0, 1           # skipped
second:
	addi	x11, x0, 2
	la	x8, halt
	jalr	x0, x8, 0            # plain jump
	addi	x11, x0, 3           # skipped
halt:
	jal	x0, halt
