# RVA22S64 Implementation Choices — Requirement Traceability Matrix

_Baseline:_ ratified RVA22 profiles release, Privileged Architecture 1.12

Implementation scope is closed. Optional capabilities not listed as
`Implemented` are permanently excluded from this simulator and are not deferred
future work.

Status values:

- `Fixed` — mandated by the profile; nothing to decide.
- `Implemented` — chosen and implemented.
- `Not implemented` — permanently excluded from the simulator's scope.
- `N/A` — the row does not apply to this configuration.

## 1. Mandatory requirements

### 1.1 Inherited from RVA22U64

RVA22S64 makes every RVA22U64 mandatory unprivileged extension
mandatory here by reference, so these rows gate conformance too.

| Clause | Requirement | Status | Notes |
|---|---|---|---|
| RVA22U64 | M — integer multiplication and division | Fixed | |
| RVA22U64 | A — atomic instructions | Fixed | |
| RVA22U64 | F — single-precision floating point | Fixed | Brings Zicsr with it |
| RVA22U64 | D — double-precision floating point | Fixed | |
| RVA22U64 | C — compressed instructions | Fixed | |
| RVA22U64 | Zicsr — CSR instructions | Fixed | Implied by F |
| RVA22U64 | Zicntr — base counters and timers | Fixed | |
| RVA22U64 | Ziccif — instruction fetch in cacheable, coherent main memory; naturally aligned fetches up to min(ILEN,XLEN) are atomic | Fixed | |
| RVA22U64 | Ziccrse — RsrvEventual in those regions | Fixed | |
| RVA22U64 | Ziccamoa — AMOArithmetic in those regions | Fixed | |
| RVA22U64 | Zicclsm — misaligned loads and stores supported in those regions | Fixed | May execute extremely slowly |
| RVA22U64 | Za64rs — reservation sets contiguous, naturally aligned, at most 64 bytes | Fixed | Tied to the 64-byte block size |
| RVA22U64 | B — bit manipulation | Implemented | Zba, Zbb, and Zbs |
| RVA22U64 | Zihpm — hardware performance counters | Implemented | Three event-selectable counters; the other 26 are read-only zero |
| RVA22U64 | Zihintpause — PAUSE hint | Implemented | Executes as a NOP |
| RVA22U64 | Zic64b — cache blocks 64 bytes, naturally aligned | Implemented | Architectural block size is 64 bytes |
| RVA22U64 | Zicbom — cache-block management operations | Implemented | Full-block check, then cacheless no-op |
| RVA22U64 | Zicbop — cache-block prefetch operations | Implemented | Untrapping hints; no cache effect |
| RVA22U64 | Zicboz — cache-block zero operations | Implemented | Preflight then clear the containing block |
| RVA22U64 | Zfhmin — half-precision transfer and convert | Implemented | Full Zfh also implemented |
| RVA22U64 | Zkt — data-independent execution time | Fixed | |

### 1.2 Added by RVA22S64

| Clause | Requirement | Status | Notes |
|---|---|---|---|
| RV64I base | 64-bit little-endian base ISA; `ecall` semantics as specified | Fixed | |
| Zifencei | Instruction-fetch fence | Fixed | Only standard icache-coherence hook |
| Ss1p12 | Privileged architecture version 1.12 | Fixed | |
| Svbare | `satp.MODE` = Bare | Fixed | |
| Sv39 | 39-bit page-based virtual memory | Implemented | Bare/Sv39, 4 KiB and superpages, 16-bit ASID |
| Svade | Page faults on clear A/D bits; no hardware A/D update | Implemented | |
| Ssccptr | Hardware page-table reads in cacheable, coherent main memory | Implemented | Page walks use the DRAM bus |
| Sstvecd | `stvec.MODE` = Direct; any valid four-byte-aligned `stvec.BASE` | Fixed | |
| Sstvala | `stval` contents on specified faults | Fixed | |
| Sscounterenw | Writable `scounteren` bits for non-zero `hpmcounter`s | Fixed | |
| Svpbmt | Page-based memory types | Implemented | PMA/default, non-cacheable, and I/O types; PBMT=3 faults |
| Svinval | Fine-grained address-translation cache invalidation | Implemented | `SINVAL.VMA`, `SFENCE.W.INVAL`, and `SFENCE.INVAL.IR` |

## 2. Optional unprivileged extensions

| Clause | Option | Status | Notes |
|---|---|---|---|
| RVA22U64 option | Zfh | Implemented | Half arithmetic, comparisons, classification, and conversions |
| RVA22U64 option | V | Not implemented | Permanently excluded |
| RVA22U64 option | Zkn | Not implemented | Permanently excluded |
| RVA22U64 option | Zks | Not implemented | Permanently excluded |
| Beyond RVA22U64 | Zacas | Not implemented | Compare-and-swap operations are permanently excluded |

## 3. Optional privileged extensions

| Clause | Option | Status | Notes |
|---|---|---|---|
| RVA22S64 option | Sv48 | Not implemented | Permanently excluded |
| RVA22S64 option | Sv57 | Not implemented | Permanently excluded |
| RVA22S64 option | Svnapot | Not implemented | Permanently excluded |
| RVA22S64 option | Ssu64xl | Implemented | UXLEN=64 hardwired; `sstatus.UXL` reads 2 |
| RVA22S64 option | Sstc | Not implemented | Permanently excluded |
| RVA22S64 option | Sscofpmf | Not implemented | Permanently excluded |
| RVA22S64 option | Zkr | Not implemented | Permanently excluded |
| RVA22S64 option | H (augmented) | Not implemented | Permanently excluded |

## 4. Platform choices outside the profile

| Area | Choice | Status | Notes |
|---|---|---|---|
| DRAM base and size | Configurable from the CLI | Implemented | Defaults to 128 MiB at `0x8000_0000`; validated to 4 KiB alignment, 57-bit physical addressing, and non-overlapping MMIO |
| Cache geometry and coherence | No cache model | Implemented | 64-byte, naturally aligned architectural blocks |
| PMP entries and granularity | No PMP entries | Not implemented | Permanently excluded |
| PMA regions and attributes | One implicit cacheable region accepting PBMT NC/I/O overrides | Implemented | |
| Interrupt controller | | Implemented | One-source PLIC with machine and supervisor contexts |
| Timer facility | | Implemented | 64-bit CLINT `mtime` and `mtimecmp` |
| Misaligned access outside cacheable, coherent main memory | No additional regions | Not implemented | Zicclsm main-memory support remains implemented |
| Debug (per debug specification) | | Implemented | No trigger module; trigger CSRs report zero triggers |
| Multi-hart topology | One hart | Not implemented | Permanently excluded |

## 5. Recommendations

| Clause | Recommendation | Status |
|---|---|---|
| RVA22S64 | Illegal-instruction exception on unimplemented opcodes | Decided | Routed through `mtvec`, or `stvec` when delegated |
