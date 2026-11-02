//! End-to-end OpenSBI-to-Linux boot gate.

#[path = "support/linux.rs"]
mod linux;
#[path = "support/opensbi.rs"]
mod opensbi;

use std::process::Command;

const DEFAULT_BOOT_TIMEOUT_SECONDS: u64 = 300;
const OPENSBI_BANNER: &str = "OpenSBI v1.9";
const LINUX_BANNER: &str = "Linux version ";
const PMU_INITIALIZED: &str = "riscv-pmu-sbi: SBI PMU extension is available";
const INIT_SUCCESS: &str = "ARVSR-LINUX-OK";
const BOOTARGS: &str = "console=ttyS0 earlycon=sbi";

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
