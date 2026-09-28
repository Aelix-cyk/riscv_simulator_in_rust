//! S1 differential harness — the verifier's gate for work order 001.
//!
//! This is a **separate step**, not part of `cargo test`'s ordinary run: every corpus test is
//! `#[ignore]`d, so a green unit suite can never be mistaken for acceptance (`docs/verification/`
//! `s1-surface.md` §3.1). Run it with:
//!
//!     cargo test --test s1_differential -- --ignored --test-threads=1
//!
//! The unit tests at the end of this file are *not* ignored: they exercise the normalizer, which
//! ADR 006 flags as trusted code, and the symbol reader, which is covered by synthetic images
//! built in memory. Nothing in the ordinary suite needs the oracle, the test set, or an
//! environment variable, so it runs in a bare checkout. One extra ignored test checks the same
//! reader against a real test binary when `$RISCV_TESTS_DIR` is set.
//!
//! Shape, per `docs/verification/s1-harness-design.md`: spike and the simulator are both driven as
//! subprocesses, black box, through the frozen CLI. Nothing here reaches into `src/**`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::thread::sleep;
use std::time::{Duration, Instant};

// --- the pinned configuration -------------------------------------------------------------

const SPIKE_BUILD: &str = "Spike RISC-V ISA Simulator 1.1.1-dev, riscv-isa-sim.git @ 8fc5ab03";
const ISA: &str = "rv64imafdc_zicsr_zifencei_zicntr_zihpm_zicclsm_zihintpause_zicbom_zicboz_zicbop_zca_zcd_zba_zbb_zbs_zfhmin_zaamo_zalrsc_svade_svpbmt_svinval";
const PRIV: &str = "msu";
const EXPECTED_TEST_SET: usize = 71;
const ORACLE_TIMEOUT: Duration = Duration::from_secs(120);
const SIMULATOR_TIMEOUT: Duration = Duration::from_secs(60);

/// The oracle binary: `$SPIKE`, falling back to `spike` on `PATH`.
fn spike() -> String {
    std::env::var("SPIKE").unwrap_or_else(|_| "spike".to_string())
}

/// The test set lives in `$RISCV_TESTS_DIR/isa`. There is no default on purpose: a wrong
/// environment fails loudly instead of quietly testing the wrong binaries.
fn test_set_dir() -> PathBuf {
    let root = std::env::var("RISCV_TESTS_DIR").unwrap_or_else(|_| {
        panic!(
            "RISCV_TESTS_DIR is not set: point it at a riscv-tests checkout, the directory that \
             contains isa/"
        )
    });
    Path::new(&root).join("isa")
}

/// Spike commits five instructions of its own boot ROM before the ELF entry point. Records below
/// this address belong to the ROM and are dropped (surface §5, rule 2).
const PROGRAM_BASE: u64 = 0x8000_0000;

// --- process plumbing ---------------------------------------------------------------------

fn scratch_dir() -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("target").join("s1-harness");
    fs::create_dir_all(&dir).expect("create the harness scratch directory");
    dir
}

fn test_set() -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(test_set_dir())
        .unwrap_or_else(|e| {
            panic!(
                "cannot read the test set directory {}: {e}",
                test_set_dir().display()
            )
        })
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().map(|kind| kind.is_file()).unwrap_or(false))
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        // The S1 test set is exactly these two families; the directory also holds 300-odd binaries
        // from other slices, source directories, and `.dump` companions (the raw glob yields 142
        // entries for 71 binaries).
        .filter(|name| {
            (name.starts_with("rv64ui-p-") || name.starts_with("rv64mi-p-")) && !name.contains('.')
        })
        .collect();
    names.sort();
    names
}

struct Run {
    status: ExitStatus,
    stdout: String,
}

/// Spawn, wait with a deadline, and capture the output through files — a pipe nobody drains can
/// deadlock the child.
fn run_captured(
    cmd: &mut Command,
    timeout: Duration,
    scratch: &Path,
    tag: &str,
) -> Result<Run, String> {
    let out_path = scratch.join(format!("{tag}.out"));
    let err_path = scratch.join(format!("{tag}.err"));
    let out =
        fs::File::create(&out_path).map_err(|e| format!("create {}: {e}", out_path.display()))?;
    let err =
        fs::File::create(&err_path).map_err(|e| format!("create {}: {e}", err_path.display()))?;
    cmd.stdout(Stdio::from(out)).stderr(Stdio::from(err));

    let described = format!("{cmd:?}");
    let mut child = cmd.spawn().map_err(|e| format!("could not spawn {described}: {e}"))?;
    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {}
            Err(e) => return Err(format!("waiting for {described}: {e}")),
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!("{described} did not finish within {timeout:?}"));
        }
        sleep(Duration::from_millis(20));
    };

    let stdout = fs::read_to_string(&out_path).unwrap_or_default();
    let _ = fs::remove_file(&out_path);
    let _ = fs::remove_file(&err_path);
    Ok(Run { status, stdout })
}

fn spike_command(elf: &Path, log: &Path) -> Command {
    let mut cmd = Command::new(spike());
    cmd.arg(format!("--isa={ISA}"))
        .arg(format!("--priv={PRIV}"))
        .arg("--triggers=0")
        .args(["-l", "--log-commits"])
        .arg(format!("--log={}", log.display()))
        .arg(elf);
    cmd
}

