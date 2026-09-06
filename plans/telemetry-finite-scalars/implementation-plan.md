# Finite normalized telemetry scalars

Status: candidate
Owner: telemetry/contracts
Issue: #311
Proposal/spec/ADR/active goal: n/a — focused numeric validation repair

## Goal

Keep invalid floating-point values out of normalized FFB state and prevent RPM
fraction calculations from treating invalid redlines as usable telemetry.

## Production delta

- `with_ffb_scalar` stores a value only when it is finite; accepted finite
  values retain the existing `[-1.0, 1.0]` clamp.
- Rejected NaN and infinities leave `ffb_scalar` unset, so `has_ffb_data()`
  remains a truthful presence check for data accepted through this builder.
- `rpm_fraction` returns `None` unless the supplied redline is finite and
  strictly positive. Existing stored RPM validation already guarantees a
  finite non-negative numerator for builder-created values.
- Positive finite redlines retain the existing division and `[0.0, 1.0]`
  clamp.

The public fields and Serde wire shape are unchanged. This work does not make
raw deserialization of `NormalizedTelemetry` validate every public field; that
would be a separate schema/compatibility decision.

## Acceptance

- NaN, +infinity and -infinity passed to `with_ffb_scalar` produce no FFB
  value and `has_ffb_data() == false`.
- Finite FFB endpoints and out-of-range finite values preserve current clamp
  behavior.
- `rpm_fraction` returns `None` for zero, negative, NaN and either infinity as
  redline.
- Positive finite redlines preserve current fraction/clamp behavior.
- Existing callers that use valid positive redlines continue unchanged.
- Focused tests use neither `unwrap()` nor `expect()`.
- Telemetry-contract tests, Clippy, policy and the normalized required Rust
  result pass before integration; #302 may not be bypassed.

## Proof

```text
python scripts/cargo_fmt_workspace.py --check
cargo test --locked -p racing-wheel-telemetry-contracts
cargo clippy --locked -p racing-wheel-telemetry-contracts --all-targets --all-features -- -D warnings
python scripts/policy_file.py --strict
git diff --check
OpenRacing Rust Small Result
```

## Non-goals

- No telemetry wire/schema version changes.
- No source adapter, game-support, LED implementation, FFB transport, RT, or
  hardware-support changes.
- No broad validation of public fields set through direct struct construction
  or deserialization.

## Rollback

Revert the builder/redline validation and their focused regressions together.
No persisted-data migration is required.
