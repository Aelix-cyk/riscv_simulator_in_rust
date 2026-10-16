# The fence forms the assembler emits, plus fence.i from Zifencei. Both are
# no-ops on one hart with no cache model. Used by tests/isa_run.rs and the
# decode/render suites.
	.text
	.globl _start
	.include "clear-registers.inc"
_start:
	clear_gprs
	fence                # iorw,iorw
	fence	rw,rw
	fence	rw,w
	fence.i
	addi	x5, x0, 1      # observable effect after the fences
halt:
	jal	x0, halt