fn simulator_command(elf: &Path) -> Command {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let mut cmd = Command::new(cargo);
    cmd.current_dir(env!("CARGO_MANIFEST_DIR"))
        .arg("run")
        .arg("--release")
        .arg("--")
        .arg(elf)
        .arg("--trace");
    cmd
}

/// Echoed into every failure, so triage can tell a configuration mismatch from a simulator bug
/// without a re-run (review of 2026-09-26, §3.5).
fn describe_config(elf: &Path, log: &Path) -> String {
    format!(
        "pinned configuration (surface §2):\n  \
         {} --isa={ISA} --priv={PRIV} --triggers=0 -l --log-commits --log={} {}\n  \
         {SPIKE_BUILD}\n  \
         simulator: cargo run --release -- {} --trace",
        spike(),
        log.display(),
        elf.display(),
        elf.display()
    )
}

// --- the `tohost` symbol ------------------------------------------------------------------

/// Minimal ELF64 little-endian symbol lookup, returning `tohost`'s address. The harness must not
/// hardcode it: 69 binaries place it at `0x80001000` and two at `0x80002000`, so a hardcoded value
/// silently passes 69/71 (surface §6).
fn tohost_address(elf: &Path) -> Result<u64, String> {
    let bytes = fs::read(elf).map_err(|e| format!("read {}: {e}", elf.display()))?;
    let need = |ok: bool, what: &str| -> Result<(), String> {
        if ok {
            Ok(())
        } else {
            Err(format!("{}: truncated or malformed ELF ({what})", elf.display()))
        }
    };

    need(bytes.len() >= 64, "header")?;
    need(&bytes[0..4] == b"\x7fELF", "magic")?;
    need(bytes[4] == 2, "not ELF64")?;
    need(bytes[5] == 1, "not little-endian")?;

    let u16_at = |off: usize| -> Result<u16, String> {
        need(off + 2 <= bytes.len(), "u16")?;
        Ok(u16::from_le_bytes([bytes[off], bytes[off + 1]]))
    };
    let u32_at = |off: usize| -> Result<u32, String> {
        need(off + 4 <= bytes.len(), "u32")?;
        Ok(u32::from_le_bytes([
            bytes[off],
            bytes[off + 1],
            bytes[off + 2],
            bytes[off + 3],
        ]))
    };
    let u64_at = |off: usize| -> Result<u64, String> {
        need(off + 8 <= bytes.len(), "u64")?;
        let mut buf = [0u8; 8];
        buf.copy_from_slice(&bytes[off..off + 8]);
        Ok(u64::from_le_bytes(buf))
    };

    let sh_off = u64_at(0x28)? as usize;
    let sh_entsize = u16_at(0x3a)? as usize;
    let sh_num = u16_at(0x3c)? as usize;
    need(sh_entsize >= 64 && sh_num > 0, "no section headers")?;

    let mut symtab = None; // (offset, size, entry size)
    let mut strtab = None; // (offset, size)
    for index in 0..sh_num {
        let sh = sh_off + index * sh_entsize;
        if u32_at(sh + 4)? != 2 {
            continue; // SHT_SYMTAB
        }
        symtab = Some((u64_at(sh + 0x18)? as usize, u64_at(sh + 0x20)? as usize, u64_at(sh + 0x38)? as usize));
        let link = u32_at(sh + 0x28)? as usize;
        let str_sh = sh_off + link * sh_entsize;
        strtab = Some((u64_at(str_sh + 0x18)? as usize, u64_at(str_sh + 0x20)? as usize));
        break;
    }
    let (sym_off, sym_size, sym_entsize) = symtab.ok_or("no .symtab in the ELF")?;
    let (str_off, str_size) = strtab.ok_or(".symtab has no string table")?;
    need(sym_entsize >= 24, "bad symtab entry size")?;

    let strtab_bytes = bytes
        .get(str_off..str_off + str_size)
        .ok_or("string table lies outside the file")?;
    for index in 0..sym_size / sym_entsize {
        let sym = sym_off + index * sym_entsize;
        let name_off = u32_at(sym)? as usize;
        let name = match strtab_bytes.get(name_off..) {
            Some(rest) => match rest.iter().position(|&byte| byte == 0) {
                Some(end) => String::from_utf8_lossy(&rest[..end]).into_owned(),
                None => continue,
            },
            None => continue,
        };
        if name == "tohost" {
            return Ok(u64_at(sym + 8)?);
        }
    }
    Err(format!("{}: no `tohost` symbol", elf.display()))
}

// --- normalization (surface §5) -----------------------------------------------------------

enum Line {
    Commit {
        pc: String,
        insn: String,
        privilege: String,
        fields: Vec<String>,
    },
    Trap {
        name: String,
        epc: String,
    },
    /// The `tval` continuation line, folded into the trap record above it.
    TrapDetail(String),
    Ignore,
}

