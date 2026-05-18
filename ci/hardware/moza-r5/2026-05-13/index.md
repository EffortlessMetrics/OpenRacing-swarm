# Moza Lane Artifact Index

This index is diagnostic navigation only. It reads stored lane files, opens no HID device, sends no output or feature reports, and does not authorize hardware output or promote readiness.

- Lane: `ci/hardware/moza-r5/2026-05-13`
- Frontier: `repeated_safe_undertravel_attempt_03_preflight_recorded`
- Highest passing stage: `native_response_ready`
- Next required stage: `native_visible_ready`
- Native actuator response proven: `true`
- Native visible motion proven: `false`
- Release ready: `false`

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
| `native-controlled-angle-attempt-03-preflight.json` | `json` | `native_visible_or_pidff_diagnosis_evidence` |
| `native-controlled-angle-authorization.json` | `json` | `native_visible_or_pidff_diagnosis_evidence` |
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
| `native-visible-verification.json` | `json` | `native_visible_or_pidff_diagnosis_evidence` |

## Pit House External Compatibility

| Path | Kind | Evidence Role |
| --- | --- | --- |
| `pit-house-availability.json` | `json` | `external_compatibility_evidence` |
| `pit-house-closed.json` | `json` | `external_compatibility_evidence` |
| `pit-house-evidence-closed.json` | `json` | `external_compatibility_evidence` |
| `pit-house-observation-closed.json` | `json` | `external_compatibility_evidence` |

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

## Required Support-Bundle Artifact Index

| Path | Required Stage | Status |
| --- | --- | --- |
| `manifest.json` | `passive` | `pass` |
| `device-list.json` | `passive` | `pass` |
| `moza-probe.json` | `passive` | `pass` |
| `hid-list.json` | `passive` | `pass` |
| `hardware-doctor.json` | `passive` | `pass` |
| `descriptor.json` | `passive` | `pass` |
| `captures/r5-idle.jsonl` | `passive` | `pass` |
| `captures/r5-steering-sweep.jsonl` | `passive` | `pass` |
| `captures/r5-throttle-only-sweep.jsonl` | `passive` | `pass` |
| `captures/r5-brake-only-sweep.jsonl` | `passive` | `pass` |
| `captures/r5-clutch-only-sweep.jsonl` | `passive` | `pass` |
| `captures/r5-handbrake-only-sweep.jsonl` | `passive` | `pass` |
| `captures/r5-aggregated-idle-after-controls.jsonl` | `passive` | `pass` |
| `captures/ks-controls.jsonl` | `passive` | `pass` |
| `captures/es-controls.jsonl` | `passive` | `pass` |
| `parser-fixture-validation.json` | `passive` | `pass` |
| `fixture-promotion.json` | `passive` | `pass` |
| `init-off.json` | `openracing_control_ready` | `pass` |
| `init-standard.json` | `openracing_control_ready` | `pass` |
| `moza-status.json` | `openracing_control_ready` | `pass` |
| `device-status.json` | `openracing_control_ready` | `pass` |
| `support-bundle.json` | `openracing_control_ready` | `pass` |
| `zero-torque-proof.json` | `zero` | `pass` |
| `watchdog-proof.json` | `zero` | `pass` |
| `disconnect-proof.json` | `zero` | `pass` |
| `low-torque-proof.json` | `openracing_control_ready` | `pass` |
| `steering-angle-stream-proof.json` | `openracing_control_ready` | `pass` |
| `native-actuator-profile-smoke.json` | `openracing_control_ready` | `pass` |
| `native-actuator-visible-smoke.json` | `smoke_ready` | `missing` |
| `pit-house-coexistence.json` | `smoke_ready` | `missing` |
| `simulator-telemetry-proof.json` | `smoke_ready` | `missing` |
| `simulator-ffb-smoke.json` | `smoke_ready` | `missing` |
| `passive-verification.json` | `passive` | `pass` |
| `manifest-promotion-passive.json` | `passive` | `pass` |
| `lane-audit-passive.json` | `passive` | `pass` |
| `zero-verification.json` | `zero` | `pass` |
| `manifest-promotion-zero.json` | `zero` | `pass` |
| `lane-audit-zero.json` | `zero` | `pass` |
| `openracing-control-verification.json` | `openracing_control_ready` | `pass` |
| `manifest-promotion-openracing-control.json` | `openracing_control_ready` | `pass` |
| `lane-audit-openracing-control.json` | `openracing_control_ready` | `pass` |
| `smoke-ready-verification.json` | `smoke_ready` | `pass` |
| `manifest-promotion-smoke-ready.json` | `smoke_ready` | `missing` |
| `lane-audit-smoke-ready.json` | `smoke_ready` | `missing` |

