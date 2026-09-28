# Memory model research — Spike, QEMU, and what S1 should do

Author: assistant · Date: 2026-09-28 · Question: does the memory model dominate performance, and
what should M1 build?

## 1. Spike 1.1.1-dev — source read on disk

**RAM is sparse pages inside a declared region.** `mem_t` holds
`std::map<reg_t, char*> sparse_memory_map` (`riscv/devices.h:56`); `mem_t::contents(addr)` looks up
the page number and `calloc`s a 4 KiB page on first touch (`riscv/devices.cc:137`); `load_store`
does a bounds check and a per-page copy (`riscv/devices.cc:116`). `make_mems` creates one region
per `--m` entry (`spike_main/spike.cc:261`), default 2 GiB at `0x80000000`.

**Speed comes from the MMU, not the page map.** `TLB_ENTRIES = 256` with three direct-mapped
TLBs — load, store, instruction (`riscv/mmu.h:393`); `access_tlb` indexes `vpn % TLB_ENTRIES`
(`riscv/mmu.h:346`); the load fast path requires a TLB hit plus an aligned or intrapage access
(`riscv/mmu.cc:282`). Misaligned data access is gated by `is_misaligned_enabled()` — the Zicclsm
switch this project pinned.

## 2. QEMU — docs and source fetched 2026-09-28

**Memory is an acyclic graph of `MemoryRegion`s**; leaves are RAM and MMIO, an `AddressSpace`
gives each CPU or device its view, and the graph is rendered into a **FlatView** for dispatch
(`docs/devel/memory.rst`). MMIO leaves are host callbacks (`->read()`, `->write()`), so devices
attach without the core knowing them.

**The hot path is cached in the softmmu TLB**, sized dynamically between a minimum and a maximum
per MMU index (`accel/tcg/cputlb.c:215-306`), holding host pointers and permission bits. The design
centre is many devices and many address spaces; nothing in it is tuned for one flat RAM.

## 3. Measurement — what each access path costs

Micro-benchmark, 20M 8-byte reads, random addresses across a 1 MiB window, `rustc -O`, this
machine, 2026-09-28:

| Access path | ns/access |
|---|---|
| `BTreeMap` page lookup once per access | 30.9 |
| same, page lookup per byte | 122.7 |
| flat `Vec` window | 1.20 |
| `BTreeMap` plus one-entry last-page cache | 28.7 |

Two caveats keep these honest. Random addresses are the pessimistic case: real traces are local,
where even a one-entry cache hits constantly and the gap to a flat window narrows. And this
measures the memory path alone — in a real interpreter, decode and dispatch cost more than either
figure.

## 4. Comparison

| Axis | Spike | QEMU | S1 as drafted |
|---|---|---|---|
| RAM | lazily allocated 4 KiB pages inside declared regions | RAMBlocks behind regions, FlatView dispatch | sparse 4 KiB pages where ELF segments land |
| Holes and devices | device address map beside `mem_t` | MMIO leaves in the region graph | none; unmapped access faults |
| Fast path | 256-entry direct-mapped TLB per access type | dynamically sized per-MMU-index TLB | one page lookup per access |
| Design centre | one hart, one profile — closest to us | boards, devices, many address spaces | smallest thing that faults honestly |

## 5. Recommendation

1. **Keep sparse 4 KiB pages in M1.** It is Spike's own representation, it faults honestly, and
   4 KiB is the unit Sv39 needs in S8.
2. **Make “one page lookup per access” an invariant.** The per-byte variant is four times slower
   and is exactly the kind of accident that survives review.
3. **Do not optimize in M1.** The whole S1 test set retires roughly 380k instructions and runs in
   well under a second either way. Performance earns a slice of its own when S9 boots Linux; the
   measured escape hatches are declared regions with lazy pages (Spike), an internal direct-mapped
   page cache, or a flat window over the dense part of RAM.
4. **Keep the door open without reshaping.** `Memory::add_region(base, size)` and an internal
   cache are additions to M1's module, not changes to its interface, which is why M1 can stay
   simple without building a dead end.
