//! Explicit external compliance gates. Run with:
//! `cargo test --test compliance -- --ignored --nocapture`

#[path = "support/compliance.rs"]
mod compliance;

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use compliance::{GateReport, expected_failures, required_dir, run_checked, run_elf};

const LEGACY_FAMILIES: [&str; 5] = ["rv64ui", "rv64um", "rv64ua", "rv64uc", "rv64mi"];

#[test]
#[ignore = "requires RISCV_TESTS_DIR and runs the external riscv-tests suite"]
fn riscv_tests_gate() {
    let root = required_dir("RISCV_TESTS_DIR");
    let expected = expected_failures();
    let mut report = GateReport::default();
    let filter = env::var("COMPLIANCE_FILTER").ok();
    let tests = select_legacy_tests(&root.join("isa"), filter.as_deref()).unwrap_or_else(|error| {
        panic!("{error}");
    });

    for path in tests {
        let name = test_name(&path);
        report.check(format!("riscv-tests/{name}"), run_elf(&path), &expected);
    }

    println!(
        "riscv-tests: {} passed, {} expected failures",
        report.passed.len(),
        report.expected_failures.len()
    );
    report.assert_clean("riscv-tests");
}

#[test]
#[ignore = "requires RISCV_ARCH_TEST_DIR and a complete ACT4 toolchain"]
fn act4_gate() {
    let root = required_dir("RISCV_ARCH_TEST_DIR");
    let project = Path::new(env!("CARGO_MANIFEST_DIR"));
    let config = project.join("tests/compliance/act4/test_config.yaml");
    let workdir = resolve_act4_workdir(
        project,
        &root,
        env::var_os("ACT4_WORKDIR").map(PathBuf::from),
    );

    let mut act = Command::new("mise");
    act.args(["exec", "--", "uv", "run", "act"])
        .arg(config)
        .arg("--workdir")
        .arg(&workdir)
        .arg("--fast");
    if let Some(extensions) = env::var_os("ACT4_EXTENSIONS") {
        act.arg("--extensions").arg(extensions);
    }
    run_checked(act.current_dir(&root));

    let elf_root = workdir.join("arvsr-rv64imac").join("elfs");
    let mut elfs = Vec::new();
    collect_elfs(&elf_root, &mut elfs);
    elfs.sort();
    assert!(!elfs.is_empty(), "ACT4 produced no ELF files");

    let expected = expected_failures();
    let mut report = GateReport::default();
    for elf in elfs {
        let relative = elf
            .strip_prefix(&elf_root)
            .expect("ELF under ACT4 output root");
        report.check(
            format!("act4/{}", relative.display()),
            run_elf(&elf),
            &expected,
        );
    }

    println!(
        "ACT4: {} passed, {} expected failures",
        report.passed.len(),
        report.expected_failures.len()
    );
    report.assert_clean("ACT4");
}

fn test_name(path: &Path) -> &str {
    path.file_name()
        .and_then(|name| name.to_str())
        .expect("UTF-8 test filename")
}

fn select_legacy_tests(isa_dir: &Path, filter: Option<&str>) -> Result<Vec<PathBuf>, String> {
    let mut selected = Vec::new();
    for family in LEGACY_FAMILIES {
        let prefix = format!("{family}-p-");
        let family_tests = fs::read_dir(isa_dir)
            .map_err(|error| format!("cannot read {}: {error}", isa_dir.display()))?
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| {
                        name.starts_with(&prefix)
                            && !name.ends_with(".dump")
                            && filter.is_none_or(|filter| name.contains(filter))
                    })
            })
            .collect::<Vec<_>>();
        if family_tests.is_empty() && filter.is_none() {
            return Err(format!("no prebuilt {family} tests found"));
        }
        selected.extend(family_tests);
    }
    selected.sort();
    if selected.is_empty() {
        return Err(match filter {
            Some(filter) => format!("COMPLIANCE_FILTER={filter:?} matched no tests"),
            None => "no prebuilt rv64 tests found".to_owned(),
        });
    }
    Ok(selected)
}

fn resolve_act4_workdir(
    project: &Path,
    arch_test_root: &Path,
    override_value: Option<PathBuf>,
) -> PathBuf {
    match override_value {
        Some(path) if path.is_absolute() => path,
        Some(path) => arch_test_root.join(path),
        None => project.join("target/compliance/act4"),
    }
}

fn collect_elfs(root: &Path, elfs: &mut Vec<PathBuf>) {
    for entry in
        fs::read_dir(root).unwrap_or_else(|error| panic!("cannot read {}: {error}", root.display()))
    {
        let path = entry.expect("directory entry").path();
        if path.is_dir() {
            collect_elfs(&path, elfs);
        } else if path.extension().is_some_and(|extension| extension == "elf") {
            elfs.push(path);
        }
    }
}

#[test]
fn relative_act4_workdir_uses_the_external_root_base() {
    let project = Path::new("project");
    let arch_test_root = Path::new("arch-test");

    assert_eq!(
        resolve_act4_workdir(project, arch_test_root, Some(PathBuf::from("work/act4"))),
        PathBuf::from("arch-test/work/act4")
    );
}

#[test]
fn compliance_filter_matching_no_tests_is_rejected() {
    let temp = env::temp_dir().join(format!("arvsr-empty-compliance-{}", std::process::id()));
    let isa_dir = temp.join("isa");
    fs::create_dir_all(&isa_dir).expect("create empty ISA directory");

    let error = select_legacy_tests(&isa_dir, Some("does-not-exist"))
        .expect_err("zero-match filter must fail");

    fs::remove_dir_all(&temp).ok();
    assert!(error.contains("matched no tests"), "{error}");
}
