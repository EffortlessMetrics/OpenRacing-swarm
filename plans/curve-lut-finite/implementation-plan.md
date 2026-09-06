# Finite CurveLut acceptance

Status: candidate
Owner: curves/configuration
Issue: #312
Proposal/spec/ADR/active goal: n/a — focused accepted-value boundary repair

## Goal

Ensure every accepted `CurveLut` contains only finite values without silently
accepting non-finite persisted/user input and without breaking the existing
infallible closure-constructor signature.

## Contract

- Deserialization is strict: any NaN or infinity is rejected with the invalid
  table index and without dumping the table or arbitrary surrounding input.
- `try_from_table([f32; 256])` is the strict programmatic raw-table boundary.
- `try_from_fn` is the strict generated-table boundary and reports the first
  non-finite generated index.
- Existing `from_fn` and `from_bezier` remain infallible for compatibility.
  Their generated outputs are normalized deterministically: finite values keep
  the existing `[0,1]` clamp; any non-finite generated value becomes `0.0`.
  Callers that require rejection use the `try_*` APIs.
- No raw non-finite persisted/user table is silently replaced. Serde loading
  goes through the strict raw-table boundary.
- `lookup` remains unchanged and allocation-free on the RT path.

This deliberately does not add monotonicity or finite-input policy to `lookup`,
and it does not validate Bezier x-domain semantics; #313 owns inversion.

## Acceptance

- Strict construction and deserialization reject NaN, +infinity and -infinity
  at the first, interior and final entries with a stable index diagnostic.
- Finite 0.0/1.0 tables remain accepted.
- Legacy infallible generation cannot create a non-finite table and documents
  its deterministic sanitization rule.
- Strict generated construction reports non-finite output rather than
  sanitizing it.
- Lookup of an accepted table remains finite for finite normalized inputs.
- Focused tests use no `unwrap()` or `expect()`.
- Curves tests, Clippy, pipeline tests, policy and normalized routed proof pass
  before integration; #302 may not be bypassed.

## Proof

```text
python scripts/cargo_fmt_workspace.py --check
cargo test --locked -p openracing-curves
cargo clippy --locked -p openracing-curves --all-targets --all-features -- -D warnings
cargo test --locked -p openracing-pipeline
python scripts/policy_file.py --strict
git diff --check
OpenRacing Rust Small Result
```

## Rollback

Revert strict validation, generator sanitization, APIs and regressions together.
Do not broaden persisted-data acceptance during rollback without an explicit
compatibility decision.
