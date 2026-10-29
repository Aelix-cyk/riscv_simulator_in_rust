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
| Hart count and topology | One hart, hart ID 0, supporting M, S, and U privilege modes | Decided |
| Reset PC, the platform's boot address | `DRAM_BASE` (`0x8000_0000`); images link and load there | Decided |
| Reset value of GPRs and CSRs | All zero | Decided |
| IALIGN | 16; Zca provides compressed instructions, so branch and jump targets need only be two-byte aligned | Decided |
| Behaviour on a misaligned instruction address | Raise an instruction-address-misaligned exception when the PC or control-transfer target is not two-byte aligned | Decided |
| `misa` | WARL RV64IMAFDCBSU: `MXL = 2`, with A, B, C, D, F, I, M, S, and U fixed; writes are accepted and ignored | Decided |
| `mvendorid`, `marchid`, `mimpid` | Read-only zero | Decided |
| Physical address width | Registers are 64-bit; the implemented physical space is the single DRAM region below | Decided |

## 2. Memory map, attributes, and cache

| Choice the spec leaves open | Ours | Status |
|---|---|---|
| DRAM base and size | `0x8000_0000`, 1 MiB — the spec fixes no addresses | Decided (will grow) |
| Page tables | Sv39 page tables are physically addressed and must reside in the cacheable initial DRAM region | Decided |
| MMIO regions (UART, interrupt controller, debug module) | Syscon, CLINT, PLIC, and NS16550-compatible UART; no debug module | Decided |
| PMA regions and attributes | The whole region is cacheable, coherent main memory, and accepts NC/I/O PBMT overrides | Decided |
| PMP entries and granularity | None | Open |
| Misaligned access outside cacheable, coherent main memory | Main memory supports misaligned access (Zicclsm); no other regions exist | Open |
| Cache geometry and coherence | No cache model; architectural cache blocks are 64 bytes and naturally aligned | Decided |
| Cache-block operations (Zicbom/bop/boz) | Management operations validate the full block and otherwise do nothing; `CBO.ZERO` preflights and clears the block; prefetches are untrapping hints | Decided |
| CBO privilege controls | `menvcfg.CBZE`, `CBCFE`, and WARL `CBIE` gate S-mode operations; both `menvcfg` and `senvcfg` gate U-mode. M-mode is always enabled | Decided |
| Misaligned LR/SC/AMO behavior | Raise load or store/AMO access faults, matching the configured Spike reference | Decided |

## 3. Traps, interrupts, counters, and timers

| Choice the spec leaves open | Ours | Status |
|---|---|---|
| Trap vector base and alignment | `mtvec` and `stvec` implement Direct mode; base writes are masked to 4-byte alignment | Decided |
| Privilege transitions | `mret` enters `mstatus.MPP`; delegated traps enter S mode; `sret` enters `sstatus.SPP`; unsupported transitions trap | Decided |
| Exception delegation | `medeleg` retains writable bits for implemented synchronous exceptions; traps originating in M mode are never delegated | Decided |
| Interrupt state | Machine and supervisor software, timer, and external pending bits feed the architectural interrupt priority and trap paths | Decided |
| Interrupt controller | PLIC with one UART receive source and separate machine/supervisor contexts | Decided |
| Timer facility | 64-bit CLINT `mtime`/`mtimecmp` drives the machine timer interrupt; Sstc remains optional | Decided |
| Number of hardware performance counters | Three: `hpmcounter3`–`hpmcounter5`; counters 6–31 are read-only zero | Decided |
| Hardware performance events | One-hot selector bits for retired instructions, loads, stores, and branches/jumps; faulting attempts do not count | Decided |
| Base counters | Full 64-bit `mcycle` and `minstret`, with `cycle`, `time`, and `instret` aliases; `time` mirrors `mcycle` | Decided |
| Counter access control | `mcounteren` gates S/U reads, and `scounteren` additionally gates U reads; `mcountinhibit` stops base and HPM counting | Decided |
| `WFI` and `PAUSE` behaviour | `WFI` stalls in M/S mode until any locally enabled interrupt is pending; it is illegal in U mode, and `mstatus.TW` makes S-mode `WFI` illegal. `PAUSE` executes as a no-op | Decided |
| Behaviour on unimplemented opcodes | Illegal-instruction exception, delivered through `mtvec`, or through `stvec` when delegated via `medeleg` | Decided |

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
| F/D/H register width and representation | 32 FPRs store 64-bit architectural bits; single- and half-precision values are NaN-boxed | Decided |
| F/D/H rounding and flags | Project-owned integer-backed operations implement all five rounding modes and all five IEEE flags | Decided |
| Tininess and NaN policy | Spike-compatible tininess detection and canonical NaN results | Decided |
| F CSR visibility | `fflags`, `frm`, and `fcsr` are U-accessible only while `mstatus.FS` is enabled | Decided |
| `mstatus.FS` dirty updates | Precise updates whenever an FP instruction changes FPR or FCSR state | Decided |
| Compressed FP accesses | Zcd implements `c.fld`, `c.fsd`, `c.fldsp`, and `c.fsdsp` | Decided |