fn parse_line(line: &str) -> Line {
    let body = match line.split_once(':') {
        Some((_, rest)) => rest.trim_start(),
        None => return Line::Ignore,
    };

    if let Some(rest) = body.strip_prefix("exception ") {
        return match rest.split_once(", epc ") {
            // Spike prints `trap_illegal_instruction, epc …` and `interrupt #1, epc …`; §4's
            // grammar wants `illegal_instruction` and `interrupt#1`.
            Some((name, epc)) => {
                let joined = name.replace(' ', "");
                Line::Trap {
                    name: joined.strip_prefix("trap_").unwrap_or(&joined).to_string(),
                    epc: epc.trim().to_string(),
                }
            }
            None => Line::Ignore,
        };
    }
    if let Some(rest) = body.strip_prefix("tval ") {
        return Line::TrapDetail(rest.trim().to_string());
    }

    // A commit line is the only one whose first field is the privilege; disassembly lines start
    // with a PC and symbol lines with `>>>>`.
    let mut tokens = body.split_whitespace();
    let (Some(privilege), Some(pc), Some(insn)) = (tokens.next(), tokens.next(), tokens.next())
    else {
        return Line::Ignore;
    };
    if !privilege.chars().all(|c| c.is_ascii_digit())
        || !pc.starts_with("0x")
        || !insn.starts_with('(')
    {
        return Line::Ignore;
    }
    Line::Commit {
        pc: pc.to_string(),
        insn: insn.trim_matches(['(', ')']).to_string(),
        privilege: privilege.to_string(),
        fields: tokens.map(str::to_string).collect(),
    }
}

/// Rule 4: rewrite Spike's field tokens into the frozen `field=value` forms, keeping the order
/// Spike printed them in (register write, then memory or CSR; no S1 record has both).
fn rewrite_fields(fields: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    let mut index = 0;
    while index < fields.len() {
        let token = fields[index].as_str();
        let value = fields.get(index + 1).map(String::as_str);
        if token.starts_with('x') && token.len() > 1 {
            if let Some(value) = value {
                out.push(format!("{token}={value}"));
                index += 2;
                continue;
            }
        } else if token == "mem" {
            if let Some(addr) = value {
                if let Some(stored) = fields.get(index + 2) {
                    out.push(format!("mem {addr}={stored}"));
                    index += 3;
                    continue;
                }
                out.push(format!("mem {addr}"));
                index += 2;
                continue;
            }
        } else if let Some(csr) = token.strip_prefix('c') {
            if token.contains('_') {
                if let Some(value) = value {
                    out.push(format!("csr{csr}={value}"));
                    index += 2;
                    continue;
                }
            }
        }
        index += 1;
    }
    out
}

struct Reference {
    records: Vec<String>,
    /// The value stored to `tohost` by the halting store, if the log contains one.
    halt_tohost: Option<u64>,
}

fn normalize_reference(log: &str, tohost: u64) -> Reference {
    let mut records: Vec<String> = Vec::new();
    let mut halt_tohost = None;
    let mut reached_program = false;

    for raw in log.lines() {
        match parse_line(raw) {
            Line::Ignore => {}
            Line::TrapDetail(value) => {
                // Rule 3: fold the continuation line into the trap record above it.
                if let Some(last) = records.last_mut() {
                    if last.starts_with("trap ") && !last.contains(" tval=") {
                        *last = format!("{last} tval={value}");
                    }
                }
            }
            Line::Trap { name, epc } => {
                reached_program = true;
                records.push(format!("trap {name} epc={epc}"));
            }
            Line::Commit {
                pc,
                insn,
                privilege,
                fields,
            } => {
                let address = u64::from_str_radix(pc.trim_start_matches("0x"), 16).unwrap_or(0);
                if !reached_program && address < PROGRAM_BASE {
                    continue; // rule 2: Spike's boot ROM
                }
                reached_program = true;

                let rewritten = rewrite_fields(&fields);
                let mut record = format!("{pc} {insn} p{privilege}");
                for field in &rewritten {
                    record.push(' ');
                    record.push_str(field);
                }
                records.push(record);

                // Rule 5: stop after the first nonzero store to `tohost`.
                let halt_field = format!("mem 0x{tohost:016x}=0x");
                if let Some(field) = rewritten.iter().find(|field| field.starts_with(&halt_field)) {
                    let value = u64::from_str_radix(&field[halt_field.len()..], 16).unwrap_or(0);
                    if value != 0 {
                        halt_tohost = Some(value);
                        break;
                    }
                }
            }
        }
    }

    Reference { records, halt_tohost }
}

/// The simulator's own records, grammar-checked so a stub binary fails with a useful message
/// instead of a mysterious divergence at record 0.
fn our_records(stdout: &str) -> Result<Vec<String>, String> {
    let mut records = Vec::new();
    for (index, raw) in stdout.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        if !is_record(line) {
            return Err(format!(
                "output line {} is not a record in §4's grammar: `{line}`",
                index + 1
            ));
        }
        records.push(line.to_string());
    }
    Ok(records)
}

fn is_record(line: &str) -> bool {
    if let Some(rest) = line.strip_prefix("trap ") {
        let tokens: Vec<&str> = rest.split(' ').collect();
        if tokens.len() < 2 || tokens[1].starts_with("epc=0x") == false {
            return false;
        }
        return match tokens.len() {
            2 => true,
            3 => tokens[2].starts_with("tval=0x"),
            _ => false,
        };
    }

    let mut tokens = line.splitn(4, ' ');
    let (Some(pc), Some(insn), Some(privilege)) = (tokens.next(), tokens.next(), tokens.next())
    else {
        return false;
    };
    if !pc.starts_with("0x") || pc.len() != 18 {
        return false;
    }
    if !insn.starts_with("0x") || insn.len() != 10 {
        return false;
    }
    if !privilege.starts_with('p') || !privilege[1..].chars().all(|c| c.is_ascii_digit()) {
        return false;
    }
    match tokens.next() {
        None => true,
        Some(tail) => tail_is_valid(tail),
    }
}

