# Moza Lane Artifact Index

This index is diagnostic navigation only. It reads stored lane files, opens no HID device, sends no output or feature reports, and does not authorize hardware output or promote readiness.

- Lane: `ci/hardware/moza-r5/2026-05-13`
- Frontier: `closed_loop_undertravel_recorded`
- Highest passing stage: `native_response_ready`
- Next required stage: `native_visible_ready`
- Native actuator response proven: `true`
- Native visible motion proven: `false`
- Release ready: `false`

## Input Role Semantics

This section is diagnostic navigation only. `generic_aux` roles are valid parser-visible passive evidence, but they are not fully role-specific semantic mappings.

- Source artifact: `native-visible-verification.json`
- Semantic status: `partial_generic_aux_mapping`
- Required roles: `7`
- Required parser-visible roles: `7`
- Role-specific semantic mapping complete: `false`
- Unproven required role semantics: `1`
- Generic auxiliary roles: `1`

- Candidate-only extended slots: `2`

| Control | Role | Evidence Capture | Semantic Status | Candidate Slots |
| --- | --- | --- | --- | --- |
| `clutch` | `clutch` | `captures/r5-clutch-only-sweep.jsonl` | `generic_aux` | `r5_v1_extended_aux0_u16, r5_v1_extended_aux1_u16` |

## Vendor Authority Handoff

This section is native-control research navigation only. It does not open HID, send serial traffic, create authorization, emit the hardware attempt command, or claim native-visible readiness.

- State: `protocol_evidence_review_recorded`
- Next allowed action: Continue no-output vendor protocol investigation; finish remaining passive sniff scenarios or decode reviewed protocol candidates before any future output plan.
- Hardware output authorized: `false`
- Native visible ready: `false`
- Hardware attempt command emitted: `false`
- Exact command: `estop_set_ffb` frame `7E02461C0001F0` payload `01` risk `vendor_output_candidate`
- Required bench-clear evidence: `bench clear for exact estop_set_ffb: R5 stable, hands clear, wheel clear`
- Requires exclusive R5 serial/CDC access before separate attempt: `true`
- Pit House dependency: `false`; serial-owner risk: `true`
- Handoff opens serial: `false`; sends output: `false`
- R5 serial port hints: `COM4`
- Serial precondition guidance: Before creating short-lived exact authorization or running the separate bounded attempt, close or release Pit House and any other app that may own the R5 serial/CDC port; this is an exclusive-port precondition, not a Pit House dependency for native control.
- Live precondition refresh command: `wheelctl hardware doctor --json-out target/moza-current/vendor-authority-precondition-hardware-doctor.json --json`
- Live precondition receipt: `target/moza-current/vendor-authority-precondition-hardware-doctor.json`

| Artifact | Path |
| --- | --- |
| `authorization` | `vendor-authority-authorization.json` |
| `smoke_dry_run` | `vendor-authority-smoke-dry-run.json` |
| `attempt` | `vendor-authority-attempt.json` |
| `post_authority_pidff_smoke` | `vendor-post-authority-pidff-smoke.json` |
| `post_authority_pidff_response` | `vendor-post-authority-pidff-response.json` |
| `vendor_protocol_evidence_review` | `vendor-protocol-evidence-review.json` |

### Protocol Decode Priority

Frequency-ranked tuples are decode-priority navigation only. They do not make an unknown tuple sendable.

- Status: `frequency_ranked_unknown_commanded_tuples`
- Source receipt: `vendor-protocol-evidence-review.json`
- Unknown tuple risk class: `unknown_do_not_send`
- Output sendability claim: `false`
- Decode candidate sample scope: `highest_frequency_unknown_commanded_tuples`
- Decode candidate sample frames: `45`
- Decode candidate payload shapes: `5`
- Payloads empty or zero-filled in samples: `true`
- Decode candidate packet groups: `15`
- Unique packet patterns: `3`
- Repeated contiguous motifs: `7`
- Semantic hypothesis count: `5`
- Semantic decode claim: `false`
- Registry promotion claim: `false`
- Semantic correlation target count: `2`
- Semantic correlation sendability claim: `false`
- Semantic correlation next action: `capture or summarize named passive correlation scenarios; no output`

