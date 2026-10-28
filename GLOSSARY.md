# Glossary

RISC-V ISA and hardware terms used throughout this simulator, grouped by
category and listed alphabetically within each section.

## Foundations & Formats

- **ABI name** — The conventional mnemonic (e.g. `sp`, `ra`, `a0`) used for a register by the standard calling convention instead of its `x`-number.
- **Base ISA** — The mandatory integer instruction set of RISC-V; for this project, RV64I.
- **Byte** — An 8-bit value, the smallest addressable unit of memory.
- **Doubleword** — A 64-bit value, eight consecutive bytes.
- **Extension** — An optional named set of instructions added on top of the base ISA, such as M, A, F, D, or C.
- **funct3** — A 3-bit field in most instruction encodings that selects the specific operation within an opcode.
- **funct7** — A 7-bit field that further distinguishes R-type operations sharing an opcode and funct3.
- **Halfword** — A 16-bit value, two consecutive bytes.
- **Hardware performance counter (HPM)** — A configurable CSR that counts a selected architectural event, such as retired instructions or memory accesses.
- **Immediate** — A constant value encoded directly inside an instruction rather than read from a register.
- **Instruction format** — One of the fixed bit layouts (R, I, S, B, U, J) that determines where fields and immediates sit in a 32-bit encoding.
- **ISA** — Instruction Set Architecture: the contract of instructions, registers, and memory behavior a processor implements.
- **Little-endian** — Byte ordering where the least-significant byte of a value is stored at the lowest address.
- **Opcode** — The field in an instruction that identifies the broad operation class.
- **Program counter (PC)** — The register holding the address of the instruction currently being executed.
- **Pseudo-instruction** — An assembly mnemonic that assemblers translate into one or more real instructions, e.g. `nop` for `addi x0, x0, 0`.
- **Register** — A small, fast, named storage location inside the CPU; RV64I has 32 general-purpose registers.
- **Register file** — The collection of all registers a hart can read and write directly.
- **RISC-V** — An open, modular, royalty-free instruction set architecture.
- **RV64I** — The 64-bit base integer ISA, the minimal instruction set this simulator targets, extended by the RVA22S64 profile.
- **Sv39** — RISC-V's 39-bit, three-level page-based virtual-memory scheme.
- **Svade** — A mode where invalid Accessed or Dirty PTE state raises page faults instead of being updated in hardware.
- **TLB** — Translation Lookaside Buffer: a cache of virtual-to-physical page mappings.
- **ASID** — Address Space Identifier, used to distinguish TLB entries belonging to different address spaces.
- **PTE** — Page Table Entry, describing a page-table link or leaf mapping and its permissions.
- **Sign-extension** — Widening a value by filling the new high bits with copies of its sign bit, preserving its two's-complement value.
- **Word** — A 32-bit value, four consecutive bytes.
- **x0** — The first integer register, hardwired to zero; writes are ignored and reads always yield 0.
- **XLEN** — The width of the integer registers and address space, 64 for RV64.

## RV64I Base Integer Instructions

- **ADD / SUB** — Register-register integer addition and subtraction.
- **ADDI** — Add an immediate value to a register.
- **ADDIW** — Add an immediate to a register and sign-extend the low 32 bits of the result.
- **ADDW / SUBW** — 32-bit addition and subtraction whose results are sign-extended to 64 bits.
- **AND / ANDI, OR / ORI, XOR / XORI** — Bitwise logical AND, OR, and XOR, in register and immediate forms.
- **AUIPC** — Add an immediate to the PC and store the result, used for PC-relative addressing.
- **BEQ / BNE** — Branch if two registers are equal, or not equal.
- **BGE / BGEU** — Branch if the first register is greater than or equal to the second, signed or unsigned.
- **BLT / BLTU** — Branch if the first register is less than the second, signed or unsigned.
- **EBREAK** — An instruction that raises a breakpoint exception to the debugging environment.
- **ECALL** — An instruction that raises an environment-call exception to request a service.
- **FENCE** — A memory-ordering instruction that constrains how loads and stores from other harts are observed.
- **JAL** — Jump and link: performs a PC-relative jump and writes the return address to a register.
- **JALR** — Jump and link register: jumps to a register plus immediate and writes the return address.
- **Load instructions (LB, LH, LW, LD, LBU, LHU, LWU)** — Load a byte, halfword, word, or doubleword from memory with signed or unsigned extension.
- **LUI** — Load upper immediate: places a 20-bit constant into the top of a register.
- **Store instructions (SB, SH, SW, SD)** — Write a byte, halfword, word, or doubleword to memory.
- **Shift instructions (SLL, SRL, SRA and immediate forms)** — Logical-left, logical-right, and arithmetic-right shifts.
- **Shift-word instructions (SLLW, SRLW, SRAW and immediate forms)** — Shifts that operate on the low 32 bits and sign-extend the result.
- **SLT / SLTU, SLTI / SLTIU** — Set a register to 1 if the first operand is less than the second, in signed or unsigned and register or immediate form.

