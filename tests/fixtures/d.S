# Exercises the RV64D instruction families against Spike.
	.text
	.globl _start
	.include "clear-registers.inc"
_start:
	clear_gprs
	lui	t0, 0x2
	csrw	mstatus, t0

	la	t3, values
	fld	ft0, 0(t3)             # 1.5
	fld	ft1, 8(t3)             # 2.5
	fld	ft8, 16(t3)            # -1.0
	fld	ft11, 24(t3)           # 0.5

	fadd.d	ft2, ft0, ft1
	fsub.d	ft3, ft0, ft1
	fmul.d	ft4, ft0, ft1
	fdiv.d	ft5, ft1, ft0
	fsqrt.d	ft6, ft4
	fmin.d	ft7, ft0, ft1
	fmax.d	fs0, ft0, ft1
	fsgnj.d	fs1, ft0, ft1
	fsgnjn.d fs2, ft0, ft1
	fsgnjx.d fs3, ft0, ft1
	fmadd.d	fa0, ft0, ft1, ft2
	fmsub.d	fa1, ft0, ft1, ft2
	fnmsub.d fa2, ft0, ft1, ft2
	fnmadd.d fa3, ft0, ft1, ft2

	feq.d	a0, ft0, ft0
	flt.d	a1, ft8, ft0
	fle.d	a2, ft0, ft1
	fclass.d a3, ft8
	fcvt.w.d a4, ft0, rtz
	fcvt.l.d a5, ft0
	fcvt.d.w fa4, a4
	fcvt.d.l fa5, a5, rmm
	fcvt.s.d fs4, ft0, rdn
	fcvt.d.s fs5, fs4
	fmv.x.d a6, ft0
	fmv.d.x fs6, a6

	la	t2, scratch
	fsd	ft2, 0(t2)
	fld	fs11, 0(t2)

	fdiv.d	ft9, ft0, ft11

halt:
	jal	x0, halt

	.data
	.balign 8
values:
	.dword	0x3ff8000000000000
	.dword	0x4004000000000000
	.dword	0xbff0000000000000
	.dword	0x3fe0000000000000
scratch:
	.dword	0