| Correlation target | Tuples | Observed completed scenarios | Missing scenarios | Next capture | Sendable |
| --- | --- | --- | --- | --- | --- |
| `base_status_or_mode_poll_candidate` | `0x25/0x19/0x01`, `0x25/0x19/0x02`, `0x25/0x19/0x03` | `pit-house-open-idle, pit-house-full-controls, pit-house-setting-change` | `simhub-open-idle, simhub-output-session, simulator-session-start-stop` | `simhub-open-idle` | `false` |
| `session_or_status_keepalive_candidate` | `0x5A/0x1B/0x00`, `0x5D/0x1B/0x01` | `pit-house-open-idle, pit-house-full-controls, pit-house-setting-change` | `simhub-open-idle, simhub-output-session, simulator-session-start-stop` | `simhub-open-idle` | `false` |
- Mode/enable decode candidate count: `2`
- Mode/enable sendability claim: `false`
- Mode/enable evidence sufficient for output plan: `false`

| Mode/enable candidate | Semantic questions | Tuples | Direction | Frame | Risk | Sendable |
| --- | --- | --- | --- | --- | --- | --- |
| `base_status_or_mode_poll_candidate` | `status_query, standard_pidff_mode_enable, game_control_mode_select` | `0x25/0x19/0x01`, `0x25/0x19/0x02`, `0x25/0x19/0x03` | `host_to_device` | `0x7E` | `unknown_do_not_send` | `false` |
| `session_or_status_keepalive_candidate` | `authority_keepalive, volatile_ffb_session_enable` | `0x5A/0x1B/0x00`, `0x5D/0x1B/0x01` | `host_to_device` | `0x7E` | `unknown_do_not_send` | `false` |
- Residual payload export gap packets: `4`
- Payload export gap scenarios: `3`
- Payload gap sendability claim: `false`

| Payload gap scenario | Missing packets | Example packet | Data len | Sendable |
| --- | ---: | ---: | ---: | --- |
| `pit-house-open-idle` | 1 | 5 | 8 | `false` |
| `pit-house-full-controls` | 1 | 5 | 8 | `false` |
| `pit-house-setting-change` | 2 | 7449 | 8 | `false` |

| Tuple | Count | Payload bytes | Scenarios |
| --- | ---: | ---: | ---: |
| `0x5A/0x1B/0x00` | 3295 | 0..0 | 3 |
| `0x5D/0x1B/0x01` | 3292 | 2..2 | 3 |
| `0x25/0x19/0x01` | 1243 | 2..2 | 3 |
| `0x25/0x19/0x02` | 1243 | 2..2 | 3 |
| `0x25/0x19/0x03` | 1243 | 2..2 | 3 |

| Semantic hypothesis tuple | Pattern hint | Hypothesis | Confidence | Sendable |
| --- | --- | --- | --- | --- |
| `0x5A/0x1B/0x00` | `repeated_high_frequency_0x1b_pair` | `session_or_status_keepalive_candidate` | `low_pattern_only` | `false` |
| `0x5D/0x1B/0x01` | `repeated_high_frequency_0x1b_pair` | `session_or_status_keepalive_candidate` | `low_pattern_only` | `false` |
| `0x25/0x19/0x01` | `repeated_zero_payload_0x19_triad` | `base_status_or_mode_poll_candidate` | `low_pattern_only` | `false` |
| `0x25/0x19/0x02` | `repeated_zero_payload_0x19_triad` | `base_status_or_mode_poll_candidate` | `low_pattern_only` | `false` |
| `0x25/0x19/0x03` | `repeated_zero_payload_0x19_triad` | `base_status_or_mode_poll_candidate` | `low_pattern_only` | `false` |

