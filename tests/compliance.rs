//! Explicit external compliance gates. Run with:
//! `cargo test --test compliance -- --ignored --nocapture`

#[path = "support/compliance.rs"]
mod compliance;

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use compliance::{GateReport, expected_failures, required_dir, run_checked, run_elf};

const LEGACY_FAMILIES: [&str; 4] = ["rv64ui", "rv64um", "rv64ua", "rv64mi"];

#[test]
#[ignore = "requires RISCV_TESTS_DIR and runs the external riscv-tests suite"]
fn riscv_tests_gate() {
    let root = required_dir("RISCV_TESTS_DIR");
    let expected = expected_failures();
    let mut report = GateReport::default();
    let filter = env::var("COMPLIANCE_FILTER").ok();

    for family in LEGACY_FAMILIES {
        let prefix = format!("{family}-p-");
        let mut tests = fs::read_dir(root.join("isa"))
            .unwrap_or_else(|error| panic!("cannot read {}: {error}", root.join("isa").display()))
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| {
                        name.starts_with(&prefix)
                            && !name.ends_with(".dump")
                            && filter.as_ref().is_none_or(|filter| name.contains(filter))
                    })
            })
            .collect::<Vec<_>>();
        tests.sort();
        if tests.is_empty() {
            assert!(filter.is_some(), "no prebuilt {family} tests found");
            continue;
        }

        for path in tests {
            let name = test_name(&path);
            report.check(format!("riscv-tests/{name}"), run_elf(&path), &expected);
        }
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
    let workdir = env::var_os("ACT4_WORKDIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| project.join("target/compliance/act4"));

    run_checked(
        Command::new("mise")
            .args(["exec", "--", "uv", "run", "act"])
            .arg(config)
            .arg("--workdir")
            .arg(&workdir)
            .arg("--fast")
            .current_dir(&root),
    );

    let elf_root = workdir.join("arvsr-rv64ima").join("elfs");
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
