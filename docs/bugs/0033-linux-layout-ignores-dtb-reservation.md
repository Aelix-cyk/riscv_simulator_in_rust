# Bug 0033: Linux layout does not reserve OpenSBI's fixed DTB window

| Field | Value |
|---|---|
| Found | 2026-11-02, code review of commit `f821649` |
| Affected | `src/boot.rs:18-21` (DTB constants), `:164-190` (`linux_layout`) |
| Status | Fixed |
| Fixed | 2026-11-02 |

## Summary

Generic OpenSBI relocates the FDT to the fixed `DEVICE_TREE_ADDRESS`
(`0x8220_0000`) and reserves `DEVICE_TREE_MAX_BYTES` (2 MiB), i.e.
`0x8220_0000..0x8240_0000`. `linux_layout` only checks the initrd and patched
DTB against `dram.end()`:

```rust
for (name, start, end) in [
    ("initramfs", initrd_start, initrd_end),
    ("device tree", device_tree_address, device_tree_end),
] {
    if end > dram.end() {
        return Err(BootError::BootImageOutsideDram { ... });
    }
}
```

It never excludes the fixed DTB target. A larger kernel whose effective end
passes `0x8220_0000`, or an initrd spanning that window, is overwritten when
OpenSBI relocates the FDT. The pinned image happens to leave the window inside
the 16 MiB growth gap, which hides the bug.

The ELF probe path already enforces the equivalent invariant
(`src/boot.rs:52`: next-stage `end > DEVICE_TREE_ADDRESS` is rejected); the
Linux layout lacks it.

## Requirement

OpenSBI's generic platform uses the fixed FDT relocation address and size
encoded by `DEVICE_TREE_ADDRESS` and `DEVICE_TREE_MAX_BYTES`
(`src/boot.rs:18-21`). The simulator's boot layout must treat that range as
reserved for firmware use. This is a boot-contract requirement, not an ISA
rule.

## Reproduction

1. Build a Linux `Image` whose effective end plus growth gap extends past
   `0x8220_0000`, or an initrd whose range includes
   `0x8220_0000..0x8240_0000`.
2. Boot with OpenSBI.

Expected: layout rejects the overlap (or relocates the initrd/DTB outside the
reserved window) before boot.
Actual: layout accepts it; OpenSBI overwrites the kernel/initrd with the FDT
during relocation.

## Impact

Valid-looking larger kernels or initrds are silently corrupted before Linux
starts, producing mysterious early-boot failures. Boot success depends on the
pinned image size rather than on the documented layout.

## Why it escaped

The pinned Linux fixture leaves the fixed window free inside the growth gap,
and there is no layout test with a kernel end or initrd range crossing
`DEVICE_TREE_ADDRESS`.

## Suggested fix

Reserve `[DEVICE_TREE_ADDRESS, DEVICE_TREE_ADDRESS + DEVICE_TREE_MAX_BYTES)` in
`linux_layout` and do one of:

- reject any kernel/initrd overlap with that range via a dedicated
  `BootRegionOverlap` error, or
- move the initrd (and the patched DTB) after the reserved window when it
  would overlap, if DRAM space permits.

Also keep the existing `DEVICE_TREE_MAX_BYTES` size check for the patched DTB.

## Regression tests

- Kernel effective end crossing `0x8220_0000` → layout error (or relocated
  layout with no overlap).
- Initrd range spanning `0x8220_0000..0x8240_0000` → layout error/relocation.
- Boundaries just below/at/above `0x8220_0000` and `0x8240_0000`.
- The pinned fixture still boots unchanged.

## Prevention

Model firmware, next-stage, kernel+growth, initrd, and FDT window as an
explicit reserved-region set and check all pairwise overlaps before loading.

## Fix

The Linux layout now treats
`[DEVICE_TREE_ADDRESS, DEVICE_TREE_END_ADDRESS)` as an OpenSBI-reserved
region. A Linux kernel whose effective memory range crosses the fixed window is
rejected with `BootRegionOverlap`. An initramfs whose normal placement would
land inside the window is moved to the first 4 KiB-aligned address at or after
the end of the reserved range. The patched DTB is then placed after the
initramfs with the existing alignment and bounds checks.

## Regression tests

- `boot::tests::linux_layout_places_initrd_after_the_dtb_window`
- `boot::tests::linux_layout_allows_kernel_ending_at_the_dtb_window`
- `boot::tests::linux_layout_rejects_kernel_crossing_the_dtb_window`
- `boot::tests::linux_layout_rejects_images_past_dram_end`

## Verification

The pinned Linux fixture still boots to `/init`, prints `ARVSR-LINUX-OK`, and
powers off cleanly after the reserved-window fix.
