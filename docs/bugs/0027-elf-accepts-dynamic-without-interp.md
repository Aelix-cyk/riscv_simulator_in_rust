# Bug 0027: ELF loader accepts dynamic executables without `PT_INTERP`

| Field | Value |
|---|---|
| Found | 2026-10-29, code review of commit `5b9134d` |
| Fixed | 2026-10-29 |
| Affected | `src/elf.rs:55-62` (`ElfImage::parse`, line 60), contract at `docs/impl-choices/implement_specific_choices_list.md:117` |

## Summary

The program-header loop rejects only `PT_INTERP`:

```rust
match program.p_type(endian) {
    obj_elf::PT_INTERP => return Err(ElfError::DynamicExecutable),
    obj_elf::PT_LOAD => { ... }
    _ => {}
}
```

An `ET_EXEC` can be dynamically linked through a `PT_DYNAMIC` segment with
`DT_NEEDED` entries and no interpreter. Such images pass the type check and
are loaded without dependency resolution or relocations, even though the
decided contract says to reject dynamic executables.

## Requirement

Project execution-environment decision
(`docs/impl-choices/implement_specific_choices_list.md:117`):

> reject `PT_INTERP`, PIE, and dynamic executables

ELF gABI: `PT_DYNAMIC` identifies the segment holding dynamic-linking
information (including `DT_NEEDED` and relocation tables); its presence means
the image requires dynamic-linker processing. The project loads raw segments
only, so any such image is unsupported.

Against the code: only `PT_INTERP` triggers `DynamicExecutable`; a
`PT_DYNAMIC`-only executable is accepted.

## Impact

A dynamically linked executable can start at the wrong addresses or crash in
the loader/application because relocations were never applied and dependencies
were never mapped. Behaviour contradicts the documented CLI contract.

## Why it escaped

All fixtures are statically linked, and the negative test covers only
`PT_INTERP`; no image has `PT_DYNAMIC` without `PT_INTERP`.

## Suggested fix

Treat every dynamic-linking program header as unsupported:

```rust
obj_elf::PT_INTERP | obj_elf::PT_DYNAMIC => {
    return Err(ElfError::DynamicExecutable);
}
```

Keep `ET_EXEC`-only parsing for PIE rejection.

## Regression tests

- Minimal `ET_EXEC` with `PT_LOAD` plus `PT_DYNAMIC` and no `PT_INTERP` →
  `Err(ElfError::DynamicExecutable)`.
- Existing `PT_INTERP` case still rejects.
- Static `ET_EXEC` fixture still loads.

## Fix

`ElfImage::parse` now rejects both `PT_INTERP` and `PT_DYNAMIC` as dynamic
executable markers. Static `ET_EXEC` loading is unchanged.

## Verification

- `rejects_dynamic_and_mismatched_segments` covers both `PT_INTERP` and
  `PT_DYNAMIC` without an interpreter.
- `toolchain_elf_runs_through_loader` confirms a static executable still loads
  and runs.
- `cargo test`: 221 unit tests and all local integration tests pass.
- `cargo clippy --all-targets -- -D warnings`: clean.

## Prevention

Classify unsupported ELF features by every relevant program header, and add a
negative fixture for each dynamic-linking marker.
