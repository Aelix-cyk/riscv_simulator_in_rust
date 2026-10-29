# Bug 0028: ELF segment preflight is byte-at-a-time for large BSS images

| Field | Value |
|---|---|
| Found | 2026-10-29, code review of commit `5b9134d` |
| Fixed | 2026-10-29 |
| Affected | `src/elf.rs:126-131` (`ElfImage::load` preflight), `src/bus.rs:80-89` (default `check_write`) |

## Summary

`ElfImage::load` preflights each segment's full `p_memsz`:

```rust
bus.check_write(segment.address, segment.memory_size, MemoryType::Cacheable)
    .map_err(ElfError::Access)?;
```

`Platform` does not override `Bus::check_write`, so the default runs one
`read_u8` per byte. A valid ELF with a large BSS tail (tens of MiB) performs
millions of byte probes in a debug build before the image starts, adding
seconds of startup latency for no additional correctness.

The ELF parser already validates each segment inside the DRAM window, where the
platform memory is fully writable, so this preflight can be a single range
check.

## Requirement

No architectural rule mandates the probe width; this is a loader/performance
requirement. `Bus::check_write` documents that the default probes byte by
byte and that implementations with different read/write permissions must
override it:

> The default probes read access byte by byte. Implementations whose read and
> write permissions differ must override this method.

## Impact

Slow CLI startup and CI runs for legitimate large-BSS binaries; a 64 MiB BSS
means 64 million virtual calls. Correctness is unchanged, but the cost scales
with the image and is avoidable.

## Why it escaped

Tests use tiny fixtures and do not exercise a large BSS or measure load
latency; the default `check_write` is only tested on small widths.

## Suggested fix

Override `Platform::check_write`:

- If the whole `[address, address + width)` range is inside the effective DRAM
  window, return `Ok(())` after one checked range comparison.
- Otherwise fall back to the default byte-wise probe (or device-specific
  checks) so MMIO side effects and access faults are preserved.

Use the same DRAM bound as `read_u8`/`write_u8` (see bug 0025) and checked
`u64` arithmetic.

## Regression tests

- A segment with a multi-MiB BSS loads successfully and quickly.
- A segment crossing the DRAM end still fails preflight with `AccessFault`.
- MMIO/other regions retain byte-wise (or explicit) check semantics.
- `Platform::check_write` at the DRAM boundaries matches `write_u8` behaviour.

## Fix

The byte-wise default probe is now a shared helper used only as a fallback.
`Platform::check_write` recognizes a complete write span inside its effective
DRAM window in constant time and falls back to the shared probe for MMIO and
invalid ranges.

## Verification

- `loads_a_multi_megabyte_bss` loads a 4 MiB BSS through the optimized platform
  preflight.
- `dram_write_preflight_checks_whole_ranges_and_preserves_boundaries` covers a
  complete DRAM span and both invalid boundaries.
- Existing device and bus checks continue to pass.
- `cargo test`: 221 unit tests and all local integration tests pass.
- `cargo clippy --all-targets -- -D warnings`: clean.

## Prevention

Document `check_write` cost in the trait, override it for every bus with known
range semantics, and benchmark ELF loading with a large BSS fixture.