fn tail_is_valid(tail: &str) -> bool {
    let mut tokens = tail.split(' ');
    while let Some(token) = tokens.next() {
        let is_value = token.contains('=');
        if is_value && (token.starts_with('x') || token.starts_with("csr")) {
            continue;
        }
        if token == "mem" {
            match tokens.next() {
                Some(address) if address.starts_with("0x") => continue,
                _ => return false,
            }
        }
        return false;
    }
    true
}

// --- the differential step ----------------------------------------------------------------

fn expected_exit(reference: &Reference) -> i32 {
    match reference.halt_tohost {
        Some(1) => 0,
        Some(value) => (value >> 1) as i32,
        None => panic!("the reference never stored a nonzero value to `tohost`"),
    }
}

fn record_at(records: &[String], index: usize) -> String {
    records
        .get(index)
        .cloned()
        .unwrap_or_else(|| "<no record: the stream ended here>".to_string())
}

fn divergence_message(
    binary: &str,
    config: &str,
    index: usize,
    ours: &[String],
    reference: &[String],
) -> String {
    let mut out = format!(
        "{binary}: first divergence at record {index} \
         (expected {} records, ours has {})\n\n  \
         ours  record {index}: {}\n  \
         spike record {index}: {}\n\npreceding records (ours / spike)\n",
        reference.len(),
        ours.len(),
        record_at(ours, index),
        record_at(reference, index)
    );
    for previous in index.saturating_sub(3)..index {
        out.push_str(&format!(
            "  {previous:>5}  {}\n         {}\n",
            record_at(ours, previous),
            record_at(reference, previous)
        ));
    }
    out.push('\n');
    out.push_str(config);
    out
}

fn differential(binary: &str) {
    let elf = test_set_dir().join(binary);
    assert!(elf.is_file(), "test binary not found: {}", elf.display());

    let scratch = scratch_dir();
    let log_path = scratch.join(format!("{binary}.log"));
    let config = describe_config(&elf, &log_path);

    // P3/P4: the oracle.
    let oracle = run_captured(
        &mut spike_command(&elf, &log_path),
        ORACLE_TIMEOUT,
        &scratch,
        &format!("{binary}.spike"),
    )
    .unwrap_or_else(|e| panic!("{binary}: could not run the oracle: {e}\n\n{config}"));
    assert!(
        oracle.status.success(),
        "{binary}: the oracle exited with {:?}; fix the environment before comparing\n\n{config}",
        oracle.status.code()
    );
    let log = fs::read_to_string(&log_path)
        .unwrap_or_else(|e| panic!("{binary}: the oracle wrote no log: {e}\n\n{config}"));

    // P4/P5: the halt address, then normalization.
    let tohost = tohost_address(&elf)
        .unwrap_or_else(|e| panic!("{binary}: cannot read the `tohost` symbol: {e}\n\n{config}"));
    let reference = normalize_reference(&log, tohost);
    assert!(
        !reference.records.is_empty(),
        "{binary}: normalization produced no records — the harness is wrong, not the simulator\n\n{config}"
    );

    // P6/P7: our own stream, grammar-checked rather than rewritten.
    let simulator = run_captured(
        &mut simulator_command(&elf),
        SIMULATOR_TIMEOUT,
        &scratch,
        &format!("{binary}.sim"),
    )
    .unwrap_or_else(|e| panic!("{binary}: could not run the simulator: {e}\n\n{config}"));
    let _ = fs::remove_file(&log_path);
    let ours = our_records(&simulator.stdout).unwrap_or_else(|e| {
        panic!("{binary}: the simulator did not produce a record stream: {e}\n\n{config}")
    });
    assert!(
        !ours.is_empty(),
        "{binary}: the simulator produced no records\n\n{config}"
    );

    // P8: the first divergence, with context.
    let shared = ours.len().min(reference.records.len());
    for index in 0..shared {
        if ours[index] != reference.records[index] {
            panic!(
                "{}",
                divergence_message(binary, &config, index, &ours, &reference.records)
            );
        }
    }
    if ours.len() != reference.records.len() {
        panic!(
            "{}",
            divergence_message(binary, &config, shared, &ours, &reference.records)
        );
    }

    // P9: the verdict.
    let want = expected_exit(&reference);
    let got = simulator.status.code().unwrap_or(-1);
    assert_eq!(
        got, want,
        "{binary}: exit status differs (the halting store carried tohost={:?})\n\n{config}",
        reference.halt_tohost
    );
}

/// One test per binary, so a failure names the binary without parsing output.
macro_rules! differential_tests {
    ($($name:ident => $binary:literal),* $(,)?) => {
        $(
            #[test]
            #[ignore = "the differential step: run with --ignored"]
            fn $name() {
                differential($binary);
            }
        )*
    };
}