## Standard Extensions

- **A extension** — Atomic memory operations for synchronization between harts.
- **B extension** — Bit manipulation, formed for this profile by Zba (address generation), Zbb (basic operations), and Zbs (single-bit operations).
- **AMO** — Atomic memory operation: an A-extension instruction that reads, modifies, and writes memory indivisibly.
- **C extension** — Compressed instructions: 16-bit encodings of common operations.
- **Cache-block operation (CBO)** — An explicit architectural memory operation that manages, prefetches, or zeroes a naturally aligned cache block.
- **D extension** — Double-precision (64-bit) IEEE-754 floating point.
- **F extension** — Single-precision (32-bit) IEEE-754 floating point.
- **Floating-point register file** — 32 separate registers (`f0`-`f31`) holding floating-point values in F/D code.
- **FLW / FSW** — Load and store single-precision floats between memory and the floating-point register file.
- **LR / SC** — Load-reserved and store-conditional, a paired primitive for atomic read-modify-write.
- **M extension** — Integer multiply and divide instructions.
- **MUL / MULH / DIV / REM** — Integer multiplication (low and high halves), division, and remainder, plus unsigned variants.
- **Rounding mode** — The field selecting how a floating-point result is rounded, e.g. round-to-nearest-even.
- **Zfh** — Half-precision (16-bit) IEEE-754 floating-point arithmetic, comparisons, conversions, and transfers.
- **Zfhmin** — The minimal half-precision subset: loads, stores, conversions, and integer-register moves.

## Privileged Architecture & CSRs

- **Control and Status Register (CSR)** — A dedicated register accessed by CSR instructions that configures or reports machine state.
- **Exception** — A synchronous trap caused by the executing instruction, such as an illegal opcode.
- **Interrupt** — An asynchronous trap raised by an external or timer event.
- **Machine mode (M-mode)** — The highest privilege level, with unrestricted access to the hardware.
- **mcause** — The CSR recording why the most recent trap occurred.
- **mepc** — The CSR holding the PC to resume at when returning from a trap.
- **mie / mip** — CSRs that enable interrupt sources and show which interrupts are pending.
- **mstatus** — The main machine-mode CSR holding global processor state and privilege bits.
- **mtvec** — The CSR holding the trap handler's base address and mode.
- **PMP** — Physical Memory Protection: optional per-region access control for memory.
- **Privilege level** — The current execution mode (M, S, or U) that determines which instructions and CSRs are legal.
- **satp** — The CSR controlling address translation and the page-table base.
- **Sret** — Return from a supervisor trap, restoring state from supervisor CSRs.
- **Supervisor mode (S-mode)** — A privilege level below M-mode that runs an operating system with a protected view of memory.
- **Trap** — The transfer of control to a handler caused by an exception or interrupt.
- **User mode (U-mode)** — The least-privileged level, where application code runs.
- **WFI** — Wait for interrupt: halt execution until an interrupt becomes pending.

## Simulator & Hardware Concepts

- **ALU** — Arithmetic Logic Unit: the circuit that performs integer arithmetic and logic.
- **Clock cycle** — One tick of the processor clock, the basic unit of simulated time.
- **Core** — The part of a CPU that executes instructions, roughly synonymous with a hart in this single-threaded simulator.
- **CPI** — Cycles per instruction: the average number of clock cycles each instruction takes.
- **CPU** — Central Processing Unit: the core, its registers, and the control logic that together execute instructions.
- **Datapath** — The network of registers, ALU, and buses along which data flows during execution.
- **Decode** — The pipeline stage that interprets instruction bits and decides what to do.
- **Execute** — The pipeline stage that performs the arithmetic, logic, or address computation.
- **Fetch** — The pipeline stage that reads the next instruction from memory at the PC.
- **Hazard / stall** — A dependency that would give a wrong result if instructions proceeded, resolved by pausing the pipeline.
- **Memory hierarchy** — The layered storage from fast small caches to slow large main memory.
- **Microarchitecture** — The concrete hardware design that implements an ISA.
- **MMIO** — Memory-mapped I/O: device registers accessed as ordinary memory addresses.
- **Pipeline** — An overlapping execution scheme where several instructions are in different stages at once.
- **Write-back** — The pipeline stage that commits a result to the register file or memory.