| Payload shape tuple | Samples | Payload bytes | Payload kinds | Sendable |
| --- | ---: | ---: | --- | --- |
| `0x5A/0x1B/0x00` | 9 | 0..0 | `empty` | `false` |
| `0x5D/0x1B/0x01` | 9 | 2..2 | `zero_filled` | `false` |
| `0x25/0x19/0x01` | 9 | 2..2 | `zero_filled` | `false` |
| `0x25/0x19/0x02` | 9 | 2..2 | `zero_filled` | `false` |
| `0x25/0x19/0x03` | 9 | 2..2 | `zero_filled` | `false` |

| Packet group pattern | Packets | Samples | Scenarios | Sendable |
| --- | ---: | ---: | ---: | --- |
| `0x25/0x19/0x02 -> 0x25/0x19/0x03 -> 0x25/0x19/0x01` | 6 | 18 | 3 | `false` |
| `0x5A/0x1B/0x00 -> 0x5D/0x1B/0x01` | 6 | 12 | 3 | `false` |
| `0x5A/0x1B/0x00 -> 0x5D/0x1B/0x01 -> 0x25/0x19/0x02 -> 0x25/0x19/0x03 -> 0x25/0x19/0x01` | 3 | 15 | 2 | `false` |

| Repeated motif | Length | Observed | Scenarios | Sendable |
| --- | ---: | ---: | ---: | --- |
| `0x25/0x19/0x02 -> 0x25/0x19/0x03` | 2 | 9 | 3 | `false` |
| `0x25/0x19/0x02 -> 0x25/0x19/0x03 -> 0x25/0x19/0x01` | 3 | 9 | 3 | `false` |
| `0x25/0x19/0x03 -> 0x25/0x19/0x01` | 2 | 9 | 3 | `false` |
| `0x5A/0x1B/0x00 -> 0x5D/0x1B/0x01` | 2 | 9 | 3 | `false` |
| `0x5A/0x1B/0x00 -> 0x5D/0x1B/0x01 -> 0x25/0x19/0x02` | 3 | 3 | 2 | `false` |
| `0x5D/0x1B/0x01 -> 0x25/0x19/0x02` | 2 | 3 | 2 | `false` |
| `0x5D/0x1B/0x01 -> 0x25/0x19/0x02 -> 0x25/0x19/0x03` | 3 | 3 | 2 | `false` |

| Sample fixture tuple | Samples | First frame | First payload bytes | Sendable |
| --- | ---: | --- | ---: | --- |
| `0x5A/0x1B/0x00` | 9 | `7E015A1B0001` | 0 | `false` |
| `0x5D/0x1B/0x01` | 9 | `7E035D1B01000007` | 2 | `false` |
| `0x25/0x19/0x01` | 9 | `7E032519010000CD` | 2 | `false` |
| `0x25/0x19/0x02` | 9 | `7E032519020000CE` | 2 | `false` |
| `0x25/0x19/0x03` | 9 | `7E032519030000CF` | 2 | `false` |

## Pit House Compatibility

This section is external-smoke navigation only. Pit House is not required for native OpenRacing control, and these artifacts do not authorize hardware output.

- Availability status: `running_install_location_unknown`
- Pit House available: `true`
- Official download page: `https://support.mozaracing.com/en/support/solutions/articles/70000627795-moza-pit-house-downloads`
- Install guidance: Install or update Pit House from the official MOZA Pit House Downloads support page; do not treat package-manager availability as authoritative evidence.
- Coexistence gate status: `fail`
- Pit House coexistence claimed: `false`
- Recorded cases: `2` / `5`

| Case | Case Artifact | Observation Artifact | Status |
| --- | --- | --- | --- |
| `pit_house_closed` | `pit-house-closed.json` | `pit-house-observation-closed.json` | `recorded` |
| `pit_house_open_idle_standard` | `pit-house-open-standard.json` | `pit-house-observation-open-standard.json` | `recorded` |
| `pit_house_open_direct` | `pit-house-direct-blocked.json` | `pit-house-observation-open-direct.json` | `missing` |
| `pit_house_mode_change_during_run` | `pit-house-mode-change.json` | `pit-house-observation-mode-change.json` | `missing` |
| `pit_house_firmware_update_page_open` | `pit-house-firmware-page.json` | `pit-house-observation-firmware-page.json` | `missing` |

