# Exercises the RV64F instruction families and compares the complete
# architectural result with Spike.
	.text
	.globl _start
	.include "clear-registers.inc"
_start:
	clear_gprs
	lui	t0, 0x2
	csrw	mstatus, t0

	lui	t1, 0x3fc00            # 1.5
	fmv.w.x	ft0, t1
	lui	t1, 0x40200            # 2.5
	fmv.w.x	ft1, t1
	lui	t1, 0xbf800            # -1.0
	fmv.w.x	ft8, t1
	fmv.w.x	ft11, zero

	fadd.s	ft2, ft0, ft1
	fsub.s	ft3, ft0, ft1
	fmul.s	ft4, ft0, ft1
	fdiv.s	ft5, ft1, ft0
	fsqrt.s	ft6, ft4
	fmin.s	ft7, ft0, ft1
	fmax.s	fs0, ft0, ft1
	fsgnj.s	fs1, ft0, ft1
	fsgnjn.s fs2, ft0, ft1
	fsgnjx.s fs3, ft0, ft1
	fmadd.s	fa0, ft0, ft1, ft2
	fmsub.s	fa1, ft0, ft1, ft2
	fnmsub.s fa2, ft0, ft1, ft2
	fnmadd.s fa3, ft0, ft1, ft2

	feq.s	a0, ft0, ft0
	flt.s	a1, ft8, ft0
	fle.s	a2, ft0, ft1
	fclass.s a3, ft8
	fcvt.w.s a4, ft0
	fcvt.l.s a5, ft0
	fcvt.s.w fa4, a4
	fcvt.s.l fa5, a5
	fmv.x.w a6, ft0

	la	t2, scratch
	fsw	ft2, 0(t2)
	flw	fs11, 0(t2)

	fdiv.s	ft9, ft0, ft11
	fsqrt.s	ft10, ft8

halt:
	jal	x0, halt

	.data
	.balign 8
scratch:
	.word	0
