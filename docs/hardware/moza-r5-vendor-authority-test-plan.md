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
   - Add a software-only fake transport that replays checked-in fixture bytes through the decoder.
   - Accept only read-only status fixture commands and reject write-like vendor output/configuration candidates until exact authorization exists.
   - Record a `fake_transport_verified` artifact with `transport_kind=fake_only`, no serial open, no query send, no output/configuration writes, no hardware evidence, and no hardware-write eligibility.
   - Do not add CLI, hardware read-only probing, authorization receipts, serial device I/O, or hardware output behavior.
5. **No-output CLI tools**
   - Add a `wheelctl moza vendor-fake-transport` command that exercises only the software fake transport from checked-in fixtures.
   - Emit a non-claiming CLI receipt with `claim_scope=software_cli_fake_transport_only`, no HID open, no serial open, no read-only query send, no output/configuration writes, no hardware evidence, and no hardware-write eligibility.
   - Add a schema for the no-output CLI receipt and tests proving read-only status fixtures are accepted while write-like candidates remain blocked.
   - Do not add read-only hardware probing, authorization receipts, serial device I/O, hardware output behavior, or readiness promotion.
6. **Read-only vendor status probe**
   - Add a guarded `wheelctl moza vendor-status-probe` command for the R5 serial/CDC interface.
   - Require explicit `--confirm-read-only-query` and USB VID/PID port identity matching before opening the serial port or sending query frames.
   - Send only registry-allowed vendor status queries, record decoded status responses, and emit a non-claiming receipt with `sent_read_only_query_commands=true`, `sent_output_writes=false`, `sent_configuration_writes=false`, `sent_firmware_or_dfu_commands=false`, `hardware_output_authorized=false`, and `native_control_evidence=false`.
   - Do not add exact authorization, output/configuration writes, firmware/DFU behavior, native-visible promotion, smoke-ready promotion, or simulator claims.
7. **Exact authorization support**
   - Add a guarded `wheelctl moza authorize-vendor-authority` command that creates a single-use authorization receipt without opening HID, opening serial, sending read-only queries, or sending output/configuration/firmware writes.
   - Require explicit `--confirm-exact-vendor-authority-authorization`, command-bound bench-clear evidence, a registry command id, and a full decoded frame whose tuple, device id, checksum, frame hash, and payload hash are bound into the receipt.
   - Accept only registry commands whose risk class requires exact authorization; reject read-only status commands, standard PIDFF commands, firmware/DFU, unknown, empty-payload, tuple-mismatched, and checksum-invalid frames.
   - Keep `native_control_evidence=false` and `native_visible_ready=false`; this receipt authorizes only one later exact vendor-authority frame after the no-output dry-run stage reviews and consumes it.
   - Do not add hardware send behavior, serial transport writes, native-visible promotion, smoke-ready promotion, coexistence claims, simulator claims, or readiness promotion.
8. **Vendor authority smoke dry-run**
   - Add a guarded `wheelctl moza vendor-authority-smoke-dry-run` command that validates a prior exact vendor-authority authorization receipt without opening HID, opening serial, sending read-only queries, or sending output/configuration/firmware writes.
   - Require explicit `--confirm-no-output-smoke-dry-run`, validate the authorization schema and safety gates, reject consumed/expired/drifted receipts, and re-decode the bound frame/hash/payload before emitting the dry-run receipt.
   - Emit `claim_scope=software_vendor_authority_smoke_dry_run`, `native_control_evidence=false`, `hardware_output_authorized=false`, `native_visible_ready=false`, `authorization_consumed=false`, `commands_sent=[]`, and `planned_next_output.allowed=false`.
   - Do not add hardware send behavior, authorization consumption, serial transport writes, native-visible promotion, smoke-ready promotion, coexistence claims, simulator claims, or readiness promotion.
