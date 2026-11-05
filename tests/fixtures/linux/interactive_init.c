// Interactive Linux PID 1 used by the aRVSR UART gate.

enum {
    SYS_READ = 63,
    SYS_WRITE = 64,
    SYS_EXIT = 93,
    SYS_REBOOT = 142,
};

enum {
    STDIN_FILENO = 0,
    STDOUT_FILENO = 1,
};

enum {
    LINUX_REBOOT_MAGIC1 = 0xfee1dead,
    LINUX_REBOOT_MAGIC2 = 672274793,
    LINUX_REBOOT_CMD_POWER_OFF = 0x4321fedc,
};

enum {
    LINE_BYTES = 128,
    NEWLINE = '\n',
    CARRIAGE_RETURN = '\r',
    READ_LINE_OK = 1,
    READ_LINE_OVERFLOW = -1,
};

static const char prompt[] = "ARVSR> ";
static const char help_text[] = "commands: help, echo <text>, exit\n";
static const char unknown_text[] = "unknown command\n";
static const char poweroff_failure[] = "ARVSR-POWEROFF-FAILED\n";
static const char echo_prefix[] = "echo ";

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

static long write_bytes(const char *bytes, unsigned long length) {
    return syscall3(SYS_WRITE, STDOUT_FILENO, (long)bytes, (long)length);
}

static void write_text(const char *text, unsigned long length) {
    while (length != 0) {
        long written = write_bytes(text, length);
        if (written <= 0) {
            return;
        }
        text += written;
        length -= written;
    }
}

static void write_cstring(const char *text) {
    const char *end = text;
    while (*end != '\0') {
        end++;
    }
    write_text(text, (unsigned long)(end - text));
}

static int read_byte(char *byte) {
    for (;;) {
        long status = syscall3(SYS_READ, STDIN_FILENO, (long)byte, 1);
        if (status > 0) {
            return 1;
        }
        return 0;
    }
}

static int read_line(char *line) {
    unsigned long length = 0;
    int overflow = 0;

    for (;;) {
        char byte;
        if (!read_byte(&byte)) {
            return 0;
        }
        if (byte == NEWLINE || byte == CARRIAGE_RETURN) {
            if (!overflow) {
                line[length] = '\0';
                return READ_LINE_OK;
            }
            write_text(unknown_text, sizeof(unknown_text) - 1);
            return READ_LINE_OVERFLOW;
        }
        if (length + 1 < LINE_BYTES) {
            line[length++] = byte;
        } else {
            overflow = 1;
        }
    }
}

static int starts_with(const char *text, const char *prefix) {
    while (*prefix != '\0') {
        if (*text++ != *prefix++) {
            return 0;
        }
    }
    return 1;
}

static int equal(const char *left, const char *right) {
    while (*left == *right) {
        if (*left == '\0') {
            return 1;
        }
        left++;
        right++;
    }
    return 0;
}

static void power_off(void) {
    syscall3(
        SYS_REBOOT,
        LINUX_REBOOT_MAGIC1,
        LINUX_REBOOT_MAGIC2,
        LINUX_REBOOT_CMD_POWER_OFF);
    write_text(poweroff_failure, sizeof(poweroff_failure) - 1);
    syscall1(SYS_EXIT, 1);
}

void _start(void) {
    char line[LINE_BYTES];

    for (;;) {
        int read_status;
        write_text(prompt, sizeof(prompt) - 1);
        read_status = read_line(line);
        if (read_status == 0) {
            power_off();
        }
        if (read_status < 0) {
            continue;
        }
        if (equal(line, "help")) {
            write_text(help_text, sizeof(help_text) - 1);
        } else if (starts_with(line, echo_prefix)) {
            write_cstring(line + sizeof(echo_prefix) - 1);
            write_text("\n", 1);
        } else if (equal(line, "exit")) {
            power_off();
        } else {
            write_text(unknown_text, sizeof(unknown_text) - 1);
        }
    }
}
