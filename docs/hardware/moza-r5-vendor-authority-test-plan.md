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
   - Treat the read-only status/mode matrix as the prerequisite for any later exact authorization; missing or unknown safety/mode status blocks authority planning instead of permitting a write.
   - Current evidence: `vendor-status-mode-matrix.json` records the first live read-only probe on COM4 after a fresh hardware doctor. It sent nine registry-approved read-only queries and no output/configuration/firmware commands, but decoded zero responses, so mode/safety status remained unknown.
   - Current diagnosis: `vendor-status-framing-diagnosis.json` classifies the first stored readback as repeated `0x0E/0x71/0x05` ASCII `NRFloss`/`recvGap` diagnostic stream frames, including one desynchronized partial frame, rather than registry status/mode replies.
   - Current demux follow-up: `vendor-status-mode-matrix-demux.json` records the bounded stream demux and response-side group/device tuple transform. It decoded seven registry status replies, but `estop_get_ffb` and `main_misc_get_ffb_status` still failed closed, so unknown safety/mode status still blocks authority planning.
   - Current targeted correlation: `vendor-status-reply-correlation-targeted.json` selected only `estop_get_ffb` and `main_misc_get_ffb_status`, used read-only serial queries, and decoded zero authority-state replies. `vendor-status-reply-correlation-diagnosis.json` classifies 23 of 24 scanned frames as diagnostic telemetry and records `0xA1/0x21/0x4D` as a response-like command mismatch for `main_misc_get_ffb_status`, not a semantic decode or sendability proof.
   - Current extended-scan correlation: `vendor-status-extended-scan-targeted.json` repeats the same two-command read-only probe with `--max-response-frames-per-query 64`. It scans 19 frames, decodes zero authority-state replies, and `vendor-status-extended-scan-diagnosis.json` now classifies `7E00A1214D` as a checksum-valid ACK-only/no-payload response-like frame for `0xA1/0x21/no_command`, so shallow scan-window depth is no longer the immediate explanation and the current blocker is status-payload correlation or corrected authority-status endpoint/command IDs.
   - Current ACK-only rerun: `vendor-status-ack-only-correlation-targeted.json` repeats the two-command read-only probe after a fresh observe-only doctor, decodes zero authority-state replies, opens no HID path, sends no output/configuration/firmware/PIDFF command, and `vendor-status-ack-only-correlation-diagnosis.json` reproduces the same ACK-only/no-payload `0xA1/0x21/no_command` candidate.
   - Current payload-source candidate review: `vendor-status-payload-source-candidates.json` preserves four nonzero `0x8E` setting-change response samples as `unknown_do_not_send` status-source questions only; it records no semantic decode, read-only probe readiness, authorization, output, or motion claim.
   - Current payload-source semantic review: `vendor-status-payload-source-semantic-review.json` adds fixture decoder coverage for the four `0x8E` samples but still records no same-tuple payload variation, no timing correlation, no authority-state source, no read-only probe readiness, no authorization, no output, and no motion claim.
   - Current timing-correlation plan: `vendor-status-timing-correlation-plan.json` stages a passive Pit House `0x8E` event-marker capture with a fresh hardware doctor selector and KS top-left front LED default-teal/red/default-teal notes. It records no capture, no timing proof, no live probe readiness, no authorization, no output, and no motion claim.
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
   - The executable `wheelctl moza vendor-authority-attempt` command MUST require `--confirm-bounded-vendor-authority-attempt`, validate authorization and smoke receipts, verify R5 USB serial identity before opening, send only the exact bound frame once, and write the consumed attempt receipt.
   - Do not add native-visible promotion, smoke-ready promotion, Pit House/SimHub/simulator dependencies, firmware/DFU behavior, high-torque enablement, direct HID report `0xaf`, or unknown host-to-device sends.
9b. **No-output operator handoff**
   - Surface the current vendor-authority frontier in artifact-index and bench-wizard navigation after closed-loop standard PIDFF undertravel is recorded.
   - The handoff may show the exact command id, hash-bound frame bytes, required bench-clear text, authorization receipt path, and no-output smoke dry-run path.
   - The handoff MUST keep `hardware_output_authorized=false`, create no authorization receipt itself, emit no hardware attempt command, and make Pit House/SimHub/simulator evidence non-blocking for native control.
   - Bench-wizard and artifact-index remain read-only navigation; a real serial write still requires a separate explicit operator request and the guarded `vendor-authority-attempt` command.
10. **Post-authority PIDFF response comparison**
   - Add a no-output `wheelctl moza vendor-post-authority-pidff-response` command that compares the preserved baseline PIDFF response receipt, consumed vendor-authority attempt receipt, and separately captured post-authority PIDFF response receipt.
   - The comparison MUST validate that the attempt receipt is consumed/non-claiming, both PIDFF response receipts are safe cleanup receipts, the post response is not stale relative to the attempt, and the PIDFF profile/strategy/envelope is comparable.
   - The comparison receipt MUST keep `native_control_evidence=false`, `hardware_output_authorized=false`, `native_visible_ready=false`, and `smoke_ready=false`; even a visible-motion-looking post response remains a verifier candidate only.
   - Bench-wizard may emit only the no-output comparison command after the attempt and post-response receipts exist; it MUST NOT emit the bounded vendor-authority attempt command or any PIDFF output command.
