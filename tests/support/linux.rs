//! Builds a pinned Linux Image and minimal initramfs for the end-to-end gate.

use std::collections::HashSet;
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

/// Stable BusyBox release used by the interactive UART gate.
pub const BUSYBOX_TAG: &str = "1_37_0";

/// Commit pointed to by [`BUSYBOX_TAG`].
pub const BUSYBOX_COMMIT: &str = "be7d1b7b1701d225379bc1665487ed0871b592a5";

const INIT_SOURCE: &str = "init.c";
const INIT_BINARY: &str = "init";
const INTERACTIVE_INIT_SOURCE: &str = "interactive_init.c";
const BUSYBOX_INIT_SOURCE: &str = "busybox_init";
const BUSYBOX_CONFIG_SOURCE: &str = "busybox.config";
const BUSYBOX_BINARY: &str = "busybox";
const BUSYBOX_APPLETS: [&str; 6] = ["sh", "echo", "cat", "ls", "uname", "poweroff"];
const BUSYBOX_OLDCONFIG_DEFAULT_BYTES: usize = 4 << 10;
const CONFIG_FRAGMENT: &str = "arvsr.config";
const INITRAMFS_FILE_LIST: &[u8] = b"init\n";
const INTERACTIVE_INITRAMFS_FILE_LIST: &[u8] = b"init\n";
const EXECUTABLE_MODE: u32 = 0o755;
const MAX_KNOWN_LLVM_VERSION: u32 = 32;

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// Built Linux kernel and initramfs inputs.
pub struct LinuxFixture {
    pub kernel: PathBuf,
    pub initrd: PathBuf,
    work_dir: PathBuf,
}

/// Initramfs images for the automated interactive UART checks.
pub struct InteractiveFixtures {
    pub custom_initrd: PathBuf,
    pub busybox_initrd: PathBuf,
}

/// Checks interactive-only dependencies before an expensive Linux build.
pub fn validate_busybox_environment() {
    let checkout = required_dir("BUSYBOX_DIR");
    require_commit(&checkout, BUSYBOX_TAG, BUSYBOX_COMMIT, "BusyBox");
    let cross_compile = required_value("BUSYBOX_CROSS_COMPILE");
    assert!(
        tools_exist(&[format!("{cross_compile}gcc")]),
        "BUSYBOX_CROSS_COMPILE does not provide a gcc compiler"
    );
}

