# RVA22S64 Implementation Choices — Requirement Traceability Matrix

_Baseline:_ ratified RVA22 profiles release, Privileged Architecture 1.12

Status values:

- `Fixed` — mandated by the profile; nothing to decide.
- `Implemented` — chosen and implemented.
- `Not implemented` — chosen and deliberately omitted; a conformant answer for
  an option.
- `Undecided` — nobody has chosen yet. A working state, not a final one.
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
| Sv39 | 39-bit page-based virtual memory | Fixed | |
| Svade | Page faults on clear A/D bits; no hardware A/D update | Fixed | |
| Ssccptr | Hardware page-table reads in cacheable, coherent main memory | Fixed | |
| Sstvecd | `stvec.MODE` = Direct; any valid four-byte-aligned `stvec.BASE` | Fixed | |
| Sstvala | `stval` contents on specified faults | Fixed | |
| Sscounterenw | Writable `scounteren` bits for non-zero `hpmcounter`s | Fixed | |
| Svpbmt | Page-based memory types | Fixed | |
| Svinval | Fine-grained address-translation cache invalidation | Fixed | |

## 2. Optional unprivileged extensions

| Clause | Option | Status | Notes |
|---|---|---|---|
| RVA22U64 option | Zfh | Implemented | Half arithmetic, comparisons, classification, and conversions |
| RVA22U64 option | V | Undecided | |
| RVA22U64 option | Zkn | Undecided | |
| RVA22U64 option | Zks | Undecided | |

## 3. Optional privileged extensions

| Clause | Option | Status | Notes |
|---|---|---|---|
| RVA22S64 option | Sv48 | Undecided | |
| RVA22S64 option | Sv57 | Undecided | |
| RVA22S64 option | Svnapot | Undecided | |
| RVA22S64 option | Ssu64xl | Undecided | |
| RVA22S64 option | Sstc | Undecided | Deliberately not mandatory: disruptive at system level |
| RVA22S64 option | Sscofpmf | Undecided | Platforms may mandate it |
| RVA22S64 option | Zkr | Undecided | |
| RVA22S64 option | H (augmented) | Undecided | |

## 4. Platform choices outside the profile

| Area | Choice | Status | Notes |
|---|---|---|---|
| Cache geometry and coherence | No cache model | Implemented | 64-byte, naturally aligned architectural blocks |
| PMP entries and granularity | | Undecided | |
| PMA regions and attributes | | Undecided | |
| Interrupt controller | | Undecided | |
| Timer facility | | Undecided | |
| Misaligned access outside cacheable, coherent main memory | | Undecided | Zicclsm already mandates main-memory support; this row covers other regions and performance |
| Debug (per debug specification) | | Undecided | |
| Multi-hart topology | | Undecided | |

## 5. Recommendations

| Clause | Recommendation | Status |
|---|---|---|
| RVA22S64 | Illegal-instruction exception on unimplemented opcodes | Decided | Routed through `mtvec`, or `stvec` when delegated |
