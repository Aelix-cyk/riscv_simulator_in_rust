//! Test support: assemble and disassemble a `.S` fixture with the RISC-V toolchain.
//!
//! Include with `#[path = "support/assemble.rs"] mod assemble;`. The toolchain is
//! located through `$RISCV` when set, otherwise through `PATH`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

/// Address fixtures are linked at and loaded into memory.
pub const LOAD_ADDRESS: u64 = 0x8000_0000;

/// A flat image plus the toolchain's own disassembly of it.
pub struct Program {
    /// Bytes of the loadable text, in memory order.
    pub image: Vec<u8>,
    /// One entry per instruction, in the order `objdump` lists them.
    pub instructions: Vec<Disassembled>,
}

/// One `objdump -d` instruction line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Disassembled {
    /// Address of the instruction.
    pub address: u64,
    /// Mnemonic as `-M no-aliases` prints it, e.g. `addi`. Data decodes to
    /// `.word`.
    pub mnemonic: String,
    /// Operand text, e.g. `a1,a2,80000018 <done>`.
    pub operands: String,
}

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// Assembles `tests/fixtures/<name>` and returns its image and disassembly.
pub fn assemble(name: &str) -> Program {
    let work_dir = std::env::temp_dir().join(format!(
        "arvsr-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&work_dir).expect("create work directory");

    let source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name);
    let elf = work_dir.join("fixture.elf");
    let image = work_dir.join("fixture.bin");

    run(Command::new(tool("riscv64-unknown-elf-gcc"))
        .arg("-march=rv64i")
        .arg("-mabi=lp64")
        .arg("-nostdlib")
        .arg("-Wl,-e,_start")
        .arg(format!("-Wl,-Ttext={LOAD_ADDRESS:#x}"))
        .arg("-o")
        .arg(&elf)
        .arg(&source));
    run(Command::new(tool("riscv64-unknown-elf-objcopy"))
        .arg("-O")
        .arg("binary")
        .arg(&elf)
        .arg(&image));
    let disassembly = run_capture(
        Command::new(tool("riscv64-unknown-elf-objdump"))
            .arg("-d")
            .arg("--no-show-raw-insn")
            .arg("-M")
            .arg("no-aliases,numeric")
            .arg(&elf),
    );

    let program = Program {
        image: fs::read(&image).expect("read flat image"),
        instructions: parse_disassembly(&disassembly),
    };
    fs::remove_dir_all(&work_dir).ok();
    program
}

/// Resolves a toolchain program, honouring `$RISCV`.
fn tool(name: &str) -> PathBuf {
    match std::env::var_os("RISCV") {
        Some(root) if !root.is_empty() => Path::new(&root).join("bin").join(name),
        _ => PathBuf::from(name),
    }
}

/// Runs a command, panicking with its stderr when it fails.
fn run(command: &mut Command) -> Output {
    let output = command.output().unwrap_or_else(|error| {
        panic!("failed to run {command:?}: {error}");
    });
    assert!(
        output.status.success(),
        "{command:?} failed: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    );
    output
}

/// Runs a command and returns its stdout as text.
fn run_capture(command: &mut Command) -> String {
    String::from_utf8_lossy(&run(command).stdout).into_owned()
}

/// Parses `objdump -d` output, keeping instruction lines only.
fn parse_disassembly(output: &str) -> Vec<Disassembled> {
    output
        .lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let address = fields.next()?.strip_suffix(':')?;
            let address = u64::from_str_radix(address, 16).ok()?;
            let mnemonic = fields.next()?.to_owned();
            let operands = fields.next().unwrap_or_default().to_owned();

            Some(Disassembled {
                address,
                mnemonic,
                operands,
            })
        })
        .collect()
}
