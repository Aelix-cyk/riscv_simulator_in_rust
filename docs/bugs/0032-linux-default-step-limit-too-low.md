# Bug 0032: Default CLI step limit cannot boot the pinned Linux image

| Field | Value |
|---|---|
| Found | 2026-11-02, code review of commit `f821649` |
| Affected | `src/cli.rs:77` and `:193-196`, `README.md:49-51` and `:68-69` |
| Status | Fixed |
| Fixed | 2026-11-02 |

## Summary

Every firmware boot defaults to `OPENSBI_DEFAULT_MAX_STEPS = 10_000_000`:

```rust
let max_steps = max_steps.unwrap_or(if matches!(input, ProgramInput::Boot(_)) {
    OPENSBI_DEFAULT_MAX_STEPS
} else {
    DEFAULT_MAX_STEPS
});
```

The Linux boot gate uses 1,000,000,000 steps
(`tests/linux_boot.rs:11`) because the pinned Linux image does not reach
userspace within the default. The documented Linux command
(`README.md:49-51`) omits `--max-steps`, so it stops during early boot and
exits with status 2 even though the same image boots when a larger limit is
passed.

## Requirement

This is a CLI/documentation contract issue, not an ISA rule. The README
documents a Linux Image boot command without `--max-steps`, and states
(`:68-69`) that firmware boot defaults to 10,000,000 steps. The default must be
sufficient for the documented workflow, or the workflow must require and
document an explicit higher limit.

## Reproduction

Run the README Linux command against the pinned OpenSBI/Linux fixture without
`--max-steps`:

```sh
cargo run -- --firmware fw_jump.elf --dtb platform.dtb \
    --kernel Image --initrd initramfs.cpio
```

Expected: reaches the userspace init marker.
Actual: halts at the 10,000,000-step limit and exits 2.

## Impact

Users following the documentation cannot complete the advertised Linux boot.
CI and manual smoke tests that rely on defaults silently fail, while the
dedicated Linux gate passes because it overrides the limit.

## Why it escaped

`tests/linux_boot.rs` always passes `--max-steps 1_000_000_000`; no test
exercises the default CLI path for a Linux Image. The firmware probe path also
uses 10M, so the shared default looks intentional.

## Suggested fix

Pick one explicit policy:

- Give raw Linux `Image` input a larger default (e.g. 1,000,000,000) while
  keeping the 10M default for ELF probe firmwares; or
- Require `--max-steps` for Linux `Image` input, update the README command, and
  fail usage before boot when it is omitted.

Do not raise the default for every firmware boot without measuring probe
failure latency.

## Regression tests

- The README Linux command without `--max-steps` reaches the userspace init
  marker.
- A CLI unit test asserts the Linux Image default is at least the boot-gate
  limit.
- ELF probe boots keep their existing default and failure behaviour.

## Prevention

Tie documented defaults to one constant and test the exact documented command,
not only a command with overrides.

## Fix

The simulator CLI no longer has a step limit. `--max-steps`, both default
constants, the step-limit exit path, and all CLI uses of the option were
removed. The CLI now runs until the program exits through the platform device
or reaches the halt self-loop.

`Machine::run(step_limit)` remains available as a bounded library and test
helper; it is no longer involved in command-line execution.

## Regression tests

- `cli::tests::step_limit_option_is_removed`
- `elf_run::cli_requires_the_matching_input_option`
- `linux_boot::linux_reaches_an_smode_userspace_init` launches the documented
  Linux command without an instruction budget.

## Verification

- The pinned OpenSBI and Linux Image reach `/init`, print
  `ARVSR-LINUX-OK`, and power off with the default CLI command.
- `cargo test` passes.
- `cargo clippy --all-targets -- -D warnings` is clean.
