//! End-to-end OpenSBI-to-Linux boot gate.

#[path = "support/linux.rs"]
mod linux;
#[path = "support/opensbi.rs"]
mod opensbi;

use std::io::{BufReader, Read, Write};
use std::process::{ChildStdout, Command, ExitStatus, Stdio};

const DEFAULT_BOOT_TIMEOUT_SECONDS: u64 = 300;
const OPENSBI_BANNER: &str = "OpenSBI v1.9";
const LINUX_BANNER: &str = "Linux version ";
const PMU_INITIALIZED: &str = "riscv-pmu-sbi: SBI PMU extension is available";
const INIT_SUCCESS: &str = "ARVSR-LINUX-OK";
const BOOTARGS: &str = "console=ttyS0 earlycon=sbi";
const CUSTOM_PROMPT: &[u8] = b"ARVSR> ";
const CUSTOM_HELP: &[u8] = b"commands: help, echo <text>, exit";
const CUSTOM_ECHO: &[u8] = b"echo ARVSR-UART-OK\n";
const CUSTOM_EXIT: &[u8] = b"exit\n";
const BUSYBOX_ECHO: &[u8] = b"echo ARVSR-BUSYBOX-OK\n";
const BUSYBOX_POWEROFF: &[u8] = b"poweroff -f\n";
const BUSYBOX_READY: &str = "ARVSR-BUSYBOX-READY";
const CUSTOM_UART_SUCCESS: &str = "ARVSR-UART-OK";
const BUSYBOX_UART_SUCCESS: &str = "ARVSR-BUSYBOX-OK";

struct InteractionStep {
    wait_for: &'static [u8],
    send: &'static [u8],
}