10b. **Blocked-before-send attempt receipt**
   - Add a separate non-claiming receipt for a guarded vendor-authority attempt that validates the exact receipts and R5 serial identity but fails before opening the serial port or sending the frame, such as a port-owner/access-denied condition.
   - The blocked receipt MUST NOT satisfy the consumed attempt schema or post-authority comparison gate. It must record `success=false`, `authorization_consumed=false`, `opened_serial_device=false`, `sent_authorized_frame=false`, `sent_authorized_frame_count=0`, `hardware_output_authorized=false`, `native_control_evidence=false`, `native_visible_ready=false`, and `smoke_ready=false`.
   - Bench-wizard/artifact-index navigation may surface the blocked state, but retry still requires fresh bench-clear, fresh exact authorization, fresh smoke dry-run, and a fresh attempt receipt path.
10c. **Vendor serial precondition handoff**
   - Surface an exclusive R5 serial/CDC access precondition in artifact-index and bench-wizard navigation before short-lived authorization or a separate bounded attempt.
   - Derive serial-port hints from stored `hardware-doctor.json` only; do not inspect live serial devices, open serial, create authorization, emit an attempt command, or send output from navigation.
   - The handoff may warn that Pit House or another vendor app can own the serial/CDC port, but it MUST keep Pit House non-blocking for native control and keep all readiness/control claims false.
10d. **Explicit authorization handoff command**
   - Keep bench-wizard no-output, but make the displayed exact authorization command include the explicit operator label and bounded expiry instead of relying on CLI defaults.
   - The generated command MUST parse through the normal CLI parser and MUST still omit the bounded hardware attempt command, serial port, read-only query send, output write, firmware/DFU behavior, native-control claim, native-visible claim, smoke-ready claim, and release-ready claim.
10e. **Live pre-authority hardware-doctor handoff**
   - Keep bench-wizard and artifact-index no-output, but surface a target-only `wheelctl hardware doctor` refresh command that operators must review immediately before exact authorization.
   - The refreshed receipt is current bench context only; it does not update checked-in lane evidence, create authorization, open HID, open serial, send queries, send output/configuration/firmware writes, or satisfy native-control/native-visible/smoke-ready/release-ready gates.
   - The handoff MUST continue to block authorization when a vendor app may own the R5 serial/CDC port or when the R5 serial/CDC interface is missing.
10f. **Precondition-bound exact authorization**
   - Require `wheelctl moza authorize-vendor-authority` to consume the refreshed target-only hardware-doctor precondition receipt before writing an exact authorization receipt.
   - The authorization command MUST validate that the receipt is fresh, observe-only, successful, shows the R5 serial/CDC Ports interface for `0x346E:0x0004`, and shows no running vendor app process that may own the serial port.
   - The authorization receipt MUST bind the precondition receipt path, R5 serial port/interface, precondition timestamp, and observe-only safety flags while keeping `native_control_evidence=false` and `native_visible_ready=false`.
   - Do not open HID, open serial, send read-only queries, send output/configuration/firmware writes, emit the bounded attempt command, or claim native-control/native-visible/smoke-ready/release-ready.
10g. **Consumed exact authority attempt evidence**
   - Record one bench-authorized `estop_set_ffb` vendor-authority attempt receipt after a fresh precondition-bound authorization and no-output smoke dry-run.
   - The attempt receipt MUST consume the authorization, verify the R5 serial identity, record exactly one sent authorized frame, and close `hardware_output_authorized=false` after the send.
   - The lane index MUST surface `vendor_authority_attempt_recorded` and the next allowed action as post-authority PIDFF response comparison before any motion claim.
   - Do not retry the vendor-authority frame, add PIDFF follow-up output, claim native-control/native-visible/smoke-ready/release-ready, or treat the consumed receipt as reusable authorization.
10h. **Post-authority PIDFF smoke authorization**
   - Add a guarded way to authorize the separately captured post-authority PIDFF response receipt at `vendor-post-authority-pidff-smoke.json` without replacing `native-actuator-visible-smoke.json`.
   - The authorization MUST validate the consumed vendor-authority attempt receipt, the preserved baseline response-only receipt, the exact R5 endpoint, and the exact post-authority PIDFF output command before any HID open.
   - The post-authority output remains bounded to descriptor-proven PIDFF reports, 5 percent maximum, 2000 ms maximum, final Stop All cleanup, no direct torque report, no high torque, no serial configuration, and no firmware/DFU behavior.
   - The consumed authorization state MUST close `hardware_output_authorized=false` after the response receipt and keep any visible-looking result as comparison/verifier candidate evidence only.
   - Do not claim native-control/native-visible/smoke-ready/release-ready, overwrite baseline evidence, emit commands from bench-wizard, or reuse the consumed vendor-authority attempt as authorization.
