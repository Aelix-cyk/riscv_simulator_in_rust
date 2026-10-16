//! Test support: assemble and disassemble a `.S` fixture with the RISC-V toolchain.
//!
//! Include with `#[path = "support/assemble.rs"] mod assemble;`. The toolchain is
//! located through `$RISCV` when set, otherwise through `PATH`.

// Shared by several integration tests; not every test uses every helper.
#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

use riscv_simulator_in_rust::cpu::{ABI_NAMES, GPR_COUNT};

/// Address fixtures are linked at and loaded into memory.
pub const LOAD_ADDRESS: u64 = 0x8000_0000;

/// A flat binary plus the toolchain's own disassembly of it.
pub struct Program {
    /// Bytes of the loadable text, in memory order.
    pub binary: Vec<u8>,
    /// One entry per instruction, in the order `objdump` lists them.
    pub instructions: Vec<Disassembled>,
    work_dir: PathBuf,
    elf: PathBuf,
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

impl Program {
    /// Address of the halt self-loop, `jal x0, 0`.
    pub fn halt_address(&self) -> u64 {
        self.instructions
            .iter()
            .find(|line| line.mnemonic == "jal" && branch_target(&line.operands) == line.address)
            .expect("fixture must end in a jal self-loop")
            .address
    }

    /// Bytes of memory the oracle compares: the binary rounded up to the eight
    /// bytes each `mem` command prints.
    pub fn memory_len(&self) -> usize {
        self.binary.len().div_ceil(8) * 8
    }

    /// Architectural state spike reports for this fixture: final registers plus
    /// the first `binary.len()` bytes of memory, rounded up to the eight bytes
    /// each `mem` command prints.
    ///
    /// Spike runs in debug mode and breaks at the halt self-loop, so the run is
    /// bounded by the program rather than by a timeout. `timeout` is still used as
    /// a guard against a fixture that never reaches its halt loop.
    pub fn spike_state(&self) -> SpikeState {
        let commands = self.work_dir.join("spike-debug.txt");
        let chunks = self.binary.len().div_ceil(8);
        let mut script = format!("until pc 0 {:#x}\nreg 0\n", self.halt_address());
        for chunk in 0..chunks {
            script.push_str(&format!("mem 0 {:#x}\n", LOAD_ADDRESS + (chunk * 8) as u64));
        }
        script.push_str("quit\n");
        fs::write(&commands, script).expect("write spike debug commands");

        let output = run(Command::new("timeout")
            .arg("30")
            .arg(spike_tool())
            .arg("-d")
            .arg(format!("--debug-cmd={}", commands.display()))
            .arg(&self.elf));
        // Spike writes debugger output to stderr, next to a `tohost` warning that
        // the parsers skip.
        let dump = String::from_utf8_lossy(&output.stderr);
        SpikeState {
            registers: parse_gpr_dump(&dump),
            memory: parse_memory_dump(&dump, chunks),
        }
    }
}

/// State spike reports for a fixture.
pub struct SpikeState {
    /// Final general-purpose register values.
    pub registers: [u64; GPR_COUNT],
    /// Final memory contents from `LOAD_ADDRESS`, eight bytes per `mem` line.
    pub memory: Vec<u8>,
}

impl Drop for Program {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.work_dir).ok();
    }
}

/// Assembles `tests/fixtures/<name>` and returns its binary and disassembly.
pub fn assemble(name: &str) -> Program {
    let work_dir = std::env::temp_dir().join(format!(
        "arvsr-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&work_dir).expect("create work directory");

    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures");
    let source = fixtures.join(name);
    let linker_script = fixtures.join("link.ld");
    let elf = work_dir.join("fixture.elf");
    let binary = work_dir.join("fixture.bin");

    run(Command::new(tool("riscv64-unknown-elf-gcc"))
        .arg("-march=rv64im_zifencei")
        .arg("-mabi=lp64")
        .arg("-nostdlib")
        .arg(format!("-I{}", fixtures.display()))
        .arg(format!("-Wl,-T,{}", linker_script.display()))
        .arg("-o")
        .arg(&elf)
        .arg(&source));
    run(Command::new(tool("riscv64-unknown-elf-objcopy"))
        .arg("-O")
        .arg("binary")
        .arg(&elf)
        .arg(&binary));
    let disassembly = run_capture(
        Command::new(tool("riscv64-unknown-elf-objdump"))
            .arg("-d")
            .arg("--no-show-raw-insn")
            .arg("-M")
            .arg("no-aliases")
            .arg(&elf),
    );

    Program {
        binary: fs::read(&binary).expect("read flat binary"),
        instructions: parse_disassembly(&disassembly),
        work_dir,
        elf,
    }
}

/// Resolves a toolchain program, honouring `$RISCV`.
fn tool(name: &str) -> PathBuf {
    match std::env::var_os("RISCV") {
        Some(root) if !root.is_empty() => Path::new(&root).join("bin").join(name),
        _ => PathBuf::from(name),
    }
}

/// Resolves spike, honouring `$SPIKE`.
fn spike_tool() -> PathBuf {
    match std::env::var_os("SPIKE") {
        Some(program) if !program.is_empty() => PathBuf::from(program),
        _ => PathBuf::from("spike"),
    }
}

/// Reads the branch or jump target `objdump` prints, e.g. `80000000 <_start>`.
pub fn branch_target(operands: &str) -> u64 {
    let last = operands.split(',').next_back().expect("target operand");
    let hex = last.split_whitespace().next().expect("target address");
    u64::from_str_radix(hex, 16).expect("hex target")
}

/// Reads spike's `reg 0` dump, e.g. `a0: 0x0000000000000037`.
fn parse_gpr_dump(output: &str) -> [u64; GPR_COUNT] {
    let tokens: Vec<&str> = output.split_whitespace().collect();
    let mut registers = [0; GPR_COUNT];
    let mut found = 0;

    for pair in tokens.windows(2) {
        let Some(name) = pair[0].strip_suffix(':') else {
            continue;
        };
        let Some(hex) = pair[1].strip_prefix("0x") else {
            continue;
        };
        let Some(index) = ABI_NAMES.iter().position(|abi| *abi == name) else {
            continue;
        };
        registers[index] = u64::from_str_radix(hex, 16).expect("hex register value");
        found += 1;
    }

    assert_eq!(found, GPR_COUNT, "spike printed {found} registers");
    registers
}

/// Reads spike's `mem` output: one 8-byte, little-endian hex word per chunk.
fn parse_memory_dump(output: &str, chunks: usize) -> Vec<u8> {
    let mut memory = Vec::with_capacity(chunks * 8);

    for line in output.lines() {
        let mut tokens = line.split_whitespace();
        let Some(word) = tokens.next().and_then(|token| token.strip_prefix("0x")) else {
            continue;
        };
        if tokens.next().is_some() || word.len() != 16 {
            continue; // register dump lines carry a `name:` prefix
        }
        let value = u64::from_str_radix(word, 16).expect("hex memory word");
        memory.extend_from_slice(&value.to_le_bytes());
    }

    assert_eq!(
        memory.len(),
        chunks * 8,
        "spike printed {} memory words, expected {chunks}",
        memory.len() / 8
    );
    memory
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
