# M1 — Skeleton specification

Status: **complete** — all nine sections approved 2026-09-28; awaiting the architect's sign-off on
the method itself.
Depends on: `docs/work-orders/001-s1-base.md` (method approval and acceptance bullets),
`docs/verification/s1-surface.md` (the records M1 must not emit yet).

## 1. Purpose and scope fence

M1 exists so every later method starts from a program that can already load a real test image,
place it in memory, find `tohost`, and end a run with the right exit status. Without that floor,
M2's decoder would be written against hypothetical inputs, and M5 would be hunting loader bugs and
instruction bugs in the same red run.

M1 writes no decoder, no CSR table, no instruction semantics, and no execution loop. Those are
M2, M3 and M4, and each needs its own approval before a line of it exists. The differential step
stays red through M1: it cannot go green before M5, so a red run here is the designed state, not
a defect to chase.

M1 is done on its own evidence — a clean release build, a green unit suite, the loader sweep over
all 71 images, and the CLI's exit-code cases — never on the differential run.

## 2. Module boundaries

| File | Owns | Never does |
|---|---|---|
| `main.rs` | argv → config, outcome → stdout/stderr/exit code | ELF or memory logic |
| `cli.rs` | argument grammar, usage text, flag errors | reading the file |
| `elf.rs` | ELF64-LE validation, `Image { entry, segments, tohost: Option<u64>, fromhost: Option<u64> }`, loader error taxonomy | allocating simulator memory |
| `memory.rs` | sparse 4 KiB pages, read/write/fault typing | knowing what an ELF is |
| `lib.rs` | module wiring and the run entry point | printing |

Why each fence exists:

- **`main.rs`** — loader code inside `main` can only be tested by spawning a process, and its
  failures arrive as panics instead of typed results. It also never prints a record: records are
  the trace emitter's job, and stdout carries nothing else.
- **`cli.rs`** — a bad flag and a bad ELF are different failures with different exits, and parsing
  that touches the filesystem forces every flag test to create a file.
- **`elf.rs`** — returning an `Image` rather than writing pages keeps the dependency arrow
  one-way and lets loader tests run without a memory instance.
- **`memory.rs`** — staying an address-to-bytes store with fault typing is what lets Sv39 in S8
  and MMIO later sit *above* it; segment or symbol knowledge would make every memory test need an
  ELF fixture.
- **`lib.rs`** — a library that prints pollutes the trace, and the surface's rule that stdout
  carries records and nothing else dies quietly.

Dependencies point one way: `main → lib → {cli, elf, memory}`. `elf` and `memory` never call each
other, and neither writes to a stream, so both are testable without a process. M2–M5 add files —
`decode.rs`, `hart.rs`, `csr.rs`, `execute.rs`, `trace.rs` — and may consume these interfaces, but
reshaping one needs the verifier and your sign-off, because later methods and the baseline both
lean on them. Standard library only, no new crates.

## 3. CLI contract

One accepted form for a run: `simulator <elf> --trace`. `--help` (or `-h`) prints the two usage
lines to stdout and exits 0. Anything else — unknown flag, missing ELF path, extra positional,
`--trace` absent — is a usage error: one usage line on stderr, exit 2.

Exit codes stay few on purpose: 0 and `n` belong to the verdict table of the surface, and every
failure that happens *before* a run exists — bad arguments, unreadable file, malformed ELF, no
`tohost` symbol — is 2. The stderr text distinguishes them; the number does not, so a harness can
never mistake a setup failure for a test failure.

M1 cannot execute, so a successful load ends with `M1: <elf> loaded, execution engine not
implemented` on stderr and exit 2. That is interim behaviour, replaced by M4 and M5; it exists so
the skeleton never claims exit 0 for a run that retired nothing. The exit table in the baseline is
unchanged, because no such run survives past M4.

Stdout carries records and nothing else — no banner, no version, no progress. In M1 it is empty.

## 4. ELF loader

`elf.rs` accepts ELF64, little-endian, `e_machine = 0xF3`, and reads `e_entry`, the program-header
table, and the section-header table. It returns
`Image { entry: u64, segments: Vec<Segment>, tohost: Option<u64>, fromhost: Option<u64> }`, where each
`Segment` already holds its final bytes: `p_filesz` copied from the file and the remaining
`p_memsz − p_filesz` zero-filled, which is what `.bss` needs. `e_type` is not checked: a PIE with
the same segments would load identically, and the test set is `ET_EXEC`.

Only `PT_LOAD` segments matter; they may appear in any number or order, and none is assumed to sit
at a fixed address. `tohost` and `fromhost` come from the symbol table — `SHT_SYMTAB`, 24-byte
entries, `st_name` resolved through the `sh_link` string table. Both are **optional**: they are
riscv-tests artifacts, and the OS images of S9 declare neither. The loader reports what the image
declares and never fails on their absence; what a missing `tohost` *means* is the caller's policy.
For the test set it is measured at `0x80001000` for 69 binaries and `0x80002000` for
`rv64ui-p-ld_st` and `rv64ui-p-ma_data`, so it is never hardcoded.