differential_tests! {
    mi_breakpoint => "rv64mi-p-breakpoint",
    mi_csr => "rv64mi-p-csr",
    mi_illegal => "rv64mi-p-illegal",
    mi_instret_overflow => "rv64mi-p-instret_overflow",
    mi_ld_misaligned => "rv64mi-p-ld-misaligned",
    mi_lh_misaligned => "rv64mi-p-lh-misaligned",
    mi_lw_misaligned => "rv64mi-p-lw-misaligned",
    mi_ma_addr => "rv64mi-p-ma_addr",
    mi_ma_fetch => "rv64mi-p-ma_fetch",
    mi_mcsr => "rv64mi-p-mcsr",
    mi_pmpaddr => "rv64mi-p-pmpaddr",
    mi_sbreak => "rv64mi-p-sbreak",
    mi_scall => "rv64mi-p-scall",
    mi_sd_misaligned => "rv64mi-p-sd-misaligned",
    mi_sh_misaligned => "rv64mi-p-sh-misaligned",
    mi_sw_misaligned => "rv64mi-p-sw-misaligned",
    mi_zicntr => "rv64mi-p-zicntr",
    ui_add => "rv64ui-p-add",
    ui_addi => "rv64ui-p-addi",
    ui_addiw => "rv64ui-p-addiw",
    ui_addw => "rv64ui-p-addw",
    ui_and => "rv64ui-p-and",
    ui_andi => "rv64ui-p-andi",
    ui_auipc => "rv64ui-p-auipc",
    ui_beq => "rv64ui-p-beq",
    ui_bge => "rv64ui-p-bge",
    ui_bgeu => "rv64ui-p-bgeu",
    ui_blt => "rv64ui-p-blt",
    ui_bltu => "rv64ui-p-bltu",
    ui_bne => "rv64ui-p-bne",
    ui_fence_i => "rv64ui-p-fence_i",
    ui_jal => "rv64ui-p-jal",
    ui_jalr => "rv64ui-p-jalr",
    ui_lb => "rv64ui-p-lb",
    ui_lbu => "rv64ui-p-lbu",
    ui_ld => "rv64ui-p-ld",
    ui_ld_st => "rv64ui-p-ld_st",
    ui_lh => "rv64ui-p-lh",
    ui_lhu => "rv64ui-p-lhu",
    ui_lui => "rv64ui-p-lui",
    ui_lw => "rv64ui-p-lw",
    ui_lwu => "rv64ui-p-lwu",
    ui_ma_data => "rv64ui-p-ma_data",
    ui_or => "rv64ui-p-or",
    ui_ori => "rv64ui-p-ori",
    ui_sb => "rv64ui-p-sb",
    ui_sd => "rv64ui-p-sd",
    ui_sh => "rv64ui-p-sh",
    ui_simple => "rv64ui-p-simple",
    ui_sll => "rv64ui-p-sll",
    ui_slli => "rv64ui-p-slli",
    ui_slliw => "rv64ui-p-slliw",
    ui_sllw => "rv64ui-p-sllw",
    ui_slt => "rv64ui-p-slt",
    ui_slti => "rv64ui-p-slti",
    ui_sltiu => "rv64ui-p-sltiu",
    ui_sltu => "rv64ui-p-sltu",
    ui_sra => "rv64ui-p-sra",
    ui_srai => "rv64ui-p-srai",
    ui_sraiw => "rv64ui-p-sraiw",
    ui_sraw => "rv64ui-p-sraw",
    ui_srl => "rv64ui-p-srl",
    ui_srli => "rv64ui-p-srli",
    ui_srliw => "rv64ui-p-srliw",
    ui_srlw => "rv64ui-p-srlw",
    ui_st_ld => "rv64ui-p-st_ld",
    ui_sub => "rv64ui-p-sub",
    ui_subw => "rv64ui-p-subw",
    ui_sw => "rv64ui-p-sw",
    ui_xor => "rv64ui-p-xor",
    ui_xori => "rv64ui-p-xori",
}

// --- guards ------------------------------------------------------------------------------

/// Review §3.2: a test set that silently shrinks must be red, not green.
#[test]
#[ignore = "the differential step: run with --ignored"]
fn test_set_is_the_expected_71() {
    let names = test_set();
    assert_eq!(
        names.len(),
        EXPECTED_TEST_SET,
        "the test set is not the 71 binaries the surface is baselined against; a change here is a \
         baseline change, not a test fix\nfound: {names:?}"
    );
}

/// Review §3.3: two full passes over the whole set, streams compared, not counts.
#[test]
#[ignore = "the differential step: run with --ignored"]
fn determinism_full_test_set_twice() {
    let scratch = scratch_dir();
    let mut passes = Vec::new();
    for pass in 0..2 {
        let mut stream = Vec::new();
        for binary in test_set() {
            let elf = test_set_dir().join(&binary);
            let run = run_captured(
                &mut simulator_command(&elf),
                SIMULATOR_TIMEOUT,
                &scratch,
                &format!("{binary}.determinism{pass}"),
            )
            .unwrap_or_else(|e| panic!("{binary}: could not run the simulator: {e}"));
            let records = our_records(&run.stdout).unwrap_or_else(|e| {
                panic!("{binary}: the simulator did not produce a record stream: {e}")
            });
            stream.push((binary, run.status.code(), records));
        }
        passes.push(stream);
    }
    assert_eq!(
        passes[0], passes[1],
        "the simulator is not deterministic across two full passes"
    );
}

