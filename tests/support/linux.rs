//! Builds a pinned Linux Image and minimal initramfs for the end-to-end gate.

use std::env;
use std::fs::{self, File};
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

/// Stable Linux longterm release used by the boot gate.
pub const LINUX_TAG: &str = "v6.12.99";

/// Commit pointed to by [`LINUX_TAG`] in the stable Linux repository.
pub const LINUX_COMMIT: &str = "720e8bcaa674dfa40b989207d6cdb44b3aaa8db6";

const INIT_SOURCE: &str = "init.c";
const INIT_BINARY: &str = "init";
const CONFIG_FRAGMENT: &str = "arvsr.config";
const INITRAMFS_FILE_LIST: &[u8] = b"init\n";
const EXECUTABLE_MODE: u32 = 0o755;
const MAX_KNOWN_LLVM_VERSION: u32 = 32;

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// Built Linux kernel and initramfs inputs.
pub struct LinuxFixture {
    pub kernel: PathBuf,
    pub initrd: PathBuf,
    work_dir: PathBuf,
}

impl LinuxFixture {
    /// Builds Linux from a pinned local `LINUX_DIR` checkout.
    pub fn build() -> Self {
        let linux = required_dir("LINUX_DIR");
        require_commit(&linux, LINUX_TAG, LINUX_COMMIT);

        let work_dir = env::temp_dir().join(format!(
            "arvsr-linux-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&work_dir).expect("create Linux work directory");
        let source = work_dir.join("source");
        let archive = work_dir.join("linux.tar");
        let build = work_dir.join("build");
        fs::create_dir_all(&source).expect("create Linux source directory");

        run(Command::new("git")
            .arg("-C")
            .arg(&linux)
            .args(["archive", "--format=tar", "--output"])
            .arg(&archive)
            .arg(LINUX_TAG));
        run(Command::new("tar")
            .arg("-xf")
            .arg(&archive)
            .arg("-C")
            .arg(&source));

        let llvm = llvm_override();
        run(make(&source, &build, &llvm).arg("defconfig"));
        merge_config_fragment(&source, &build, &fixtures().join(CONFIG_FRAGMENT));
        run(make(&source, &build, &llvm).arg("olddefconfig"));
        run(make(&source, &build, &llvm)
            .arg(format!("-j{}", available_jobs()))
            .arg("Image"));

        let kernel = build.join("arch").join("riscv").join("boot").join("Image");
        assert!(kernel.is_file(), "missing {}", kernel.display());

        let initrd = build_initramfs(&work_dir);
        Self {
            kernel,
            initrd,
            work_dir,
        }
    }
}

impl Drop for LinuxFixture {
    fn drop(&mut self) {
        if env::var_os("ARVSR_KEEP_LINUX").is_none() {
            fs::remove_dir_all(&self.work_dir).ok();
        }
    }
}

fn merge_config_fragment(source: &Path, build: &Path, fragment: &Path) {
    run(Command::new("bash")
        .arg(
            source
                .join("scripts")
                .join("kconfig")
                .join("merge_config.sh"),
        )
        .arg("-m")
        .arg("-O")
        .arg(build)
        .arg(build.join(".config"))
        .arg(fragment)
        .current_dir(build));
}

fn build_initramfs(work_dir: &Path) -> PathBuf {
    let fixtures = fixtures();
    let init = work_dir.join(INIT_BINARY);
    run(Command::new(riscv_tool("riscv64-unknown-elf-gcc"))
        .arg("-march=rv64imac")
        .arg("-mabi=lp64")
        .arg("-mcmodel=medany")
        .arg("-O2")
        .arg("-nostdlib")
        .arg("-static")
        .arg("-fno-pie")
        .arg("-fno-stack-protector")
        .arg("-no-pie")
        .arg("-Wl,-e,_start")
        .arg("-o")
        .arg(&init)
        .arg(fixtures.join(INIT_SOURCE)));
    fs::set_permissions(&init, fs::Permissions::from_mode(EXECUTABLE_MODE))
        .expect("make init executable");

    let root = work_dir.join("initramfs");
    fs::create_dir_all(&root).expect("create initramfs root");
    fs::copy(&init, root.join(INIT_BINARY)).expect("copy init into initramfs");
    fs::set_permissions(
        root.join(INIT_BINARY),
        fs::Permissions::from_mode(EXECUTABLE_MODE),
    )
    .expect("make initramfs init executable");

    let initrd = work_dir.join("initramfs.cpio");
    let output_file = File::create(&initrd).expect("create initramfs archive");
    let mut child = Command::new("cpio")
        .args(["--create", "--format=newc", "--quiet"])
        .current_dir(&root)
        .stdin(Stdio::piped())
        .stdout(Stdio::from(output_file))
        .stderr(Stdio::piped())
        .spawn()
        .expect("start cpio");
    child
        .stdin
        .take()
        .expect("cpio stdin")
        .write_all(INITRAMFS_FILE_LIST)
        .expect("write cpio file list");
    let output = child.wait_with_output().expect("wait for cpio");
    assert!(
        output.status.success(),
        "cpio failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    initrd
}

fn make(source: &Path, build: &Path, llvm: &str) -> Command {
    let mut command = Command::new("make");
    command
        .arg("-C")
        .arg(source)
        .arg(format!("O={}", build.display()))
        .arg("ARCH=riscv")
        .arg(format!("LLVM={llvm}"));
    command
}

fn available_jobs() -> usize {
    env::var("LINUX_JOBS")
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|jobs| *jobs != 0)
        .unwrap_or_else(|| {
            std::thread::available_parallelism()
                .map(usize::from)
                .unwrap_or(1)
        })
}

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("linux")
}

fn required_dir(variable: &str) -> PathBuf {
    let value = env::var_os(variable)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| panic!("{variable} must point to a Linux checkout"));
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
        "Linux {tag} resolved to an unexpected commit"
    );
}

fn llvm_override() -> String {
    if let Some(value) = env::var_os("LLVM").filter(|value| !value.is_empty()) {
        return value.to_string_lossy().into_owned();
    }

    if tools_exist(&["clang", "llvm-ar", "ld.lld", "llvm-objcopy", "llvm-nm"]) {
        return "1".to_owned();
    }

    for version in (1..=MAX_KNOWN_LLVM_VERSION).rev() {
        let suffix = format!("-{version}");
        let tools = [
            format!("clang{suffix}"),
            format!("llvm-ar{suffix}"),
            format!("ld.lld{suffix}"),
            format!("llvm-objcopy{suffix}"),
            format!("llvm-nm{suffix}"),
        ];
        if tools_exist(&tools) {
            return suffix;
        }
    }
    panic!("a complete LLVM/Clang toolchain with lld is required to build Linux");
}

fn tools_exist(names: &[impl AsRef<str>]) -> bool {
    names.iter().all(|name| {
        Command::new(name.as_ref())
            .arg("--version")
            .output()
            .is_ok_and(|output| output.status.success())
    })
}

fn riscv_tool(name: &str) -> PathBuf {
    match env::var_os("RISCV") {
        Some(root) if !root.is_empty() => Path::new(&root).join("bin").join(name),
        _ => PathBuf::from(name),
    }
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