10i. **Post-authority PIDFF response evidence**
   - Record the separately authorized post-authority PIDFF response receipt and no-output comparison receipt after the consumed vendor-authority attempt.
   - The evidence MUST preserve the baseline PIDFF response receipt, record the post-authority response at `vendor-post-authority-pidff-smoke.json`, and write `vendor-post-authority-pidff-response.json`.
   - The comparison MUST keep `native_control_evidence=false`, `hardware_output_authorized=false`, `native_visible_ready=false`, and `smoke_ready=false` regardless of whether the response improves, regresses, or looks like a visible-motion candidate.
   - The lane index MUST surface the recorded post-authority comparison and the next action as strict verifier review before any motion claim.
   - Do not authorize another output, retry the vendor-authority frame, promote native-visible, promote smoke-ready, or treat a PIDFF response comparison as native-control proof.
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

## Proof commands (PR9 CLI)

```powershell
python scripts/cargo_fmt_workspace.py
cargo test --locked -p wheelctl --bin wheelctl vendor_authority_attempt -- --nocapture
cargo test --locked -p wheelctl --bin wheelctl parse_moza_vendor_authority_attempt -- --nocapture
cargo test --locked -p wheelctl --test cli_comprehensive_e2e_tests help_snapshots::snapshot_moza_help -- --nocapture
cargo clippy --locked -p wheelctl --bin wheelctl --all-features -- -D warnings
cargo hakari verify
cargo run --locked -p openracing-tools --bin package-surface -- --check
python scripts/policy_file.py
git diff --check
```

## Proof commands (PR10d)

```powershell
python scripts/cargo_fmt_workspace.py
cargo test --locked -p wheelctl --bin wheelctl vendor_authority_handoff -- --nocapture
cargo clippy --locked -p wheelctl --bin wheelctl --all-features -- -D warnings
cargo run --locked -p openracing-tools --bin package-surface -- --check
python scripts/policy_file.py
git diff --check
```

## Proof commands (PR10e)

```powershell
python scripts/cargo_fmt_workspace.py
cargo test --locked -p wheelctl --bin wheelctl vendor_authority_handoff -- --nocapture
cargo clippy --locked -p wheelctl --bin wheelctl --all-features -- -D warnings
cargo run --locked -p openracing-tools --bin package-surface -- --check
python scripts/policy_file.py
git diff --check
```

## Proof commands (PR10f)

```powershell
python scripts/cargo_fmt_workspace.py
cargo test --locked -p wheelctl --bin wheelctl vendor_authority_authorization -- --nocapture
cargo test --locked -p wheelctl --bin wheelctl vendor_authority_handoff -- --nocapture
cargo test --locked -p wheelctl --bin wheelctl parse_moza_authorize_vendor_authority -- --nocapture
cargo clippy --locked -p wheelctl --bin wheelctl --all-features -- -D warnings
cargo run --locked -p openracing-tools --bin package-surface -- --check
python scripts/policy_file.py
git diff --check
```

## Proof commands (PR10g)

```powershell
cargo test --locked -p wheelctl --bin wheelctl vendor_authority_attempt -- --nocapture
cargo test --locked -p wheelctl --bin wheelctl checked_in_moza_lane_index_matches_artifact_index_renderer -- --nocapture
cargo run --locked -p openracing-tools --bin package-surface -- --check
python scripts/policy_file.py
git diff --check
```

## Proof commands (PR9 handoff)

```powershell
python scripts/cargo_fmt_workspace.py
cargo test --locked -p wheelctl --bin wheelctl vendor_authority_handoff -- --nocapture
cargo test --locked -p wheelctl --bin wheelctl checked_in_moza_lane_index_matches_artifact_index_renderer -- --nocapture
cargo clippy --locked -p wheelctl --bin wheelctl --all-features -- -D warnings
cargo hakari verify
cargo run --locked -p openracing-tools --bin package-surface -- --check
python scripts/policy_file.py
git diff --check
```

## Proof commands (PR10)

```powershell
python scripts/cargo_fmt_workspace.py
cargo test --locked -p wheelctl --bin wheelctl vendor_post_authority_pidff_response -- --nocapture
cargo test --locked -p wheelctl --bin wheelctl post_authority_comparison -- --nocapture
cargo test --locked -p wheelctl --bin wheelctl parse_moza_vendor_post_authority_pidff_response -- --nocapture
cargo test --locked -p wheelctl --test cli_comprehensive_e2e_tests help_snapshots::snapshot_moza_help -- --nocapture
cargo clippy --locked -p wheelctl --bin wheelctl --all-features -- -D warnings
cargo hakari verify
cargo run --locked -p openracing-tools --bin package-surface -- --check
python scripts/policy_file.py
git diff --check
```

## Proof commands (PR10b)

```powershell
python scripts/cargo_fmt_workspace.py
cargo test --locked -p wheelctl --bin wheelctl vendor_authority_attempt_blocked -- --nocapture
cargo test --locked -p wheelctl --bin wheelctl vendor_authority_handoff -- --nocapture
cargo clippy --locked -p wheelctl --bin wheelctl --all-features -- -D warnings
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