Every failure is one stderr line and exit 2:

| Condition | Line |
|---|---|
| unreadable or missing file | `cannot read <path>: <os error>` |
| magic mismatch | `<path>: not an ELF file` |
| wrong class, endianness or machine | `<path>: ELF64 little-endian RISC-V required` |
| header, table or segment beyond the file | `<path>: truncated <what>` |
| `p_filesz > p_memsz`, or offsets wrap | `<path>: segment <i> is inconsistent` |
| symbol table present but no `tohost` | not a loader error — see the policy below |

M1 has one mode, the HTIF verdict path, and it cannot judge a run without `tohost`. The run path
(`lib.rs`), not the loader, therefore reports `<path>: no tohost symbol` with exit 2 when the
symbol is absent. S9's boot mode inherits the same `Image` and simply does not need the symbol.

## 5. Memory model

`memory.rs` is a sparse map of 4 KiB pages, `BTreeMap<u64, Box<[u8; 4096]>>` — a B-tree rather
than a hash map so iteration order is deterministic. Page granularity is the same unit Sv39 will
need in S8, and it costs one comparison per access.

Access is little-endian at widths 1, 2, 4 and 8, assembled byte by byte, so an access may cross a
page boundary and may be misaligned. Misalignment is legal for data — Zicclsm requires it — and a
misaligned access that straddles two pages succeeds when both are mapped. Unmapped access returns
`Err(BusFault { addr, width })` and never panics; M4 turns that into the load or store access
fault with `mtval = addr`.

Faulting is all-or-nothing: a write checks that every byte's page is mapped before it mutates
one, so a fault can never leave half a value behind. Pages are created only by `load()`, which
places an ELF segment; a store to an unmapped address faults rather than allocating, because
auto-allocation would turn a runaway pointer into a silent success. The 71 test images need no
more: their `.bss` sits inside a `PT_LOAD` segment, and the test environment never touches a
stack.

There is no device or MMIO region in M1. The loader rejects a segment whose `p_memsz` exceeds
256 MiB, so a malformed image cannot exhaust host memory through the loader.

One invariant carries all the performance this model needs, and it is the one the research note
measures: exactly one page lookup per access, never one per byte — the per-byte variant is 4×
slower (122.7 ns against 30.9 ns for an 8-byte read). Beyond that, M1 does not optimize: the test
set retires about 380k instructions, so any access path here finishes in well under a second.

Comparison with the two reference implementations, with sources and measurements, is in
`docs/notes/memory-model-research.md`: Spike is sparse pages inside declared regions plus a
256-entry direct-mapped TLB per access type, QEMU is a `MemoryRegion` graph rendered into a
FlatView with a dynamically sized softmmu TLB. M1 is deliberately the smaller thing, and the two
measured escape hatches — declared regions with lazy pages, and an internal page cache — are
additions to `memory.rs` rather than changes to its interface.

## 6. Halt protocol

The only halting condition in M1's design is the HTIF verdict: a store that overlaps the 8-byte
window starting at `tohost`. After the store commits, the run path reads that window as a
little-endian `u64`; zero means "not a verdict" and execution would continue, any nonzero value
ends the run and becomes the exit status. Measured sizes and behaviour agree: `tohost` is an
8-byte symbol, riscv-tests writes `1` on pass and `(n << 1) | 1` on failure, and the store is
committed to the trace before the process exits.

| `tohost` value | Meaning | Exit |
|---|---|---|
| `1` | pass | 0 |
| `(n << 1) \| 1`, `n > 0` | test `n` failed | `n` |
| anything else nonzero | undefined by the test set; treated as `value >> 1` | `value >> 1` |

`fromhost` is located but never read in S1: no test in the set uses it.

What M1 can and cannot show. It can prove the symbol is found, typed as optional, and handed to
the run path — for 69 images at `0x80001000` and two at `0x80002000`. It cannot exercise the halt
because nothing retires, so the first real verdict arrives in M4, and the exit table above stays
unverified until M5 runs the differential step. M1 therefore ends every successful load with the
interim `not implemented` message and exit 2 described in §3, rather than inventing a verdict.

M1 also defines no instruction budget and no wall-clock limit. It needs neither, because it never
loops; converting a hung run into a failure is a stopping condition that belongs to M4, where
execution exists and the harness's own timeout is the backstop.

## 7. Interfaces M2–M5 will consume

Shapes are baselined; parameter names and internal style are the implementer's.