/// The mirror of the vacuous-green guard: a differ that can never say yes is as useless as one
/// that always does. This pins the normalizer's output for one binary against the numbers measured
/// when the surface was baselined, so a normalizer regression shows up as a red test rather than as
/// a corpus that mysteriously stops converging.
#[test]
#[ignore = "the differential step: run with --ignored"]
fn differ_self_check_rv64ui_p_add() {
    let scratch = scratch_dir();
    let elf = test_set_dir().join("rv64ui-p-add");
    let log_path = scratch.join("rv64ui-p-add.selfcheck.log");
    let oracle = run_captured(
        &mut spike_command(&elf, &log_path),
        ORACLE_TIMEOUT,
        &scratch,
        "rv64ui-p-add.selfcheck",
    )
    .expect("run the oracle");
    assert!(oracle.status.success(), "the oracle failed");
    let log = fs::read_to_string(&log_path).expect("read the oracle log");
    let _ = fs::remove_file(&log_path);

    let tohost = tohost_address(&elf).expect("the tohost symbol");
    let reference = normalize_reference(&log, tohost);

    assert_eq!(tohost, 0x8000_1000, "tohost moved: the test set changed");
    assert_eq!(
        reference.records.len(),
        511,
        "the reference for rv64ui-p-add is 511 records after normalization (surface §5)"
    );
    assert_eq!(
        reference.records.last().unwrap(),
        "0x0000000080000040 0xfc3f2223 p3 mem 0x0000000080001000=0x00000001",
        "the last compared record must be the halting store (surface §5, rule 5)"
    );
    assert_eq!(reference.halt_tohost, Some(1), "tohost carried the pass verdict");
    assert_eq!(expected_exit(&reference), 0, "a passing binary exits 0");
}

// --- unit tests for the trusted parts (these run under a plain `cargo test`) ---------------

/// Verbatim from `rv64ui-p-add`, log lines 1–15: the boot ROM, the ISA banner, and the first two
/// records of the program itself.
const FIXTURE_ROM_AND_ENTRY: &str = r#"core   0: 0x0000000000001000 (0x00000297) auipc   t0, 0x0
core   0: 3 0x0000000000001000 (0x00000297) x5  0x0000000000001000
core   0: 0x0000000000001004 (0x02028593) addi    a1, t0, 32
core   0: 3 0x0000000000001004 (0x02028593) x11 0x0000000000001020
core   0: 0x0000000000001008 (0xf1402573) csrr    a0, mhartid
core   0: 3 0x0000000000001008 (0xf1402573) x10 0x0000000000000000
core   0: 0x000000000000100c (0x0182b283) ld      t0, 24(t0)
core   0: 3 0x000000000000100c (0x0182b283) x5  0x0000000080000000 mem 0x0000000000001018
core   0: 0x0000000000001010 (0x00028067) jr      t0
core   0: 3 0x0000000000001010 (0x00028067)
core   0: >>>>  $xrv64i2p1_m2p0_a2p1_f2p2_d2p2_zicsr2p0_zifencei2p0_zmmul1p0_zaamo1p0_zalrsc1p0
core   0: 0x0000000080000000 (0x0500006f) j       pc + 0x50
core   0: 3 0x0000000080000000 (0x0500006f)
core   0: 0x0000000080000050 (0x00000093) li      ra, 0
core   0: 3 0x0000000080000050 (0x00000093) x1  0x0000000000000000
"#;

/// Verbatim from `rv64mi-p-illegal`, log lines 84–91: a CSR write, a trapping instruction with
/// the two-line exception form, and no commit record for the instruction that trapped.
const FIXTURE_TRAP: &str = r#"core   0: 3 0x00000000800000dc (0x01028293) x5  0x00000000800000e8
core   0: 0x00000000800000e0 (0x30529073) csrw    mtvec, t0
core   0: 3 0x00000000800000e0 (0x30529073) c773_mtvec 0x00000000800000e8
core   0: 0x00000000800000e4 (0x74445073) csrwi   mnstatus, 8
core   0: exception trap_illegal_instruction, epc 0x00000000800000e4
core   0:           tval 0x0000000074445073
core   0: 0x00000000800000e8 (0x00000297) auipc   t0, 0x0
core   0: 3 0x00000000800000e8 (0x00000297) x5  0x00000000800000e8
"#;

/// Verbatim from `rv64ui-p-add`, log lines 1068–1080: the halt store, and three records that keep
/// running after it. Fifteen of the 71 references end past their own halt store.
const FIXTURE_HALT_AND_TAIL: &str = r#"core   0: 3 0x0000000080000008 (0x00800f93) x31 0x0000000000000008
core   0: 0x000000008000000c (0x03ff0863) beq     t5, t6, pc + 48
core   0: 3 0x000000008000000c (0x03ff0863)
core   0: >>>>  write_tohost
core   0: 0x000000008000003c (0x00001f17) auipc   t5, 0x1
core   0: 3 0x000000008000003c (0x00001f17) x30 0x000000008000103c
core   0: 0x0000000080000040 (0xfc3f2223) sw      gp, -60(t5)
core   0: 3 0x0000000080000040 (0xfc3f2223) mem 0x0000000080001000 0x00000001
core   0: 0x0000000080000044 (0x00001f17) auipc   t5, 0x1
core   0: 3 0x0000000080000044 (0x00001f17) x30 0x0000000080001044
core   0: 0x0000000080000048 (0xfc0f2023) sw      zero, -64(t5)
core   0: 3 0x0000000080000048 (0xfc0f2023) mem 0x0000000080001004 0x00000000
core   0: 0x000000008000004c (0xff1ff06f) j       pc - 0x10
"#;