impl LinuxFixture {
    /// Builds Linux from a pinned local `LINUX_DIR` checkout.
    pub fn build() -> Self {
        let linux = required_dir("LINUX_DIR");
        require_commit(&linux, LINUX_TAG, LINUX_COMMIT, "Linux");

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

    /// Builds the custom and BusyBox initramfs images for the UART gate.
    pub fn build_interactive(&self) -> InteractiveFixtures {
        InteractiveFixtures {
            custom_initrd: self.build_custom_initramfs(),
            busybox_initrd: self.build_busybox_initramfs(),
        }
    }

    fn build_custom_initramfs(&self) -> PathBuf {
        let init = self.compile_freestanding_init(INTERACTIVE_INIT_SOURCE);
        let root = self.work_dir.join("interactive-initramfs");
        fs::create_dir_all(&root).expect("create interactive initramfs root");
        fs::copy(&init, root.join(INIT_BINARY)).expect("copy interactive init into initramfs");
        fs::set_permissions(
            root.join(INIT_BINARY),
            fs::Permissions::from_mode(EXECUTABLE_MODE),
        )
        .expect("make interactive init executable");

        let initrd = self.work_dir.join("interactive-initramfs.cpio");
        create_initramfs(&root, INTERACTIVE_INITRAMFS_FILE_LIST, &initrd);
        initrd
    }

    fn build_busybox_initramfs(&self) -> PathBuf {
        let busybox_checkout = required_dir("BUSYBOX_DIR");
        require_commit(&busybox_checkout, BUSYBOX_TAG, BUSYBOX_COMMIT, "BusyBox");

        let source = self.work_dir.join("busybox-source");
        let archive = self.work_dir.join("busybox.tar");
        fs::create_dir_all(&source).expect("create BusyBox source directory");
        run(Command::new("git")
            .arg("-C")
            .arg(&busybox_checkout)
            .args(["archive", "--format=tar", "--output"])
            .arg(&archive)
            .arg(BUSYBOX_TAG));
        run(Command::new("tar")
            .arg("-xf")
            .arg(&archive)
            .arg("-C")
            .arg(&source));

        let build = self.work_dir.join("busybox-build");
        fs::create_dir_all(&build).expect("create BusyBox build directory");
        let cross_compile = required_value("BUSYBOX_CROSS_COMPILE");
        run(busybox_make(&source, &build, &cross_compile).arg("allnoconfig"));
        replace_busybox_config(&build, &fixtures().join(BUSYBOX_CONFIG_SOURCE));
        let default_answers = self.work_dir.join("busybox-oldconfig-answers");
        fs::write(
            &default_answers,
            vec![b'\n'; BUSYBOX_OLDCONFIG_DEFAULT_BYTES],
        )
        .expect("write BusyBox oldconfig answers");
        run_with_stdin(
            busybox_make(&source, &build, &cross_compile).arg("oldconfig"),
            &default_answers,
        );
        run(busybox_make(&source, &build, &cross_compile)
            .arg(format!("-j{}", available_jobs()))
            .arg(BUSYBOX_BINARY));

        let binary = build.join(BUSYBOX_BINARY);
        assert!(binary.is_file(), "missing {}", binary.display());

        let root = self.work_dir.join("busybox-initramfs");
        let bin = root.join("bin");
        let dev = root.join("dev");
        fs::create_dir_all(&bin).expect("create BusyBox initramfs bin directory");
        fs::create_dir_all(&dev).expect("create BusyBox initramfs dev directory");
        fs::copy(fixtures().join(BUSYBOX_INIT_SOURCE), root.join(INIT_BINARY))
            .expect("copy BusyBox init script");
        fs::set_permissions(
            root.join(INIT_BINARY),
            fs::Permissions::from_mode(EXECUTABLE_MODE),
        )
        .expect("make BusyBox init script executable");
        fs::copy(&binary, bin.join(BUSYBOX_BINARY)).expect("copy BusyBox binary");
        fs::set_permissions(
            bin.join(BUSYBOX_BINARY),
            fs::Permissions::from_mode(EXECUTABLE_MODE),
        )
        .expect("make BusyBox binary executable");
        let mut file_list = b"init\nbin\ndev\nbin/busybox\n".to_vec();
        for applet in BUSYBOX_APPLETS {
            std::os::unix::fs::symlink(BUSYBOX_BINARY, bin.join(applet))
                .expect("create BusyBox applet link");
            file_list.extend_from_slice(format!("bin/{applet}\n").as_bytes());
        }

        let initrd = self.work_dir.join("busybox-initramfs.cpio");
        create_initramfs(&root, &file_list, &initrd);
        initrd
    }

    fn compile_freestanding_init(&self, source: &str) -> PathBuf {
        let fixtures = fixtures();
        let output = self
            .work_dir
            .join(source.strip_suffix(".c").unwrap_or(source));
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
            .arg(&output)
            .arg(fixtures.join(source)));
        output
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
    create_initramfs(&root, INITRAMFS_FILE_LIST, &initrd);
    initrd
}

fn create_initramfs(root: &Path, file_list: &[u8], initrd: &Path) {
    let output_file = File::create(initrd).expect("create initramfs archive");
    let mut child = Command::new("cpio")
        .args(["--create", "--format=newc", "--quiet"])
        .current_dir(root)
        .stdin(Stdio::piped())
        .stdout(Stdio::from(output_file))
        .stderr(Stdio::piped())
        .spawn()
        .expect("start cpio");
    child
        .stdin
        .take()
        .expect("cpio stdin")
        .write_all(file_list)
        .expect("write cpio file list");
    let output = child.wait_with_output().expect("wait for cpio");
    assert!(
        output.status.success(),
        "cpio failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn busybox_make(source: &Path, build: &Path, cross_compile: &str) -> Command {
    let mut command = Command::new("make");
    command
        .arg("-C")
        .arg(source)
        .arg(format!("O={}", build.display()))
        .arg("ARCH=riscv")
        .arg(format!("CROSS_COMPILE={cross_compile}"));
    command
}

fn replace_busybox_config(build: &Path, overrides: &Path) {
    let path = build.join(".config");
    let config = fs::read_to_string(&path).expect("read BusyBox configuration");
    let overrides_text =
        fs::read_to_string(overrides).expect("read BusyBox configuration overrides");
    let override_keys = overrides_text
        .lines()
        .filter_map(config_key)
        .collect::<HashSet<_>>();
    let mut updated = config
        .lines()
        .filter(|line| config_key(line).is_none_or(|key| !override_keys.contains(key)))
        .collect::<Vec<_>>()
        .join("\n");
    updated.push('\n');
    updated.push_str(&overrides_text);
    fs::write(path, updated).expect("configure BusyBox");
}

fn config_key(line: &str) -> Option<&str> {
    if let Some(disabled) = line.strip_prefix("# ") {
        disabled.strip_suffix(" is not set")
    } else {
        line.split_once('=').map(|(key, _)| key)
    }
}

fn run_with_stdin(command: &mut Command, input: &Path) -> Output {
    command.stdin(Stdio::from(
        File::open(input).expect("open command standard input"),
    ));
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
        .unwrap_or_else(|| panic!("{variable} must point to a source checkout"));
    PathBuf::from(value)
}

fn require_commit(root: &Path, tag: &str, expected: &str, project: &str) {
    let output = run(Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["rev-parse", &format!("{tag}^{{commit}}")]));
    let actual = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    assert_eq!(
        actual, expected,
        "{project} {tag} resolved to an unexpected commit"
    );
}

fn required_value(variable: &str) -> String {
    env::var(variable)
        .ok()
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| panic!("{variable} must name the RISC-V Linux cross compiler prefix"))
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
