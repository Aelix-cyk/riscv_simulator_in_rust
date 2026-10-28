# Exercises the RV64Zfh instruction families against Spike.
	.text
	.globl _start
	.include "clear-registers.inc"
_start:
	clear_gprs
	lui	t0, 0x2
	csrw	mstatus, t0

	la	t3, values
	flh	ft0, 0(t3)             # 1.5
	flh	ft1, 2(t3)             # 2.5
	flh	ft8, 4(t3)             # -1.0
	flh	ft11, 6(t3)            # 0.5

	fadd.h	ft2, ft0, ft1
	fsub.h	ft3, ft0, ft1
	fmul.h	ft4, ft0, ft1
	fdiv.h	ft5, ft1, ft0
	fsqrt.h	ft6, ft4
	fmin.h	ft7, ft0, ft1
	fmax.h	fs0, ft0, ft1
	fsgnj.h	fs1, ft0, ft1
	fsgnjn.h fs2, ft0, ft1
	fsgnjx.h fs3, ft0, ft1
	fmadd.h	fa0, ft0, ft1, ft2
	fmsub.h	fa1, ft0, ft1, ft2
	fnmsub.h fa2, ft0, ft1, ft2
	fnmadd.h fa3, ft0, ft1, ft2

	feq.h	a0, ft0, ft0
	flt.h	a1, ft8, ft0
	fle.h	a2, ft0, ft1
	fclass.h a3, ft8
	fcvt.w.h a4, ft0, rtz
	fcvt.wu.h a5, ft0
	fcvt.l.h s0, ft0
	fcvt.lu.h s1, ft0
	fcvt.h.w fa4, a4
	fcvt.h.wu fa5, a5
	fcvt.h.l fa6, s0, rmm
	fcvt.h.lu fa7, s1
	fcvt.s.h fs4, ft0
	fcvt.d.h fs5, ft0
	fcvt.h.s fs6, fs4, rdn
	fcvt.h.d fs7, fs5
	fmv.x.h a6, ft0
	fmv.h.x fs8, a6
	fmv.w.x	ft3, zero
	fmv.x.h	s2, ft3

	fmul.h	ft2, ft0, ft1, rup
	fsqrt.h	ft3, ft0, rtz
	fmadd.h	fa0, ft0, ft1, ft2, rdn
	fcvt.w.h a4, ft0, rmm

	la	t2, scratch
	fsh	ft2, 0(t2)
	flh	fs11, 0(t2)

	fdiv.h	ft9, ft0, ft11
	fsqrt.h	ft10, ft8

halt:
	jal	x0, halt

	.data
	.balign 2
values:
	.half	0x3e00
	.half	0x4100
	.half	0xbc00
	.half	0x3800
scratch:
	.half	0