## Simulator Compatibility

This section is external-smoke navigation only. Simulator telemetry and bounded simulator FFB are not required for native OpenRacing control, and this navigation does not authorize hardware output.

- Simulator telemetry claimed: `false`
- Bounded simulator FFB claimed: `false`
- Recorded artifacts: `0` / `2`
- Blocks smoke-ready: `true`

| Gate | Artifact | Gate Status | Claim Status |
| --- | --- | --- | --- |
| `simulator_telemetry` | `simulator-telemetry-proof.json` | `fail` | `missing` |
| `simulator_ffb_bounded` | `simulator-ffb-smoke.json` | `fail` | `missing` |

## Passive USB Sniffing

This section is protocol research/support navigation only. Passive sniff artifacts do not authorize output, do not satisfy native-visible or smoke-ready gates, and are not required for native OpenRacing control.

- Recorded scenarios: `3` / `6`
- Readiness claim: `false`
- Blocks native control: `false`
- Blocks native visible: `false`
- Blocks smoke-ready: `false`

- Next passive gap: `simhub-open-idle` (SimHub open idle)
- Next capture required: `true`

| Scenario | Status | Plan | Receipt | Summary |
| --- | --- | --- | --- | --- |
| `pit-house-open-idle` | `summary_recorded` | `present_non_claiming` | `present_non_claiming` | `present_non_claiming` |
| `pit-house-full-controls` | `summary_recorded` | `present_non_claiming` | `present_non_claiming` | `present_non_claiming` |
| `pit-house-setting-change` | `summary_recorded` | `present_non_claiming` | `present_non_claiming` | `present_non_claiming` |
| `simhub-open-idle` | `partial_or_unaccepted` | `present_non_claiming` | `missing` | `missing` |
| `simhub-output-session` | `partial_or_unaccepted` | `present_non_claiming` | `missing` | `missing` |
| `simulator-session-start-stop` | `partial_or_unaccepted` | `present_non_claiming` | `missing` | `missing` |

## Passive Enumeration And Input

