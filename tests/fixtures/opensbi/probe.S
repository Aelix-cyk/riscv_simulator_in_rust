    .option norvc
    .section .text.init, "ax"
    .globl _start

_start:
    bnez a0, fail_hart

    li t0, 0xedfe0dd0
    lwu t1, 0(a1)
    bne t0, t1, fail_dtb

    la a0, success_message
    call puts

    li a7, 0x53525354
    li a6, 0
    li a0, 0
    li a1, 0
    ecall

    la a0, reset_failure_message
    call puts
    j fail

fail_hart:
    la a0, hart_failure_message
    call puts
    j fail

fail_dtb:
    la a0, dtb_failure_message
    call puts
    j fail

fail:
    j fail

puts:
    mv t2, a0
1:
    lbu a0, 0(t2)
    beqz a0, 2f
    li a7, 1
    ecall
    addi t2, t2, 1
    j 1b
2:
    ret

    .section .rodata, "a"
success_message:
    .asciz "ARVSR-PROBE-OK\n"
hart_failure_message:
    .asciz "ARVSR-PROBE-BAD-HART\n"
dtb_failure_message:
    .asciz "ARVSR-PROBE-BAD-DTB\n"
reset_failure_message:
    .asciz "ARVSR-PROBE-BAD-RESET\n"
