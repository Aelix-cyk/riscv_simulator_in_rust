//! Shared host-side support for external RISC-V compliance suites.

#![allow(dead_code)]

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

use riscv_simulator_in_rust::bus::Bus;
use riscv_simulator_in_rust::machine::{Machine, StepOptions};
use riscv_simulator_in_rust::memory::{DRAM_BASE, Memory};

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// Result reported by one compliance ELF.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TestOutcome {
    Passed,
    Failed { tohost: u64 },
    TimedOut { steps: u64, pc: u64 },
}

impl TestOutcome {
    pub fn passed(&self) -> bool {
        matches!(self, Self::Passed)
    }
}

/// One expected failure manifest entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpectedFailure {
    pub category: String,
    pub reason: String,
}

/// Expected failures keyed by `<suite>/<test>`.
pub type ExpectedFailures = BTreeMap<String, ExpectedFailure>;

/// Resolves a required suite directory from the environment.
pub fn required_dir(variable: &str) -> PathBuf {
    let value = env::var_os(variable)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| panic!("{variable} must point to the compliance checkout"));
    PathBuf::from(value)
}

/// Resolves a RISC-V toolchain program, honouring `$RISCV`.
pub fn riscv_tool(name: &str) -> PathBuf {
    match env::var_os("RISCV") {
        Some(root) if !root.is_empty() => Path::new(&root).join("bin").join(name),
        _ => PathBuf::from(name),
    }
}

/// Loads and validates the checked-in expected-failure manifest.
pub fn expected_failures() -> ExpectedFailures {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("compliance")
        .join("expected_failures.tsv");
    let text = fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()));
    let mut entries = BTreeMap::new();

    for (index, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let fields: Vec<&str> = line.split('\t').collect();
        assert_eq!(
            fields.len(),
            3,
            "{}:{} must contain test, category, and reason",
            path.display(),
            index + 1
        );
        assert!(
            matches!(fields[1], "unsupported" | "known-bug"),
            "{}:{} has an invalid category {:?}",
            path.display(),
            index + 1,
            fields[1]
        );
        let previous = entries.insert(
            fields[0].to_owned(),
            ExpectedFailure {
                category: fields[1].to_owned(),
                reason: fields[2].to_owned(),
            },
        );
        assert!(
            previous.is_none(),
            "{}:{} duplicates {:?}",
            path.display(),
            index + 1,
            fields[0]
        );
    }
    entries
}

/// Runs one ELF to its `tohost` completion or a bounded step limit.
pub fn run_elf(elf: &Path) -> TestOutcome {
    let program = ExtractedProgram::new(elf);
    let mut memory = Memory::new();
    memory
        .load_bytes(DRAM_BASE, &program.binary)
        .unwrap_or_else(|trap| panic!("cannot load {}: {trap:?}", elf.display()));
    let mut machine = Machine::new(memory);
    machine.cpu_mut().set_pc(program.entry);

    const STEP_LIMIT: u64 = 100_000;
    for steps in 1..=STEP_LIMIT {
        machine.step(StepOptions::default());
        let tohost = machine
            .bus_mut()
            .read_u64(program.tohost)
            .unwrap_or_else(|trap| panic!("cannot read tohost for {}: {trap:?}", elf.display()));
        if tohost != 0 {
            return if tohost == 1 {
                TestOutcome::Passed
            } else {
                TestOutcome::Failed { tohost }
            };
        }
        if machine.halted() {
            return TestOutcome::Failed { tohost: 0 };
        }
        if steps == STEP_LIMIT {
            return TestOutcome::TimedOut {
                steps,
                pc: machine.cpu().pc(),
            };
        }
    }
    unreachable!()
}

/// Collects unexpected results for a suite gate.
#[derive(Default)]
pub struct GateReport {
    pub passed: Vec<String>,
    pub expected_failures: Vec<String>,
    pub unexpected_failures: Vec<String>,
    pub unexpected_passes: Vec<String>,
}

impl GateReport {
    pub fn check(&mut self, key: String, outcome: TestOutcome, expected: &ExpectedFailures) {
        match (outcome, expected.get(&key)) {
            (TestOutcome::Passed, None) => self.passed.push(key),
            (TestOutcome::Passed, Some(_)) => self.unexpected_passes.push(key),
            (TestOutcome::Failed { tohost }, Some(_)) => {
                self.expected_failures
                    .push(format!("{key} (tohost={tohost:#x})"));
            }
            (TestOutcome::TimedOut { steps, pc }, Some(_)) => {
                self.expected_failures
                    .push(format!("{key} (timeout after {steps} steps at {pc:#x})"));
            }
            (TestOutcome::Failed { tohost }, None) => {
                self.unexpected_failures
                    .push(format!("{key} (tohost={tohost:#x})"));
            }
            (TestOutcome::TimedOut { steps, pc }, None) => {
                self.unexpected_failures
                    .push(format!("{key} (timeout after {steps} steps at {pc:#x})"));
            }
        }
    }

    pub fn assert_clean(&self, suite: &str) {
        assert!(
            self.unexpected_failures.is_empty() && self.unexpected_passes.is_empty(),
            "{suite} compliance mismatch\nunexpected failures:\n{}\nunexpected passes:\n{}",
            self.unexpected_failures.join("\n"),
            self.unexpected_passes.join("\n")
        );
    }
}

struct ExtractedProgram {
    binary: Vec<u8>,
    entry: u64,
    tohost: u64,
    work_dir: PathBuf,
}

impl ExtractedProgram {
    fn new(elf: &Path) -> Self {
        let work_dir = env::temp_dir().join(format!(
            "arvsr-compliance-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&work_dir).expect("create compliance work directory");
        let binary_path = work_dir.join("test.bin");
        run(Command::new(riscv_tool("riscv64-unknown-elf-objcopy"))
            .arg("-O")
            .arg("binary")
            .arg(elf)
            .arg(&binary_path));

        let symbols = run_capture(
            Command::new(riscv_tool("riscv64-unknown-elf-nm"))
                .arg("-n")
                .arg(elf),
        );
        Self {
            binary: fs::read(&binary_path).expect("read extracted test binary"),
            entry: find_symbol(&symbols, "_start")
                .or_else(|| find_symbol(&symbols, "rvtest_entry_point"))
                .unwrap_or_else(|| panic!("missing _start or rvtest_entry_point symbol")),
            tohost: symbol_address(&symbols, "tohost"),
            work_dir,
        }
    }
}

impl Drop for ExtractedProgram {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.work_dir);
    }
}

fn symbol_address(symbols: &str, name: &str) -> u64 {
    find_symbol(symbols, name).unwrap_or_else(|| panic!("missing {name} symbol"))
}

fn find_symbol(symbols: &str, name: &str) -> Option<u64> {
    symbols.lines().find_map(|line| {
        let mut fields = line.split_whitespace();
        let address = fields.next()?;
        let _kind = fields.next()?;
        (fields.next()? == name).then(|| u64::from_str_radix(address, 16).expect("symbol address"))
    })
}

fn run(command: &mut Command) -> Output {
    let output = command
        .output()
        .unwrap_or_else(|error| panic!("failed to run {command:?}: {error}"));
    assert!(
        output.status.success(),
        "{command:?} failed: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    );
    output
}

fn run_capture(command: &mut Command) -> String {
    String::from_utf8_lossy(&run(command).stdout).into_owned()
}

/// Runs a command and panics with captured stderr when it fails.
pub fn run_checked(command: &mut Command) -> Output {
    run(command)
}