9. **First bounded hardware authority attempt**
   - Define the consumed hardware-attempt receipt before adding any executable serial-write command.
   - The attempt receipt MUST validate the exact authorization receipt and matching smoke dry-run receipt, then record `authorization_consumed=true`, `sent_authorized_frame=true`, `sent_authorized_frame_count=1`, exact frame/payload hashes, serial identity verification, and the command risk class.
   - The consumed attempt receipt MUST close the authorization gate by recording `hardware_output_authorized=false` after the attempt, while keeping `native_control_evidence=false`, `native_visible_ready=false`, `smoke_ready=false`, `sent_firmware_or_dfu_commands=false`, `sent_unknown_commands=false`, `direct_hid_report_0xaf_sent=false`, and `high_torque_enabled=false`.
   - Retrying the same command/frame MUST require a new bench-clear, new exact authorization receipt, new smoke dry-run receipt, and new attempt receipt path; an existing consumed attempt receipt is not reusable authorization.
   - Do not add native-visible promotion, smoke-ready promotion, Pit House/SimHub/simulator dependencies, firmware/DFU behavior, high-torque enablement, direct HID report `0xaf`, or unknown host-to-device sends.
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

## Proof commands (PR5)

```powershell
python scripts/cargo_fmt_workspace.py
cargo test --locked -p wheelctl --bin wheelctl vendor_fake_transport -- --nocapture
cargo test --locked -p wheelctl --bin wheelctl parse_moza_vendor_fake_transport -- --nocapture
cargo test --locked -p racing-wheel-hid-moza-protocol --test vendor_fake_serial_transport -- --nocapture
cargo clippy --locked -p wheelctl --bin wheelctl --all-features -- -D warnings
cargo run --locked -p wheelctl --bin wheelctl -- moza vendor-fake-transport --json-out target/moza-current/vendor-no-output-cli.json --json
cargo run --locked -p openracing-tools --bin package-surface -- --check
python scripts/policy_file.py
git diff --check
```

## Proof commands (PR9 contract)

```powershell
python scripts/cargo_fmt_workspace.py
cargo test --locked -p wheelctl --bin wheelctl vendor_authority_attempt_schema -- --nocapture
cargo run --locked -p openracing-tools --bin package-surface -- --check
python scripts/policy_file.py
git diff --check
```

## Proof commands (PR6)

```powershell
python scripts/cargo_fmt_workspace.py
cargo test --locked -p racing-wheel-hid-moza-protocol --test vendor_status_probe -- --nocapture
cargo test --locked -p racing-wheel-hid-moza-protocol --test vendor_authority_registry -- --nocapture
cargo test --locked -p wheelctl --bin wheelctl vendor_status_probe -- --nocapture
cargo test --locked -p wheelctl --bin wheelctl parse_moza_vendor_status_probe -- --nocapture
cargo clippy --locked -p racing-wheel-hid-moza-protocol --all-targets --all-features -- -D warnings
cargo clippy --locked -p wheelctl --bin wheelctl --all-features -- -D warnings
cargo hakari verify
cargo run --locked -p openracing-tools --bin package-surface -- --check
python scripts/policy_file.py
git diff --check
```

## Proof commands (PR7)

```powershell
python scripts/cargo_fmt_workspace.py
cargo test --locked -p wheelctl --bin wheelctl vendor_authority_authorization -- --nocapture
cargo test --locked -p wheelctl --bin wheelctl parse_moza_authorize_vendor_authority -- --nocapture
cargo test --locked -p wheelctl --test cli_comprehensive_e2e_tests help_snapshots::snapshot_moza_help -- --nocapture
cargo clippy --locked -p wheelctl --bin wheelctl --all-features -- -D warnings
cargo hakari verify
cargo run --locked -p openracing-tools --bin package-surface -- --check
python scripts/policy_file.py
git diff --check
```

## Proof commands (PR8)

```powershell
python scripts/cargo_fmt_workspace.py
cargo test --locked -p wheelctl --bin wheelctl vendor_authority_smoke -- --nocapture
cargo test --locked -p wheelctl --bin wheelctl parse_moza_vendor_authority_smoke_dry_run -- --nocapture
cargo test --locked -p wheelctl --test cli_comprehensive_e2e_tests help_snapshots::snapshot_moza_help -- --nocapture
cargo clippy --locked -p wheelctl --bin wheelctl --all-features -- -D warnings
cargo hakari verify
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

## Proof commands (PR4)

```powershell
python scripts/cargo_fmt_workspace.py
cargo test --locked -p racing-wheel-hid-moza-protocol --test vendor_fake_serial_transport -- --nocapture
cargo test --locked -p racing-wheel-hid-moza-protocol --test vendor_serial_codec_fixtures -- --nocapture
cargo clippy --locked -p racing-wheel-hid-moza-protocol --all-targets --all-features -- -D warnings
cargo run --locked -p openracing-tools --bin package-surface -- --check
python scripts/policy_file.py
git diff --check
```