| Path | Kind | Evidence Role |
| --- | --- | --- |
| `captures/es-controls.jsonl` | `jsonl` | `passive_input_or_descriptor_evidence` |
| `captures/ks-controls.jsonl` | `jsonl` | `passive_input_or_descriptor_evidence` |
| `captures/r5-aggregated-idle-after-controls.jsonl` | `jsonl` | `passive_input_or_descriptor_evidence` |
| `captures/r5-brake-only-sweep.jsonl` | `jsonl` | `passive_input_or_descriptor_evidence` |
| `captures/r5-clutch-only-sweep.jsonl` | `jsonl` | `passive_input_or_descriptor_evidence` |
| `captures/r5-handbrake-only-sweep.jsonl` | `jsonl` | `passive_input_or_descriptor_evidence` |
| `captures/r5-idle.jsonl` | `jsonl` | `passive_input_or_descriptor_evidence` |
| `captures/r5-steering-sweep.jsonl` | `jsonl` | `passive_input_or_descriptor_evidence` |
| `captures/r5-throttle-only-sweep.jsonl` | `jsonl` | `passive_input_or_descriptor_evidence` |
| `descriptor.json` | `json` | `passive_input_or_descriptor_evidence` |
| `device-list.json` | `json` | `passive_input_or_descriptor_evidence` |
| `fixture-promotion.json` | `json` | `passive_input_or_descriptor_evidence` |
| `hardware-doctor.json` | `json` | `passive_input_or_descriptor_evidence` |
| `hardware-lane-status.json` | `json` | `passive_input_or_descriptor_evidence` |
| `hid-list.json` | `json` | `passive_input_or_descriptor_evidence` |
| `lane-audit-openracing-control.json` | `json` | `passive_input_or_descriptor_evidence` |
| `lane-audit-passive.json` | `json` | `passive_input_or_descriptor_evidence` |
| `lane-capture-analysis.json` | `json` | `passive_input_or_descriptor_evidence` |
| `manifest-promotion-openracing-control.json` | `json` | `passive_input_or_descriptor_evidence` |
| `manifest-promotion-passive.json` | `json` | `passive_input_or_descriptor_evidence` |
| `manifest.json` | `json` | `passive_input_or_descriptor_evidence` |
| `moza-probe.json` | `json` | `passive_input_or_descriptor_evidence` |
| `openracing-control-verification.json` | `json` | `passive_input_or_descriptor_evidence` |
| `parser-fixture-validation.json` | `json` | `passive_input_or_descriptor_evidence` |
| `passive-verification.json` | `json` | `passive_input_or_descriptor_evidence` |
| `role-status-sync.json` | `json` | `passive_input_or_descriptor_evidence` |
| `vendor-authority-attempt.json` | `json` | `passive_input_or_descriptor_evidence` |
| `vendor-authority-authorization.json` | `json` | `passive_input_or_descriptor_evidence` |
| `vendor-authority-smoke-dry-run.json` | `json` | `passive_input_or_descriptor_evidence` |
| `vendor-status-ack-only-correlation-diagnosis.json` | `json` | `passive_input_or_descriptor_evidence` |
| `vendor-status-ack-only-correlation-hardware-doctor.json` | `json` | `passive_input_or_descriptor_evidence` |
| `vendor-status-ack-only-correlation-targeted.json` | `json` | `passive_input_or_descriptor_evidence` |
| `vendor-status-authority-endpoint-diagnosis.json` | `json` | `passive_input_or_descriptor_evidence` |
| `vendor-status-authority-payload-rerun-diagnosis.json` | `json` | `passive_input_or_descriptor_evidence` |
| `vendor-status-authority-payload-rerun-targeted.json` | `json` | `passive_input_or_descriptor_evidence` |
| `vendor-status-authority-source-gap.json` | `json` | `passive_input_or_descriptor_evidence` |
| `vendor-status-endpoint-candidates-from-payload-rerun.json` | `json` | `passive_input_or_descriptor_evidence` |
| `vendor-status-endpoint-candidates.json` | `json` | `passive_input_or_descriptor_evidence` |
| `vendor-status-extended-scan-diagnosis.json` | `json` | `passive_input_or_descriptor_evidence` |
| `vendor-status-extended-scan-hardware-doctor.json` | `json` | `passive_input_or_descriptor_evidence` |
| `vendor-status-extended-scan-targeted.json` | `json` | `passive_input_or_descriptor_evidence` |
| `vendor-status-framing-diagnosis.json` | `json` | `passive_input_or_descriptor_evidence` |
| `vendor-status-mode-matrix-demux-hardware-doctor.json` | `json` | `passive_input_or_descriptor_evidence` |
| `vendor-status-mode-matrix-demux.json` | `json` | `passive_input_or_descriptor_evidence` |
| `vendor-status-mode-matrix-hardware-doctor.json` | `json` | `passive_input_or_descriptor_evidence` |
| `vendor-status-mode-matrix-plan.json` | `json` | `passive_input_or_descriptor_evidence` |
| `vendor-status-mode-matrix.json` | `json` | `passive_input_or_descriptor_evidence` |
| `vendor-status-payload-source-candidates.json` | `json` | `passive_input_or_descriptor_evidence` |
| `vendor-status-payload-source-semantic-review.json` | `json` | `passive_input_or_descriptor_evidence` |
| `vendor-status-reply-correlation-diagnosis.json` | `json` | `passive_input_or_descriptor_evidence` |
| `vendor-status-reply-correlation-hardware-doctor.json` | `json` | `passive_input_or_descriptor_evidence` |
| `vendor-status-reply-correlation-targeted.json` | `json` | `passive_input_or_descriptor_evidence` |
| `vendor-status-response-semantic-fixtures.json` | `json` | `passive_input_or_descriptor_evidence` |
| `vendor-status-response-source-correlation.json` | `json` | `passive_input_or_descriptor_evidence` |
| `vendor-status-timing-correlation-plan.json` | `json` | `passive_input_or_descriptor_evidence` |

