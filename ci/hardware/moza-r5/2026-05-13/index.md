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

