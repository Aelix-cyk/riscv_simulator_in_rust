# Bug 0009: HPM event selectors disagree with the ACT4 declaration

| Field | Value |
|---|---|
| Found | 2026-10-25, code review of commit `2511e64` |
| Fixed | 2026-10-25 |
| Affected | `tests/compliance/act4/arvsr-rv64imafdcb.yaml:106`, `src/csr.rs:59-64` and `:317-320` |

## Summary

The simulator encodes HPM event selectors as one-hot masks:

```rust
pub(crate) const HPM_EVENT_INSTRUCTION: u8 = 1 << 0;
pub(crate) const HPM_EVENT_LOAD: u8 = 1 << 1;
pub(crate) const HPM_EVENT_STORE: u8 = 1 << 2;
pub(crate) const HPM_EVENT_BRANCH: u8 = 1 << 3;
```

and `write_mhpmevent` stores `value & HPM_EVENT_MASK` (`0xF`), accepting every
4-bit value:

```rust
self.mhpmevents[index] = value & HPM_EVENT_MASK;
```

The ACT4 config declares only `HPM_EVENTS: [0, 1, 2, 3]`
(`tests/compliance/act4/arvsr-rv64imafdcb.yaml:106`). Store (`4`) and branch
(`8`) are therefore undeclared, and writing `u64::MAX` reads back `15`, which
the config does not allow. Any UDB/ACT consumer sees a supported-event set that
contradicts the implementation.

## Specification requirement

Privileged ISA, §3.1.10 (Hardware Performance Monitor), local
`riscv-spec.html`, anchor `norm:mhpmevent_sz_warl_op`:

> The event selector CSRs, mhpmevent3–mhpmevent31, are 64-bit WARL registers
> that control which event causes the corresponding counter to increment. The
> meaning of these events is defined by the platform, but event 0 is defined to
> mean “no event.”

Because the selector is WARL, every accepted write must result in a value the
platform declares as legal; the ACT4 config is that declaration. Accepting
undeclared masks (including `15`) and hiding the store/branch encodings
violates the agreement between implementation and platform description.

## Impact

Compliance consumers can generate tests for an event set the implementation
does not match, while the implemented store and branch events are invisible to
them. Selector read-back is not WARL-canonical: `u64::MAX` yields `15` rather
than a declared value.

## Why it escaped

Unit tests write the internal constants (`HPM_EVENT_LOAD`, etc.) directly and
never validate undeclared write values or compare the YAML event list against
the mask constants. The ACT4 config is not parsed by a test that checks this
agreement.

## Suggested fix

Choose one encoding and make both sides agree:

- Keep one-hot masks and declare the full accepted set in
  `HPM_EVENTS` (at minimum `0, 1, 2, 4, 8`; include the combination masks if
  counting several event classes per counter is intended), or
- Keep the declared IDs `0..3` and canonicalize writes to those event IDs
  (with `0` = no event), rejecting or mapping undeclared values to a legal
  selector.

In both cases, document the platform event encoding once and reuse it in the
config and tests rather than duplicating literals.

## Regression tests

- Writing `u64::MAX` to `mhpmevent3` reads back a value permitted by
  `HPM_EVENTS`.
- A test parses `HPM_EVENTS` from the ACT4 YAML and asserts every selector the
  implementation can retain is declared there.
- Store (`4`) and branch (`8`) selectors still increment their counters under
  whichever encoding is chosen.

## Prevention

Generate or validate the ACT4 event list from the shared selector constants,
and sweep all 16 possible 4-bit writes in a WARL read-back test.

## Fix

Keep the platform's one-hot selector encoding and declare its complete legal
WARL space in ACT4. The accepted selectors are every combination of the four
implemented event bits, so `HPM_EVENTS` now lists the sixteen values from `0`
through `15` rather than only the four single-bit values.

## Regression tests

- `act4_declares_every_retained_hpm_event_selector` sweeps all sixteen 4-bit
  writes and verifies that each retained value appears in `HPM_EVENTS`.
- Existing event-counting tests continue to cover store (`4`) and branch (`8`)
  selectors and combined selectors.

## Verification

- `cargo test`
- `cargo clippy --all-targets -- -D warnings`
- `cargo test --test compliance act4_gate -- --ignored --nocapture`