## Zero, Watchdog, Disconnect

| Path | Kind | Evidence Role |
| --- | --- | --- |
| `disconnect-proof.json` | `json` | `zero_torque_safety_evidence` |
| `lane-audit-zero.json` | `json` | `zero_torque_safety_evidence` |
| `manifest-promotion-zero.json` | `json` | `zero_torque_safety_evidence` |
| `watchdog-proof.json` | `json` | `zero_torque_safety_evidence` |
| `zero-torque-proof.json` | `json` | `zero_torque_safety_evidence` |
| `zero-verification.json` | `json` | `zero_torque_safety_evidence` |

## OpenRacing Native Control Foundation

| Path | Kind | Evidence Role |
| --- | --- | --- |
| `init-off.json` | `json` | `native_control_foundation_evidence` |
| `init-standard.json` | `json` | `native_control_foundation_evidence` |
| `low-torque-proof.json` | `json` | `native_control_foundation_evidence` |
| `native-actuator-profile-smoke.json` | `json` | `native_control_foundation_evidence` |
| `steering-angle-stream-proof.json` | `json` | `native_control_foundation_evidence` |
| `steering-angle-stream.jsonl` | `jsonl` | `native_control_foundation_evidence` |

## Native Response

| Path | Kind | Evidence Role |
| --- | --- | --- |
| `lane-audit-native-response.json` | `json` | `native_response_evidence` |
| `manifest-promotion-native-response.json` | `json` | `native_response_evidence` |
| `native-response-verification.json` | `json` | `native_response_evidence` |

## Native Visible And PIDFF Diagnosis

| Path | Kind | Evidence Role |
| --- | --- | --- |
| `native-actuator-visible-follow-up-plan.json` | `json` | `native_visible_or_pidff_diagnosis_evidence` |
| `native-actuator-visible-smoke-response-only.json` | `json` | `native_visible_or_pidff_diagnosis_evidence` |
| `native-actuator-visible-smoke.json` | `json` | `native_visible_or_pidff_diagnosis_evidence` |
| `native-controlled-angle-attempt-03-authorization.json` | `json` | `native_visible_or_pidff_diagnosis_evidence` |
| `native-controlled-angle-attempt-03-failure-analysis.json` | `json` | `native_visible_or_pidff_diagnosis_evidence` |
| `native-controlled-angle-attempt-03-preflight.json` | `json` | `native_visible_or_pidff_diagnosis_evidence` |
| `native-controlled-angle-attempt-03-smoke.json` | `json` | `native_visible_or_pidff_diagnosis_evidence` |
| `native-controlled-angle-authorization.json` | `json` | `native_visible_or_pidff_diagnosis_evidence` |
| `native-controlled-angle-closed-loop-authorization.json` | `json` | `native_visible_or_pidff_diagnosis_evidence` |
| `native-controlled-angle-closed-loop-failure-analysis.json` | `json` | `native_visible_or_pidff_diagnosis_evidence` |
| `native-controlled-angle-closed-loop-preflight.json` | `json` | `native_visible_or_pidff_diagnosis_evidence` |
| `native-controlled-angle-closed-loop-smoke.json` | `json` | `native_visible_or_pidff_diagnosis_evidence` |
| `native-controlled-angle-failure-analysis.json` | `json` | `native_visible_or_pidff_diagnosis_evidence` |
| `native-controlled-angle-plan.json` | `json` | `native_visible_or_pidff_diagnosis_evidence` |
| `native-controlled-angle-retry-authorization.json` | `json` | `native_visible_or_pidff_diagnosis_evidence` |
| `native-controlled-angle-retry-failure-analysis.json` | `json` | `native_visible_or_pidff_diagnosis_evidence` |
| `native-controlled-angle-retry-preflight.json` | `json` | `native_visible_or_pidff_diagnosis_evidence` |
| `native-controlled-angle-retry-smoke.json` | `json` | `native_visible_or_pidff_diagnosis_evidence` |
| `native-controlled-angle-smoke.json` | `json` | `native_visible_or_pidff_diagnosis_evidence` |
| `native-pidff-effect-lifecycle-plan.json` | `json` | `native_visible_or_pidff_diagnosis_evidence` |
| `native-pidff-lifecycle-trace.json` | `json` | `native_visible_or_pidff_diagnosis_evidence` |
| `native-pidff-semantics-diagnosis.json` | `json` | `native_visible_or_pidff_diagnosis_evidence` |
| `native-pidff-standard-path-diagnosis.json` | `json` | `native_visible_or_pidff_diagnosis_evidence` |
| `native-visible-verification.json` | `json` | `native_visible_or_pidff_diagnosis_evidence` |
| `vendor-post-authority-pidff-response.json` | `json` | `native_visible_or_pidff_diagnosis_evidence` |
| `vendor-post-authority-pidff-smoke.json` | `json` | `native_visible_or_pidff_diagnosis_evidence` |