/// Verbatim from `rv64ui-p-add`, log lines 1056–1066: a U-mode trap with no `tval` line, and no
/// halt store anywhere in the fragment.
const FIXTURE_ECALL_IN_U_MODE: &str = r#"core   0: 0x00000000800006a4 (0x00100193) li      gp, 1
core   0: 0 0x00000000800006a4 (0x00100193) x3  0x0000000000000001
core   0: 0x00000000800006a8 (0x05d00893) li      a7, 93
core   0: 0 0x00000000800006a8 (0x05d00893) x17 0x000000000000005d
core   0: 0x00000000800006ac (0x00000513) li      a0, 0
core   0: 0 0x00000000800006ac (0x00000513) x10 0x0000000000000000
core   0: 0x00000000800006b0 (0x00000073) ecall
core   0: exception trap_user_ecall, epc 0x00000000800006b0
core   0: >>>>  trap_vector
core   0: 0x0000000080000004 (0x34202f73) csrr    t5, mcause
core   0: 3 0x0000000080000004 (0x34202f73) x30 0x0000000000000008
"#;

fn normalized(log: &str, tohost: u64) -> Vec<String> {
    normalize_reference(log, tohost).records
}

#[test]
fn normalizer_drops_everything_but_commit_and_trap_records() {
    assert_eq!(
        normalized(FIXTURE_ROM_AND_ENTRY, 0x8000_1000),
        vec![
            "0x0000000080000000 0x0500006f p3".to_string(),
            "0x0000000080000050 0x00000093 p3 x1=0x0000000000000000".to_string(),
        ]
    );
}

#[test]
fn normalizer_folds_the_trap_continuation_line() {
    assert_eq!(
        normalized(FIXTURE_TRAP, 0x8000_1000),
        vec![
            "0x00000000800000dc 0x01028293 p3 x5=0x00000000800000e8".to_string(),
            "0x00000000800000e0 0x30529073 p3 csr773_mtvec=0x00000000800000e8".to_string(),
            "trap illegal_instruction epc=0x00000000800000e4 tval=0x0000000074445073".to_string(),
            "0x00000000800000e8 0x00000297 p3 x5=0x00000000800000e8".to_string(),
        ]
    );
}

#[test]
fn normalizer_truncates_at_the_halt_store_and_drops_what_follows() {
    assert_eq!(
        normalized(FIXTURE_HALT_AND_TAIL, 0x8000_1000),
        vec![
            "0x0000000080000008 0x00800f93 p3 x31=0x0000000000000008".to_string(),
            "0x000000008000000c 0x03ff0863 p3".to_string(),
            "0x000000008000003c 0x00001f17 p3 x30=0x000000008000103c".to_string(),
            "0x0000000080000040 0xfc3f2223 p3 mem 0x0000000080001000=0x00000001".to_string(),
        ]
    );
}

#[test]
fn normalizer_keeps_a_trap_without_tval_and_without_halt_store() {
    assert_eq!(
        normalized(FIXTURE_ECALL_IN_U_MODE, 0x8000_1000),
        vec![
            "0x00000000800006a4 0x00100193 p0 x3=0x0000000000000001".to_string(),
            "0x00000000800006a8 0x05d00893 p0 x17=0x000000000000005d".to_string(),
            "0x00000000800006ac 0x00000513 p0 x10=0x0000000000000000".to_string(),
            "trap user_ecall epc=0x00000000800006b0".to_string(),
            "0x0000000080000004 0x34202f73 p3 x30=0x0000000000000008".to_string(),
        ]
    );
}

/// Every line below is verbatim from a run of the pinned oracle; the expectation is §4's form.
#[test]
fn normalizer_rewrites_fields() {
    let cases: [(&str, Option<&str>); 7] = [
        (
            "core   0: 3 0x000000000000100c (0x0182b283) x5  0x0000000080000000 mem 0x0000000000001018",
            Some("0x000000000000100c 0x0182b283 p3 x5=0x0000000080000000 mem 0x0000000000001018"),
        ),
        (
            "core   0: 3 0x0000000080000040 (0xfc3f2223) mem 0x0000000080001000 0x00000001",
            Some("0x0000000080000040 0xfc3f2223 p3 mem 0x0000000080001000=0x00000001"),
        ),
        (
            "core   0: 3 0x00000000800000e0 (0x30529073) c773_mtvec 0x00000000800000e8",
            Some("0x00000000800000e0 0x30529073 p3 csr773_mtvec=0x00000000800000e8"),
        ),
        (
            "core   0: 3 0x00000000800001ac (0x30551573) x10 0x0000000080000004 c773_mtvec 0x00000000800001b8",
            Some("0x00000000800001ac 0x30551573 p3 x10=0x0000000080000004 csr773_mtvec=0x00000000800001b8"),
        ),
        (
            "core   0: 0 0x00000000800001b4 (0x00013703) x14 0x00ff00ff00ff00ff mem 0x0000000080002000",
            Some("0x00000000800001b4 0x00013703 p0 x14=0x00ff00ff00ff00ff mem 0x0000000080002000"),
        ),
        (
            "core   0: 0x0000000080000000 (0x0500006f) j       pc + 0x50",
            None,
        ),
        ("core   0: >>>>  reset_vector", None),
    ];

    for (line, expected) in cases {
        let got = match parse_line(line) {
            Line::Ignore => None,
            Line::Trap { .. } | Line::TrapDetail(_) => Some("trap".to_string()),
            Line::Commit {
                pc,
                insn,
                privilege,
                fields,
            } => {
                let mut record = format!("{pc} {insn} p{privilege}");
                for field in rewrite_fields(&fields) {
                    record.push(' ');
                    record.push_str(&field);
                }
                Some(record)
            }
        };
        assert_eq!(got.as_deref(), expected, "line: {line}");
    }
}

