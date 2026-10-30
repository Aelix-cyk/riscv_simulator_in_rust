//! Builds the pinned OpenSBI firmware and boot fixtures for integration tests.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

use riscv_simulator_in_rust::boot::NEXT_STAGE_ADDRESS;

/// Stable OpenSBI release used by the boot gate.
pub const OPENSBI_TAG: &str = "v1.9";

/// Commit pointed to by [`OPENSBI_TAG`].
pub const OPENSBI_COMMIT: &str = "cbf9f6734dd85a982c63e3cb5db7ffe09da839ca";

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// Highest versioned LLVM toolchain suffix searched by the test harness.
const MAX_KNOWN_LLVM_VERSION: u32 = 32;

/// Built firmware and next-stage inputs.
pub struct OpenSbiFixture {
    pub firmware: PathBuf,
    pub device_tree: PathBuf,
    pub kernel: PathBuf,
    work_dir: PathBuf,
}

impl OpenSbiFixture {
    /// Builds every input for the pinned OpenSBI boot test.
    pub fn build() -> Self {
        let opensbi = required_dir("OPENSBI_DIR");
        require_commit(&opensbi, OPENSBI_TAG, OPENSBI_COMMIT);

        let work_dir = env::temp_dir().join(format!(
            "arvsr-opensbi-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&work_dir).expect("create OpenSBI work directory");
        let source = work_dir.join("source");
        let archive = work_dir.join("opensbi.tar");
        fs::create_dir_all(&source).expect("create OpenSBI source directory");

        run(Command::new("git")
            .arg("-C")
            .arg(&opensbi)
            .args(["archive", "--format=tar", "--output"])
            .arg(&archive)
            .arg(OPENSBI_TAG));
        run(Command::new("tar")
            .arg("-xf")
            .arg(&archive)
            .arg("-C")
            .arg(&source));

        let build = work_dir.join("build");
        run(Command::new("make")
            .arg("-C")
            .arg(&source)
            .arg(format!("O={}", build.display()))
            .arg("PLATFORM=generic")
            .arg(format!("LLVM={}", llvm_override()))
            .arg("FW_JUMP=y")
            .arg(format!("FW_JUMP_ADDR={NEXT_STAGE_ADDRESS:#x}")));

        let firmware = build
            .join("platform")
            .join("generic")
            .join("firmware")
            .join("fw_jump.elf");
        assert!(firmware.is_file(), "missing {}", firmware.display());

        let fixtures = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join("opensbi");
        let device_tree = work_dir.join("qemu-virt.dtb");
        let dtc = env::var_os("DTC")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("dtc"));
        run(Command::new(dtc)
            .arg("-I")
            .arg("dts")
            .arg("-O")
            .arg("dtb")
            .arg("-o")
            .arg(&device_tree)
            .arg(fixtures.join("qemu-virt.dts")));

        let kernel = work_dir.join("probe.elf");
        run(Command::new(riscv_tool("riscv64-unknown-elf-gcc"))
            .arg("-march=rv64ima_zicsr")
            .arg("-mabi=lp64")
            .arg("-nostdlib")
            .arg(format!("-Wl,-T,{}", fixtures.join("probe.ld").display()))
            .arg("-o")
            .arg(&kernel)
            .arg(fixtures.join("probe.S")));

        Self {
            firmware,
            device_tree,
            kernel,
            work_dir,
        }
    }
}

impl Drop for OpenSbiFixture {
    fn drop(&mut self) {
        if env::var_os("ARVSR_KEEP_OPENSBI").is_none() {
            fs::remove_dir_all(&self.work_dir).ok();
        }
    }
}

fn required_dir(variable: &str) -> PathBuf {
    let value = env::var_os(variable)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| panic!("{variable} must point to an OpenSBI checkout"));
    PathBuf::from(value)
}

fn require_commit(root: &Path, tag: &str, expected: &str) {
    let output = run(Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["rev-parse", &format!("{tag}^{{commit}}")]));
    let actual = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    assert_eq!(
        actual, expected,
        "OpenSBI {tag} resolved to an unexpected commit"
    );
}

fn riscv_tool(name: &str) -> PathBuf {
    match env::var_os("RISCV") {
        Some(root) if !root.is_empty() => Path::new(&root).join("bin").join(name),
        _ => PathBuf::from(name),
    }
}

fn llvm_override() -> String {
    if let Some(value) = env::var_os("LLVM").filter(|value| !value.is_empty()) {
        return value.to_string_lossy().into_owned();
    }

    if tools_exist(&["clang", "llvm-ar", "ld.lld", "llvm-objcopy"]) {
        return "1".to_owned();
    }

    for version in (1..=MAX_KNOWN_LLVM_VERSION).rev() {
        let suffix = format!("-{version}");
        let tools = [
            format!("clang{suffix}"),
            format!("llvm-ar{suffix}"),
            format!("ld.lld{suffix}"),
            format!("llvm-objcopy{suffix}"),
        ];
        if tools_exist(&tools) {
            return suffix;
        }
    }
    panic!("a complete LLVM/Clang toolchain with lld is required to build OpenSBI");
}

fn tools_exist(names: &[impl AsRef<str>]) -> bool {
    names.iter().all(|name| {
        Command::new(name.as_ref())
            .arg("--version")
            .output()
            .is_ok()
    })
}

fn run(command: &mut Command) -> Output {
    let output = command
        .output()
        .unwrap_or_else(|error| panic!("failed to run {command:?}: {error}"));
    assert!(
        output.status.success(),
        "{command:?} failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout).trim(),
        String::from_utf8_lossy(&output.stderr).trim()
    );
    output
}