## Pit House External Compatibility

| Path | Kind | Evidence Role |
| --- | --- | --- |
| `pit-house-availability.json` | `json` | `external_compatibility_evidence` |
| `pit-house-closed.json` | `json` | `external_compatibility_evidence` |
| `pit-house-evidence-closed.json` | `json` | `external_compatibility_evidence` |
| `pit-house-evidence-open-standard.json` | `json` | `external_compatibility_evidence` |
| `pit-house-observation-closed.json` | `json` | `external_compatibility_evidence` |
| `pit-house-observation-open-standard.json` | `json` | `external_compatibility_evidence` |
| `pit-house-open-standard.json` | `json` | `external_compatibility_evidence` |

## Smoke Promotion

| Path | Kind | Evidence Role |
| --- | --- | --- |
| `smoke-ready-verification.json` | `json` | `smoke_readiness_evidence` |

## Support And Service Diagnostics

| Path | Kind | Evidence Role |
| --- | --- | --- |
| `device-status.json` | `json` | `diagnostic_support_evidence` |
| `index.md` | `markdown` | `diagnostic_support_evidence` |
| `moza-status.json` | `json` | `diagnostic_support_evidence` |
| `pre-output-readiness.json` | `json` | `diagnostic_support_evidence` |
| `support-bundle.json` | `json` | `diagnostic_support_evidence` |

## Passive USB Sniffing

| Path | Kind | Evidence Role |
| --- | --- | --- |
| `vendor-protocol-evidence-review.json` | `json` | `passive_protocol_research_evidence` |

## Required Support-Bundle Artifact Index

