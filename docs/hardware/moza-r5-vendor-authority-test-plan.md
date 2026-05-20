# Moza R5 vendor authority test implementation plan

Status: proposed
Owner: hardware
Linked proposal: docs/proposals/OR-PROP-0001-moza-native-visible-lane.md
Linked specs:
- docs/specs/OR-SPEC-0002-moza-r5-vendor-authority-test-lane.md
Linked ADRs: docs/adr/0009-hardware-validation-evidence-state-machine.md
Active goal: .openracing/goals/active.toml

## Purpose

Provide a step-by-step implementation queue for Moza R5 vendor authority infrastructure, starting with docs/schema/fixture scaffolding and deferring hardware writes to exact-authorization stages.

## PR sequence

1. **Spec + schemas only**
   - Add OR-SPEC-0002.
   - Add five schemas for registry/status-plan/authorization/smoke receipts.
   - Add PR1 partial fixture skeleton for the authority/state command family.
   - Explicitly defer gain/safety, temperature, and compatibility mode command families to PR2.
   - No runtime implementation.

2. **Command registry + risk policy tests**
   - Complete all required command families before codec, probe, authorization, or hardware-write work builds on the registry.
   - Keep the registry semantic-only with `codec_status=semantic_only`.
   - Add a Rust risk policy model with `FirmwareOrDfuForbidden` and
     `UnknownDoNotSend` non-encodable and non-sendable.
   - Keep write-like vendor control/configuration candidates blocked from
     read-only probe eligibility and native-plan eligibility until a future
     exact authorization stage.
3. **Semantic-only serial codec + fixtures**
   - Add fixture-only serial frame decode for checked-in bytes.
   - Validate message start, declared length, checksum, and registry tuple mapping.
   - Record a `fixture_decode_only` state artifact with no serial open, no query send, no output/configuration writes, and no hardware-write eligibility.
   - Do not add encode, transport, CLI, read-only probe, authorization, or hardware output behavior.
4. **Fake serial transport**
5. **No-output CLI tools**
6. **Read-only vendor status probe**
7. **Exact authorization support**
8. **Vendor authority smoke dry-run**
9. **First bounded hardware authority attempt**
10. **Post-authority PIDFF response comparison**
11+. **Closed-loop motion ladder**

## Required gating invariant

No stage may claim native-visible readiness without strict verifier acceptance of real movement evidence.
PR1 schema and fixture artifacts remain non-claiming: `native_control_evidence=false`, `hardware_output_authorized=false`, and `native_visible_ready=false`.

## Proof commands (PR1)

```powershell
python scripts/policy_file.py
cargo run --locked -p openracing-tools --bin package-surface -- --check
git diff --check
```

## Proof commands (PR2)

```powershell
python scripts/cargo_fmt_workspace.py
cargo test --locked -p racing-wheel-hid-moza-protocol --test vendor_authority_registry -- --nocapture
cargo clippy --locked -p racing-wheel-hid-moza-protocol --all-targets --all-features -- -D warnings
cargo run --locked -p openracing-tools --bin package-surface -- --check
python scripts/policy_file.py
git diff --check
```

## Proof commands (PR3)

```powershell
python scripts/cargo_fmt_workspace.py
cargo test --locked -p racing-wheel-hid-moza-protocol --test vendor_serial_codec_fixtures -- --nocapture
cargo clippy --locked -p racing-wheel-hid-moza-protocol --all-targets --all-features -- -D warnings
cargo run --locked -p openracing-tools --bin package-surface -- --check
python scripts/policy_file.py
git diff --check
```