```rust
// elf.rs
pub struct Segment { pub addr: u64, pub bytes: Vec<u8> }   // already zero-filled to p_memsz
pub struct Image { pub entry: u64, pub segments: Vec<Segment>,
                   pub tohost: Option<u64>, pub fromhost: Option<u64> }
pub fn load(path: &Path) -> Result<Image, LoadError>;

// memory.rs
pub struct BusFault { pub addr: u64, pub width: u8 }
pub fn load(&mut self, addr: u64, bytes: &[u8]);              // infallible; allocates pages
pub fn read(&self, addr: u64, width: u8) -> Result<u64, BusFault>;
pub fn write(&mut self, addr: u64, width: u8, value: u64) -> Result<(), BusFault>;

// lib.rs
pub enum Outcome { Halt { exit_code: i32 }, NotExecutable }
pub fn run(config: &Config, records: &mut dyn Write) -> Result<Outcome, RuntimeError>;
```

Invariants later methods depend on, and may not quietly break:

- Segments are placed in file order, so a later segment overwrites an earlier overlapping one.
- `Memory` carries no knowledge of instructions, CSRs or modes; M4 decides fault *causes* from the
  call site, so `BusFault` stays a plain address and width.
- `run` writes records to the sink and nothing else; diagnostics travel as `RuntimeError` for
  `main` to print. This is what keeps stdout equal to the trace.
- `Outcome::NotExecutable` is M1's interim arm; M4 replaces it with real halting, and no other
  module may match on it by then.
- Reserved additions, not reshapes: `Memory::add_region(base, size)` for S9's RAM regions, an
  internal page cache for S8's TLB, and `trace.rs` emitting into the same sink M1 already passes.

## 8. Error and panic policy

Guest input never aborts the process. A malformed ELF, an unmapped address, a segment whose size
is absurd, a store with no verdict symbol — each is a typed failure with a one-line message and
exit 2, or a `BusFault` that M4 will turn into a trap. `panic!` and `unwrap()` are reserved for
violated invariants that indicate our own bug, and even those are meant to be caught by tests, not
by a runtime `catch_unwind`.

Concretely, in `lib.rs`, `cli.rs`, `elf.rs` and `memory.rs`:

- no `unwrap()` or `expect()`; `get` and `get_mut` return the fault instead of indexing;
- every `addr + width`, `p_offset + p_filesz` and `p_memsz` conversion uses checked arithmetic,
  and a wrap is a `BusFault` or a loader error, never a silent wrap;
- addresses are cast to `usize` only after a bounds check, so the truncation cannot happen;
- `debug_assert!` documents internal invariants and costs nothing in release;
- a write error on the record sink (a closed pipe, a full disk) becomes
  `cannot write trace: <os error>` and exit 2, rather than a panic mid-record.

`main.rs` is the one place allowed to exit with a message, and it never prints a record: records
go to stdout through the sink, diagnostics to stderr. Tests may `unwrap` freely — they are not
shipped. The verifier can check the no-`unwrap` rule with a text search over the four library
files, which makes it evidence rather than a promise.

## 9. Test obligations

| # | Obligation | Lives in | What it asserts | Satisfies |
|---|---|---|---|---|
| 1 | Loader sweep | `src` unit tests | 71 images found; every one loads; entry `0x80000000`; two `PT_LOAD` segments; `tohost` `0x80001000` for 69 and `0x80002000` for `ld_st` and `ma_data` | bullet 3 |
| 2 | Malformed images | `src` unit tests | Seven generated fixtures — missing file, bad magic, wrong class, truncated header, truncated segment, `p_filesz > p_memsz`, no `tohost` — each yields its exact message from §4 | bullet 3, §4 table |
| 3 | Memory invariants | `src` unit tests | allocation by `load`; misaligned intrapage read and write; access straddling two pages; unmapped read and write return `BusFault`; a store spanning mapped and unmapped pages leaves the mapped page byte-identical | bullet 4 |
| 4 | Sink purity | `src` unit tests | `run` on a real image writes zero bytes to a `Vec` sink and returns `NotExecutable` | bullets 1 and 3 |
| 5 | Argument parsing | `src` unit tests | pure `parse` on the four rejection cases and on `--help` | bullet 2, half |
| 6 | CLI process behaviour | `tests/` — verifier | spawning the release binary: exit 2 with one stderr line and empty stdout for the four rejection cases; `--help` exits 0 with two usage lines on stdout | bullet 2, half |
| 7 | Determinism | `src` unit tests | loading the same image twice yields byte-identical segments and symbols | bullet 1 |

Obligations 1–5 and 7 are the implementer's, inside `src/**` where internals are visible.
Obligation 6 is process-level and therefore the verifier's, in `tests/**`: it needs no oracle and
no trace, so it can go green during M1 while the differential step stays red — which is how the
CLI half of M1's acceptance becomes evidence instead of a claim.

Fixtures for obligation 2 are built in memory and written under `target/`, never committed as
binary files, so each broken image is reviewable as code.

Not tested in M1: execution, record emission, exit-status verdicts, and anything the differential
harness covers. That step stays red until M5, by design.