| Path | Required Stage | Artifact Status | Claim Status |
| --- | --- | --- | --- |
| `manifest.json` | `passive` | `pass` | `artifact_only` |
| `device-list.json` | `passive` | `pass` | `artifact_only` |
| `moza-probe.json` | `passive` | `pass` | `artifact_only` |
| `hid-list.json` | `passive` | `pass` | `artifact_only` |
| `hardware-doctor.json` | `passive` | `pass` | `artifact_only` |
| `descriptor.json` | `passive` | `pass` | `artifact_only` |
| `captures/r5-idle.jsonl` | `passive` | `pass` | `artifact_only` |
| `captures/r5-steering-sweep.jsonl` | `passive` | `pass` | `artifact_only` |
| `captures/r5-throttle-only-sweep.jsonl` | `passive` | `pass` | `artifact_only` |
| `captures/r5-brake-only-sweep.jsonl` | `passive` | `pass` | `artifact_only` |
| `captures/r5-clutch-only-sweep.jsonl` | `passive` | `pass` | `artifact_only` |
| `captures/r5-handbrake-only-sweep.jsonl` | `passive` | `pass` | `artifact_only` |
| `captures/r5-aggregated-idle-after-controls.jsonl` | `passive` | `pass` | `artifact_only` |
| `captures/ks-controls.jsonl` | `passive` | `pass` | `artifact_only` |
| `captures/es-controls.jsonl` | `passive` | `pass` | `artifact_only` |
| `parser-fixture-validation.json` | `passive` | `pass` | `artifact_only` |
| `fixture-promotion.json` | `passive` | `pass` | `artifact_only` |
| `init-off.json` | `openracing_control_ready` | `pass` | `artifact_only` |
| `init-standard.json` | `openracing_control_ready` | `pass` | `artifact_only` |
| `moza-status.json` | `openracing_control_ready` | `pass` | `artifact_only` |
| `device-status.json` | `openracing_control_ready` | `pass` | `artifact_only` |
| `support-bundle.json` | `openracing_control_ready` | `pass` | `artifact_only` |
| `zero-torque-proof.json` | `zero` | `pass` | `artifact_only` |
| `watchdog-proof.json` | `zero` | `pass` | `artifact_only` |
| `disconnect-proof.json` | `zero` | `pass` | `artifact_only` |
| `low-torque-proof.json` | `openracing_control_ready` | `pass` | `artifact_only` |
| `steering-angle-stream-proof.json` | `openracing_control_ready` | `pass` | `artifact_only` |
| `native-actuator-profile-smoke.json` | `openracing_control_ready` | `pass` | `artifact_only` |
| `native-actuator-visible-smoke.json` | `native_response_ready` | `pass` | `artifact_only` |
| `pit-house-coexistence.json` | `smoke_ready` | `missing` | `artifact_only` |
| `simulator-telemetry-proof.json` | `smoke_ready` | `missing` | `artifact_only` |
| `simulator-ffb-smoke.json` | `smoke_ready` | `missing` | `artifact_only` |
| `passive-verification.json` | `passive` | `pass` | `stage_passed` |
| `manifest-promotion-passive.json` | `passive` | `pass` | `promotion_applied` |
| `lane-audit-passive.json` | `passive` | `pass` | `audit_passed` |
| `zero-verification.json` | `zero` | `pass` | `stage_passed` |
| `manifest-promotion-zero.json` | `zero` | `pass` | `promotion_applied` |
| `lane-audit-zero.json` | `zero` | `pass` | `audit_passed` |
| `openracing-control-verification.json` | `openracing_control_ready` | `pass` | `stage_passed` |
| `manifest-promotion-openracing-control.json` | `openracing_control_ready` | `pass` | `promotion_applied` |
| `lane-audit-openracing-control.json` | `openracing_control_ready` | `pass` | `audit_passed` |
| `native-response-verification.json` | `native_response_ready` | `pass` | `stage_passed` |
| `manifest-promotion-native-response.json` | `native_response_ready` | `pass` | `promotion_applied` |
| `lane-audit-native-response.json` | `native_response_ready` | `pass` | `audit_passed` |
| `native-visible-verification.json` | `native_visible_ready` | `pass` | `stage_failed` |
| `manifest-promotion-native-visible.json` | `native_visible_ready` | `missing` | `artifact_only` |
| `lane-audit-native-visible.json` | `native_visible_ready` | `missing` | `artifact_only` |
| `smoke-ready-verification.json` | `smoke_ready` | `pass` | `stage_failed` |
| `manifest-promotion-smoke-ready.json` | `smoke_ready` | `missing` | `artifact_only` |
| `lane-audit-smoke-ready.json` | `smoke_ready` | `missing` | `artifact_only` |
| `native-controlled-angle-attempt-03-authorization.json` | `native_visible_ready` | `pass` | `artifact_only` |
| `native-controlled-angle-attempt-03-smoke.json` | `native_visible_ready` | `pass` | `artifact_only` |
| `native-controlled-angle-closed-loop-preflight.json` | `native_visible_ready` | `pass` | `artifact_only` |
| `native-controlled-angle-closed-loop-authorization.json` | `native_visible_ready` | `pass` | `artifact_only` |
| `native-controlled-angle-closed-loop-smoke.json` | `native_visible_ready` | `pass` | `artifact_only` |
| `native-controlled-angle-closed-loop-failure-analysis.json` | `native_visible_ready` | `pass` | `artifact_only` |