/// Reads a real test binary, so it needs `$RISCV_TESTS_DIR`. The synthetic fixtures below cover
/// the same code in the ordinary suite; this one keeps a real image in the picture.
#[test]
#[ignore = "needs $RISCV_TESTS_DIR; run with the differential step"]
fn tohost_is_read_from_the_symbol_table_not_hardcoded() {
    let address = |binary: &str| {
        tohost_address(&test_set_dir().join(binary)).expect("the tohost symbol")
    };
    assert_eq!(address("rv64ui-p-add"), 0x8000_1000);
    assert_eq!(address("rv64ui-p-ld_st"), 0x8000_2000);
    assert_eq!(address("rv64ui-p-ma_data"), 0x8000_2000);
}

// --- synthetic fixtures for the symbol reader ----------------------------------------------

/// A minimal ELF64 little-endian image built in memory: valid header, a null section, a symbol
/// table and a string table, with a `tohost` symbol when requested. No program headers, because
/// the symbol reader does not look at them.
fn synthetic_elf(tohost: Option<u64>) -> Vec<u8> {
    const EHDR: usize = 64;
    const SHDR: usize = 64;
    const SYM: usize = 24;
    let sym_off = EHDR + 3 * SHDR;
    let sym_size = if tohost.is_some() { 2 * SYM } else { SYM };
    let str_off = sym_off + sym_size;
    let strtab: &[u8] = b"\0tohost\0";
    let mut f = vec![0u8; str_off + strtab.len()];

    f[0..4].copy_from_slice(b"\x7fELF");
    f[4] = 2; // ELF64
    f[5] = 1; // little-endian
    f[6] = 1; // version
    f[0x10..0x12].copy_from_slice(&2u16.to_le_bytes()); // ET_EXEC
    f[0x12..0x14].copy_from_slice(&0xF3u16.to_le_bytes()); // EM_RISCV
    f[0x14..0x18].copy_from_slice(&1u32.to_le_bytes());
    f[0x18..0x20].copy_from_slice(&0x8000_0000u64.to_le_bytes());
    f[0x28..0x30].copy_from_slice(&(EHDR as u64).to_le_bytes()); // e_shoff
    f[0x34..0x36].copy_from_slice(&(EHDR as u16).to_le_bytes()); // e_ehsize
    f[0x3a..0x3c].copy_from_slice(&(SHDR as u16).to_le_bytes()); // e_shentsize
    f[0x3c..0x3e].copy_from_slice(&3u16.to_le_bytes()); // e_shnum

    let sh1 = EHDR + SHDR; // SHT_SYMTAB, linking section 2
    f[sh1 + 4..sh1 + 8].copy_from_slice(&2u32.to_le_bytes());
    f[sh1 + 0x18..sh1 + 0x20].copy_from_slice(&(sym_off as u64).to_le_bytes());
    f[sh1 + 0x20..sh1 + 0x28].copy_from_slice(&(sym_size as u64).to_le_bytes());
    f[sh1 + 0x28..sh1 + 0x2c].copy_from_slice(&2u32.to_le_bytes());
    f[sh1 + 0x38..sh1 + 0x40].copy_from_slice(&(SYM as u64).to_le_bytes());

    let sh2 = EHDR + 2 * SHDR; // SHT_STRTAB
    f[sh2 + 4..sh2 + 8].copy_from_slice(&3u32.to_le_bytes());
    f[sh2 + 0x18..sh2 + 0x20].copy_from_slice(&(str_off as u64).to_le_bytes());
    f[sh2 + 0x20..sh2 + 0x28].copy_from_slice(&(strtab.len() as u64).to_le_bytes());

    if let Some(value) = tohost {
        let sym1 = sym_off + SYM;
        f[sym1..sym1 + 4].copy_from_slice(&1u32.to_le_bytes()); // st_name -> "tohost"
        f[sym1 + 4] = 0x10; // STB_GLOBAL | STT_OBJECT
        f[sym1 + 6..sym1 + 8].copy_from_slice(&1u16.to_le_bytes()); // st_shndx
        f[sym1 + 8..sym1 + 16].copy_from_slice(&value.to_le_bytes());
    }
    f[str_off..].copy_from_slice(strtab);
    f
}

fn write_fixture(name: &str, bytes: &[u8]) -> PathBuf {
    let path = scratch_dir().join(name);
    fs::write(&path, bytes).expect("write the synthetic ELF");
    path
}

#[test]
fn symbol_reader_finds_tohost_in_a_synthetic_image() {
    let path = write_fixture("synthetic-tohost.elf", &synthetic_elf(Some(0x8000_2000)));
    assert_eq!(tohost_address(&path).expect("tohost is present"), 0x8000_2000);
}

#[test]
fn symbol_reader_reports_a_missing_tohost() {
    let path = write_fixture("synthetic-no-tohost.elf", &synthetic_elf(None));
    let error = tohost_address(&path).expect_err("a missing symbol must fail");
    assert!(error.contains("no `tohost` symbol"), "unexpected message: {error}");
}

#[test]
fn symbol_reader_rejects_a_truncated_image() {
    let bytes = synthetic_elf(Some(0x8000_1000));
    let path = write_fixture("synthetic-truncated.elf", &bytes[..100]);
    let error = tohost_address(&path).expect_err("a truncated image must fail");
    assert!(error.contains("truncated"), "unexpected message: {error}");
}