## 5. Debug

| Choice the spec leaves open | Ours | Status |
|---|---|---|
| Debug module and its debug-spec parameters (trigger count, DMI run-test/idle cycles, abstract-command timeouts) | No trigger module; trigger CSRs remain accessible and report zero triggers | Decided |

## 6. Choices the profile names explicitly

Same rows as section 4 of `rva22s64-implementation-choices.md`, with our answer:

| Area | Ours |
|---|---|
| Cache geometry and coherence | No cache model; 64-byte, naturally aligned architectural blocks — Decided |
| PMP entries and granularity | None — Open |
| PMA regions and attributes | One implicit cacheable region accepting PBMT NC/I/O overrides — Decided |
| Interrupt controller | One-source PLIC for UART receive — Decided |
| Timer facility | 64-bit CLINT machine timer — Decided |
| Misaligned access outside main memory | No such regions yet — Open |
| Debug | No trigger module; trigger CSRs report zero triggers — Decided |
| Multi-hart topology | Single hart — Decided for now |

## 7. Execution-environment interface

The spec leaves `ecall` and the surrounding ABI to the execution environment:

| Choice | Ours | Status |
|---|---|---|
| Program container | Flat binary, loaded at `DRAM_BASE` | Decided |
| Program start | Entry is the load address; the CLI sets the PC there | Decided |
| Address translation | Fetch and memory accesses use one central Bare/Sv39 path with per-byte translation for cross-page accesses | Decided |
| Translation controls | `satp` supports Bare and Sv39 with a 16-bit ASID; `mstatus.TVM`, `SUM`, and `MXR` are writable; `SFENCE.VMA` and `SINVAL.VMA` honor ASID/VA forms | Decided |
| Memory types | `menvcfg.PBMTE` enables `Svpbmt`; each translated access carries PMA/default, non-cacheable, or I/O memory type into the bus | Decided |
| Fine-grained invalidation | `SINVAL.VMA`, `SFENCE.W.INVAL`, and `SFENCE.INVAL.IR` retain distinct invalidation and ordering roles | Decided |
| Translation cache | One unified, fully associative 64-entry TLB with FIFO replacement and global mappings | Decided |
| Halt and exit signalling | Syscon exit device or halt self-loop, `jal x0, 0`; `ecall` enters the machine trap handler | Decided |
| Host I/O | NS16550-compatible UART output and receive queue; CLI flushes buffered output at termination. Compliance harnesses still use test-only `tohost` PASS/FAIL stores | Decided |

## 8. Optional extensions

`V`, `Zkn`, `Zks`, `Sv48`, `Sv57`, `Svnapot`, `Ssu64xl`, `Sstc`,
`Sscofpmf`, `Zkr`, and `H` (augmented) are still **Undecided** in the profile
document; nothing here changes that yet.
