# Implementation-Specific Choices

Choices the RISC-V specification and the RVA22S64 profile leave to whoever builds
the implementation, with our current answer. The profile's own requirement rows
live in `rva22s64-implementation-choices.md`; engineering decisions inside this
codebase (decode table, benchmarks, CLI shape) are recorded in `docs/notes/`.

Status: **Decided** (fixed or implemented), **Open** (nobody has chosen), **N/A**
(the row does not apply yet).

## 1. Reset state and hart configuration

| Choice the spec leaves open | Ours | Status |
|---|---|---|
| Hart count and topology | One hart, hart ID 0, supporting M and U privilege modes | Decided |
| Reset PC, the platform's boot address | `DRAM_BASE` (`0x8000_0000`); images link and load there | Decided |
| Reset value of GPRs and CSRs | All zero | Decided |
| IALIGN | 16; Zca provides compressed instructions, so branch and jump targets need only be two-byte aligned | Decided |
| Behaviour on a misaligned instruction address | Raise an instruction-address-misaligned exception when the PC or control-transfer target is not two-byte aligned | Decided |
| `misa` | WARL RV64IMAFDCBU: `MXL = 2`, with A, B, C, D, F, I, M, and U fixed; writes are accepted and ignored | Decided |
| `mvendorid`, `marchid`, `mimpid` | Read-only zero | Decided |
| Physical address width | Registers are 64-bit; the implemented physical space is the single DRAM region below | Decided |

## 2. Memory map, attributes, and cache

| Choice the spec leaves open | Ours | Status |
|---|---|---|
| DRAM base and size | `0x8000_0000`, 1 MiB — the spec fixes no addresses | Decided (will grow) |
| MMIO regions (UART, interrupt controller, debug module) | None | Open |
| PMA regions and attributes | The whole region is cacheable, coherent main memory | Open (implicit today) |
| PMP entries and granularity | None | Open |
| Misaligned access outside cacheable, coherent main memory | Main memory supports misaligned access (Zicclsm); no other regions exist | Open |
| Cache geometry and coherence | No cache model | Open — Zic64b constrains any later choice to 64-byte, naturally aligned blocks or a compatible longer/shorter scheme |
| Cache-block operations (Zicbom/bop/boz) | None | Open |

## 3. Traps, interrupts, counters, and timers

| Choice the spec leaves open | Ours | Status |
|---|---|---|
| Trap vector base and alignment | `mtvec` implemented in Direct mode; base writes are masked to 4-byte alignment. `stvec` remains absent | Decided for M-mode |
| Privilege transitions | `mret` enters the mode recorded in `mstatus.MPP`; M-mode CSR access from U-mode raises illegal-instruction | Decided |
| Interrupt controller | None | Open |
| Timer facility | None; the profile deliberately leaves Sstc optional | Open |
| Number of hardware performance counters | Three: `hpmcounter3`–`hpmcounter5`; counters 6–31 are read-only zero | Decided |
| Hardware performance events | One-hot selector bits for retired instructions, loads, stores, and branches/jumps; faulting attempts do not count | Decided |
| Base counters | Full 64-bit `mcycle` and `minstret`, with `cycle`, `time`, and `instret` aliases; `time` mirrors `mcycle` | Decided |
| Counter access control | `mcounteren` gates U-mode reads; `mcountinhibit` stops base and HPM counting | Decided |
| `WFI` and `PAUSE` behaviour | `WFI` not decoded yet; the profile allows `PAUSE` to execute as a NOP | Open |
| Behaviour on unimplemented opcodes | Illegal-instruction exception, delivered through `mtvec` | Decided |

### HPM event selector encoding

`mhpmevent3`–`mhpmevent5` use the same 4-bit WARL one-hot bitmask encoding.
Bits are independent, and any combination is legal:

| Bit | Retired event |
|---|---|
| 0 | Every successfully retired instruction |
| 1 | Loads, including floating loads and LR |
| 2 | Stores, including floating stores, SC, and AMOs |
| 3 | Branches and jumps, whether or not control flow changes |

A selector value of `0` disables its counter. Each configured counter increments
at most once per retired instruction even when several selected categories
match. Faulting or trapping attempts do not increment any HPM counter. Upper
selector bits are WARL-masked to zero, so the complete retained value space is
`0–15`; this platform-defined event mapping is declared in the ACT4
configuration's `HPM_EVENTS` list.

## 4. Floating point

| Choice the spec leaves open | Ours | Status |
|---|---|---|
| F/D register width and representation | 32 FPRs store 64-bit architectural bits; single-precision values are NaN-boxed | Decided |
| F/D rounding and flags | Project-owned integer-backed operations implement all five rounding modes and all five IEEE flags | Decided |
| Tininess and NaN policy | Spike-compatible tininess detection and canonical NaN results | Decided |
| F CSR visibility | `fflags`, `frm`, and `fcsr` are U-accessible only while `mstatus.FS` is enabled | Decided |
| `mstatus.FS` dirty updates | Precise updates whenever an FP instruction changes FPR or FCSR state | Decided |
| Compressed FP accesses | Zcd implements `c.fld`, `c.fsd`, `c.fldsp`, and `c.fsdsp` | Decided |

## 5. Debug

| Choice the spec leaves open | Ours | Status |
|---|---|---|
| Debug module and its debug-spec parameters (trigger count, DMI run-test/idle cycles, abstract-command timeouts) | Not implemented | Open |

## 6. Choices the profile names explicitly

Same rows as section 4 of `rva22s64-implementation-choices.md`, with our answer:

| Area | Ours |
|---|---|
| Cache geometry and coherence | No cache model — Open |
| PMP entries and granularity | None — Open |
| PMA regions and attributes | One implicit cacheable region — Open |
| Interrupt controller | None — Open |
| Timer facility | None — Open |
| Misaligned access outside main memory | No such regions yet — Open |
| Debug | Not implemented — Open |
| Multi-hart topology | Single hart — Decided for now |

## 7. Execution-environment interface

The spec leaves `ecall` and the surrounding ABI to the execution environment:

| Choice | Ours | Status |
|---|---|---|
| Program container | Flat binary, loaded at `DRAM_BASE` | Decided |
| Program start | Entry is the load address; the CLI sets the PC there | Decided |
| Halt and exit signalling | Halt self-loop, `jal x0, 0`; `ecall` enters the machine trap handler | Decided (temporary) |
| Host I/O | Production host I/O remains absent. Compliance harnesses detect test-only `tohost` PASS/FAIL stores | Decided for tests |

## 8. Optional extensions

`Zfh`, `V`, `Zkn`, `Zks`, `Sv48`, `Sv57`, `Svnapot`, `Ssu64xl`, `Sstc`,
`Sscofpmf`, `Zkr`, and `H` (augmented) are still **Undecided** in the profile
document; nothing here changes that yet.
