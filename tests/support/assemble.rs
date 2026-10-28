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

use riscv_simulator_in_rust::bus::Bus;
use riscv_simulator_in_rust::cpu::{ABI_NAMES, FPR_COUNT, GPR_COUNT};
use riscv_simulator_in_rust::decode::is_compressed;

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
    spike_isa: Option<&'static str>,
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

/// Reads a 16- or 32-bit instruction word, zero-extending compressed words.
pub fn instruction_word(bus: &mut dyn Bus, address: u64) -> u32 {
    let low = bus.read_u16(address).expect("read instruction low half");
    if is_compressed(u32::from(low)) {
        u32::from(low)
    } else {
        bus.read_u32(address).expect("read 32-bit instruction")
    }
}

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
        for index in 0..FPR_COUNT {
            script.push_str(&format!("freg 0 {index}\n"));
        }
        for chunk in 0..chunks {
            script.push_str(&format!("mem 0 {:#x}\n", LOAD_ADDRESS + (chunk * 8) as u64));
        }
        script.push_str("reg 0 fcsr\n");
        script.push_str("quit\n");
        fs::write(&commands, script).expect("write spike debug commands");

        let mut spike = Command::new("timeout");
        spike.arg("30").arg(spike_tool());
        if let Some(isa) = self.spike_isa {
            spike.arg(format!("--isa={isa}"));
        }
        let output = run(spike
            .arg("-d")
            .arg(format!("--debug-cmd={}", commands.display()))
            .arg(&self.elf));
        // Spike writes debugger output to stderr, next to a `tohost` warning that
        // the parsers skip.
        let dump = String::from_utf8_lossy(&output.stderr);
        let scalars: Vec<u64> = dump.lines().filter_map(parse_scalar_hex).collect();
        let (floating_registers, memory, fcsr) = parse_float_state(&scalars, chunks);
        SpikeState {
            registers: parse_gpr_dump(&dump),
            floating_registers,
            fcsr,
            memory,
        }
    }
}

/// State spike reports for a fixture.
pub struct SpikeState {
    /// Final general-purpose register values.
    pub registers: [u64; GPR_COUNT],
    /// Final floating-point register bit patterns.
    pub floating_registers: [u64; FPR_COUNT],
    /// Final floating-point control and status register.
    pub fcsr: u64,
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
    assemble_with_march(name, "rv64ima_zifencei_zicsr")
}

/// Assembles a fixture with Zca enabled.
pub fn assemble_compressed(name: &str) -> Program {
    assemble_with_march(name, "rv64imac_zifencei_zicsr")
}

/// Assembles a fixture with the single-precision F extension enabled.
pub fn assemble_float(name: &str) -> Program {
    assemble_with_march(name, "rv64imaf_zifencei_zicsr")
}

/// Assembles a fixture with single- and double-precision floating point enabled.
pub fn assemble_double(name: &str) -> Program {
    assemble_with_march(name, "rv64imafdc_zifencei_zicsr")
}

/// Assembles a fixture with the B-extension subsets enabled.
pub fn assemble_bitmanip(name: &str) -> Program {
    assemble_with_march_and_isa(
        name,
        "rv64imac_zba_zbb_zbs_zicsr_zifencei",
        Some("rv64imafdc_zba_zbb_zbs_zicsr_zifencei"),
    )
}

/// Assembles a fixture with cache-block and pause hints enabled.
pub fn assemble_cache_blocks(name: &str) -> Program {
    assemble_with_march_and_isa(
        name,
        "rv64imac_zicbom_zicboz_zicbop_zihintpause_zicsr_zifencei",
        Some("rv64imafdc_zicbom_zicboz_zicbop_zihintpause_zicsr_zifencei"),
    )
}

/// Assembles a fixture for a selected ISA string.
fn assemble_with_march(name: &str, march: &str) -> Program {
    assemble_with_march_and_isa(name, march, None)
}

/// Assembles a fixture and optionally selects the Spike ISA explicitly.
fn assemble_with_march_and_isa(
    name: &str,
    march: &str,
    spike_isa: Option<&'static str>,
) -> Program {
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
        .arg(format!("-march={march}"))
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
        spike_isa,
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

/// Reads the ordered `freg`, memory, and FCSR scalar values.
fn parse_float_state(values: &[u64], chunks: usize) -> ([u64; FPR_COUNT], Vec<u8>, u64) {
    let mut registers = [0; FPR_COUNT];
    registers.copy_from_slice(&values[..FPR_COUNT]);
    let memory_end = FPR_COUNT + chunks;
    let mut memory = Vec::with_capacity(chunks * 8);
    for word in &values[FPR_COUNT..memory_end] {
        memory.extend_from_slice(&word.to_le_bytes());
    }
    let fcsr = values[memory_end];
    (registers, memory, fcsr)
}

/// Parses a one-token hexadecimal debugger value, retaining the low 64 bits.
fn parse_scalar_hex(line: &str) -> Option<u64> {
    let mut fields = line.split_whitespace();
    let word = fields.next()?.strip_prefix("0x")?;
    if fields.next().is_some()
        || word.is_empty()
        || !word.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return None;
    }
    let low = word.get(word.len().saturating_sub(16)..)?;
    u64::from_str_radix(low, 16).ok()
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