struct InteractiveOutput {
    status: ExitStatus,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

#[test]
#[ignore = "requires LINUX_DIR/OPENSBI_DIR, RISCV/DTC, LLVM, cpio, and builds Linux v6.12.99"]
fn linux_reaches_an_smode_userspace_init() {
    let opensbi = opensbi::OpenSbiFixture::build();
    let linux = linux::LinuxFixture::build();
    let timeout_seconds = std::env::var("LINUX_BOOT_TIMEOUT_SECONDS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(DEFAULT_BOOT_TIMEOUT_SECONDS);

    let output = Command::new("timeout")
        .arg(timeout_seconds.to_string())
        .arg(env!("CARGO_BIN_EXE_riscv_simulator_in_rust"))
        .arg("--firmware")
        .arg(&opensbi.firmware)
        .arg("--dtb")
        .arg(&opensbi.device_tree)
        .arg("--kernel")
        .arg(&linux.kernel)
        .arg("--initrd")
        .arg(&linux.initrd)
        .arg("--bootargs")
        .arg(BOOTARGS)
        .output()
        .expect("run simulator CLI");
    assert!(
        output.status.success(),
        "Linux boot failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains(OPENSBI_BANNER), "{stdout}");
    assert!(stdout.contains(LINUX_BANNER), "{stdout}");
    assert!(stdout.contains(PMU_INITIALIZED), "{stdout}");
    assert!(stdout.contains(INIT_SUCCESS), "{stdout}");
}

#[test]
#[ignore = "requires Linux/OpenSBI/BusyBox checkouts, host build tools, and a RISC-V musl cross compiler"]
fn linux_userspace_interacts_over_the_uart() {
    linux::validate_busybox_environment();
    let opensbi = opensbi::OpenSbiFixture::build();
    let linux = linux::LinuxFixture::build();
    let interactive = linux.build_interactive();

    let custom = run_interactive(
        &opensbi,
        &linux,
        &interactive.custom_initrd,
        &[
            InteractionStep {
                wait_for: CUSTOM_PROMPT,
                send: b"help\n",
            },
            InteractionStep {
                wait_for: CUSTOM_HELP,
                send: CUSTOM_ECHO,
            },
            InteractionStep {
                wait_for: CUSTOM_UART_SUCCESS.as_bytes(),
                send: CUSTOM_EXIT,
            },
        ],
    );
    assert!(
        custom.status.success(),
        "custom interactive init failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&custom.stdout),
        String::from_utf8_lossy(&custom.stderr)
    );
    let stdout = String::from_utf8_lossy(&custom.stdout);
    assert!(
        stdout.contains("commands: help, echo <text>, exit"),
        "{stdout}"
    );
    assert!(stdout.contains(CUSTOM_UART_SUCCESS), "{stdout}");
    assert!(
        !stdout.contains("pc = "),
        "interactive mode printed final state"
    );

    let busybox = run_interactive(
        &opensbi,
        &linux,
        &interactive.busybox_initrd,
        &[
            InteractionStep {
                wait_for: BUSYBOX_READY.as_bytes(),
                send: BUSYBOX_ECHO,
            },
            InteractionStep {
                wait_for: BUSYBOX_UART_SUCCESS.as_bytes(),
                send: BUSYBOX_POWEROFF,
            },
        ],
    );
    assert!(
        busybox.status.success(),
        "BusyBox interactive userspace failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&busybox.stdout),
        String::from_utf8_lossy(&busybox.stderr)
    );
    let stdout = String::from_utf8_lossy(&busybox.stdout);
    assert!(stdout.contains(BUSYBOX_READY), "{stdout}");
    assert!(stdout.contains(BUSYBOX_UART_SUCCESS), "{stdout}");
    assert!(
        !stdout.contains("pc = "),
        "interactive mode printed final state"
    );
}

fn run_interactive(
    opensbi: &opensbi::OpenSbiFixture,
    linux: &linux::LinuxFixture,
    initrd: &std::path::Path,
    steps: &[InteractionStep],
) -> InteractiveOutput {
    let timeout_seconds = std::env::var("LINUX_BOOT_TIMEOUT_SECONDS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(DEFAULT_BOOT_TIMEOUT_SECONDS);
    let mut child = Command::new("timeout")
        .arg(timeout_seconds.to_string())
        .arg(env!("CARGO_BIN_EXE_riscv_simulator_in_rust"))
        .arg("--firmware")
        .arg(&opensbi.firmware)
        .arg("--dtb")
        .arg(&opensbi.device_tree)
        .arg("--kernel")
        .arg(&linux.kernel)
        .arg("--initrd")
        .arg(initrd)
        .arg("--bootargs")
        .arg(BOOTARGS)
        .arg("--interactive")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("start interactive simulator");
    let mut stdin = child.stdin.take().expect("interactive stdin");
    let stdout = child.stdout.take().expect("interactive stdout");
    let mut stdout = BufReader::new(stdout);
    let mut received = Vec::new();
    for step in steps {
        wait_for_output(&mut stdout, &mut received, step.wait_for);
        stdin.write_all(step.send).expect("write interactive input");
        stdin.flush().expect("flush interactive input");
    }
    drop(stdin);

    let status = child.wait().expect("wait for interactive simulator");
    stdout
        .read_to_end(&mut received)
        .expect("read remaining interactive output");
    let mut stderr = Vec::new();
    child
        .stderr
        .take()
        .expect("interactive stderr")
        .read_to_end(&mut stderr)
        .expect("read interactive errors");
    InteractiveOutput {
        status,
        stdout: received,
        stderr,
    }
}

fn wait_for_output(stdout: &mut BufReader<ChildStdout>, received: &mut Vec<u8>, expected: &[u8]) {
    let mut byte = [0];
    while !received.ends_with(expected) {
        let count = stdout.read(&mut byte).expect("read interactive output");
        assert!(
            count != 0,
            "interactive output ended before {:?}:\n{}",
            String::from_utf8_lossy(expected),
            String::from_utf8_lossy(received)
        );
        received.push(byte[0]);
    }
}
