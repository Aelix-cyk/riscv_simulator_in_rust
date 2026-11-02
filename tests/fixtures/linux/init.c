// Minimal Linux PID 1 used by the aRVSR boot gate.

enum {
    SYS_WRITE = 64,
    SYS_EXIT = 93,
    SYS_REBOOT = 142,
};

enum {
    STDOUT_FILENO = 1,
};

enum {
    LINUX_REBOOT_MAGIC1 = 0xfee1dead,
    LINUX_REBOOT_MAGIC2 = 672274793,
    LINUX_REBOOT_CMD_POWER_OFF = 0x4321fedc,
};

static const char success_message[] = "ARVSR-LINUX-OK\n";
static const char failure_message[] = "ARVSR-POWEROFF-FAILED\n";

static long syscall3(long number, long first, long second, long third) {
    register long a0 asm("a0") = first;
    register long a1 asm("a1") = second;
    register long a2 asm("a2") = third;
    register long a7 asm("a7") = number;

    asm volatile("ecall"
                 : "+r"(a0)
                 : "r"(a1), "r"(a2), "r"(a7)
                 : "memory");
    return a0;
}

static long syscall1(long number, long first) {
    register long a0 asm("a0") = first;
    register long a7 asm("a7") = number;

    asm volatile("ecall" : "+r"(a0) : "r"(a7) : "memory");
    return a0;
}

static void write_message(const char *message, unsigned long bytes) {
    syscall3(SYS_WRITE, STDOUT_FILENO, (long)message, (long)bytes);
}

void _start(void) {
    write_message(success_message, sizeof(success_message) - 1);

    syscall3(
        SYS_REBOOT,
        LINUX_REBOOT_MAGIC1,
        LINUX_REBOOT_MAGIC2,
        LINUX_REBOOT_CMD_POWER_OFF);

    write_message(failure_message, sizeof(failure_message) - 1);
    syscall1(SYS_EXIT, 1);

    for (;;) {
    }
}
