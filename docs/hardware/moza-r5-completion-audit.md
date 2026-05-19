# Moza R5 Lane Completion Audit

Status: active
Owner: hardware
Created: 2026-05-18
Linked proposal: docs/proposals/OR-PROP-0001-moza-native-visible-lane.md
Linked specs: docs/specs/OR-SPEC-0001-moza-native-visible-lane.md
Linked ADRs: docs/adr/0009-hardware-validation-evidence-state-machine.md
Linked plan: plans/moza-native-visible-lane/implementation-plan.md
Support/status impact: audit only; no readiness promotion
Policy impact: no new policy exception

This audit maps the Moza lane objective to concrete artifacts. It is not a
generated receipt, does not open HID, does not authorize output, and does not
satisfy native-visible, smoke-ready, or release-ready gates.

## Objective Restatement

The lane is complete when OpenRacing has a boring, reproducible, receipt-backed
hardware bring-up path for Steven's Moza R5 stack:

- R5 wheelbase, KS/ES wheels, SR-P pedals, and HBP handbrake are represented in
  a dated lane.
- Passive enumeration, descriptor capture, input parsing, and parser fixtures
  are proven by artifacts.
- Zero-torque, low-torque, watchdog, disconnect, and final-zero / Stop All
  safety proofs are proven by artifacts.
- Native movement reaches the visible-motion gate without overclaiming or
  bypassing authorization.
- Pit House coexistence is proven as external compatibility.
- One real simulator telemetry path is proven.
- One bounded sim-to-Moza FFB smoke run is proven.
- Smoke-ready is promoted only after the verifier, manifest promotion, and lane
  audit all pass.
- `release_ready`, `high_torque_validated`, firmware, DFU, serial config, and
  direct-report `0x20` output remain unclaimed unless separate evidence exists.

## Current Verdict

The objective is not complete.

The lane is currently `native_response_ready`. The native OpenRacing response
path is proven, but native visible motion is still blocked. Smoke-ready is also
blocked by missing Pit House coexistence, simulator telemetry, and bounded
simulator FFB receipts.

The next hardware-output work remains blocked until a separate fresh
command-bound bench-clear creates an exact attempt-03 authorization receipt.
The existing attempt-03 preflight is no-output evidence only.

## Prompt-To-Artifact Checklist

| Objective requirement | Evidence inspected | Status | Notes |
| --- | --- | --- | --- |
| Dated real hardware lane exists for Steven's Moza stack | `ci/hardware/moza-r5/2026-05-13/manifest.json`; `device-list.json`; `moza-probe.json`; `hid-list.json`; `descriptor.json`; verifier endpoint and role evidence | Pass | Manifest completion state is `native_response_ready`; release, simulator, and high-torque validation are false. |
| R5 wheelbase observed | `verify-bundle --stage native-visible-ready`; gates `moza_r5_observed`, `manifest_r5_pid_consistency` | Pass | Current verifier found matching R5 `0x346E:0x0004` evidence across lane receipts and captures. |
| KS/ES wheels, SR-P pedals, and HBP handbrake represented | Verifier `role_evidence`; captures `ks-controls.jsonl`, `es-controls.jsonl`, `r5-throttle-only-sweep.jsonl`, `r5-brake-only-sweep.jsonl`, `r5-clutch-only-sweep.jsonl`, `r5-handbrake-only-sweep.jsonl` | Pass with semantic caveats | KS, ES, steering, and throttle are parser-proven. Brake, clutch, and handbrake are parser-visible through generic R5 V1 extended fields; semantic naming remains unproven and diagnostic only. |
| Input role semantic mapping complete | `pre-output-readiness.json` `input_semantic_mapping_complete=false`; `input_role_semantics`; `artifact-index` Input Role Semantics section | Partial | Six candidate-only R5 V1 extended slots are surfaced for brake, clutch, and handbrake. Four candidates are ambiguous across clutch/handbrake captures. Every candidate has `claim_status=candidate_only` and `readiness_claim=false`; these help navigation but do not prove role-specific semantics. |
| Passive enumeration proven | `passive-verification.json`; `manifest-promotion-passive.json`; `lane-audit-passive.json`; verifier passive gates | Pass | Passive verifier and audit pass. |
| Descriptor capture proven | `descriptor.json`; verifier gate `descriptor_metadata` | Pass | Descriptor metadata is complete for the R5 record. |
| Input parsing proven | Passive capture files; verifier gate `passive_captures_parse` | Pass | Current verifier replayed 128215 passive capture reports through Moza parsers. |
| Parser fixtures proven | `parser-fixture-validation.json`; `fixture-promotion.json`; verifier gates `parser_fixture_validation`, `fixture_promotion` | Pass | Current verifier validated 9 required parser captures and fixture promotion. |
| Zero-torque proof exists | `zero-torque-proof.json`; `zero-verification.json`; `manifest-promotion-zero.json`; `lane-audit-zero.json` | Pass | Real zero proof and zero-stage promotion/audit are present. |
| Watchdog proof exists | `watchdog-proof.json`; verifier gate `watchdog_zero_output` | Pass | Watchdog proof injected timeout and sent final zero. |
| Disconnect proof exists | `disconnect-proof.json`; verifier gate `disconnect_final_zero` | Pass | Disconnect proof observed HID write failure and attempted final zero with zero-only payloads. |
| Low-torque proof exists | `low-torque-proof.json`; verifier gate `low_torque_bounded`; `openracing-control-verification.json` | Pass | Bounded PIDFF low-torque proof and OpenRacing native control foundation are present. |
| Final-zero / Stop All cleanup paths proven for current native attempts | `native-actuator-profile-smoke.json`; `native-controlled-angle-smoke.json`; `native-controlled-angle-retry-smoke.json`; verifier gate details | Partial pass | Native response and both controlled-angle undertravel attempts sent cleanup successfully. Smoke-level final-zero / bounded FFB cleanup remains unproven until simulator FFB smoke exists. |
| Native actuator response proven | `native-actuator-visible-smoke.json`; `native-response-verification.json`; pre-output readiness | Pass | Response gate records about 0.181 degrees above the 0.100 degree response threshold. |
| Native visible motion proven | `native-visible-verification.json`; current `verify-bundle --stage native-visible-ready` | Missing | Current verifier fails `native_actuator_visible_smoke`; both controlled-angle attempts remain below the 1 degree visible threshold. |
| Attempt-03 profile planned without output | `native-pidff-effect-lifecycle-plan.json`; `native-controlled-angle-attempt-03-preflight.json` | Pass as no-output preparation | Profile `bounded-pidff-effect-lifecycle-v1` is preflighted with `dry_run=true`, zero writes, and `hardware_output_enabled=false`. It authorizes no output. |
| Attempt-03 authorization exists | `native-controlled-angle-attempt-03-authorization.json` | Missing | Blocked by fresh command-bound bench-clear. |
| Attempt-03 output receipt exists | `native-controlled-angle-attempt-03-smoke.json` | Missing | Blocked by matching attempt-03 authorization. |
| Pit House compatibility navigation is current | `pre-output-readiness.json`, `artifact-index`, and `bench-wizard` `pit_house_compatibility`; `ci/hardware/moza-r5/2026-05-13/index.md` Pit House Compatibility section | Pass as non-claiming navigation | The lane records `pit_house_available=false`, `recorded_case_count=1/5`, `pit_house_coexistence_claimed=false`, `readiness_claim=false`, `blocks_native_control=false`, and `blocks_native_visible=false`. Missing open/direct/mode-change/firmware-page cases remain external smoke blockers. |
| Pit House coexistence proven | `pit-house-coexistence.json`; smoke-ready verifier | Missing | `pit-house-availability.json` is non-claiming availability evidence only. Coexistence matrix is not proven. |
| Simulator compatibility navigation is current | `pre-output-readiness.json`, `artifact-index`, and `bench-wizard` `simulator_compatibility`; `ci/hardware/moza-r5/2026-05-13/index.md` Simulator Compatibility section | Pass as non-claiming navigation | The lane records `recorded_artifact_count=0/2`, `simulator_telemetry_claimed=false`, `bounded_simulator_ffb_claimed=false`, `readiness_claim=false`, `blocks_native_control=false`, and `blocks_native_visible=false`. Missing telemetry and bounded FFB artifacts remain external smoke blockers. |
| Simulator telemetry proof exists | `simulator-telemetry-proof.json`; smoke-ready verifier | Missing | No real simulator telemetry receipt exists. |
| Bounded sim-to-Moza FFB smoke exists | `simulator-ffb-smoke.json`; smoke-ready verifier | Missing | No bounded simulator FFB receipt or output log exists. |
| Smoke-ready verification passes | `smoke-ready-verification.json`; current smoke-ready verifier state | Missing | Current smoke-ready verification fails native visible motion, Pit House coexistence, simulator telemetry, and simulator FFB. |
| Smoke-ready manifest promotion and audit exist | `manifest-promotion-smoke-ready.json`; `lane-audit-smoke-ready.json` | Missing | These cannot be created until smoke-ready verification passes. |
| Release-ready remains unclaimed | `manifest.json`; support/readiness receipts | Pass | `release_ready=false`; this lane does not claim release readiness. |
| High torque remains unclaimed | `manifest.json`; controlled-angle receipts; verifier details | Pass | `high_torque_validated=false`; controlled-angle receipts record no high torque. |
| Direct report `0x20`, serial config, firmware, and DFU remain out of scope | Controlled-angle receipts; verifier details; bench wizard | Pass | Current artifacts forbid direct `0x20`, high torque, serial config, firmware, and DFU. |

## Current Gate Evidence

The current no-output verifier result for `native-visible-ready` is expected to
fail:

```text
success=false
failed_gates=1
failed gate: native_actuator_visible_smoke
next_commands=[]
no_hid_device_opened=true
no_ffb_writes=true
```

The current `bench-wizard` result is diagnostic only:

```text
frontier=repeated_safe_undertravel_attempt_03_preflight_recorded
highest_passing_stage=native_response_ready
next_required_stage=native_visible_ready
hardware_output_authorized=false
authorization_receipt_created=false
next_operator_step.kind=awaiting_separate_attempt_03_authorization
next_operator_step.required_authorization_receipt=native-controlled-angle-attempt-03-authorization.json
next_operator_step.planned_output_receipt=native-controlled-angle-attempt-03-smoke.json
next_operator_step.required_bench_clear_evidence=bench clear for exactly one Moza controlled-angle attempt 03: target 1 degree, max 5%, timeout 2000 ms, strategy pidff-bounded-effect, profile bounded-pidff-effect-lifecycle-v1, R5 stable, KS attached securely, hands clear, wheel clear, prior undertravel receipts preserved
```

The current input-role semantic evidence remains diagnostic:

```text
input_semantic_mapping_complete=false
semantic_status=partial_generic_aux_mapping
semantic_mapping_complete=false
semantic_candidate_count=6
ambiguous_semantic_candidate_count=4
unproven_required_role_count=3
readiness_claim=false
```

The current external-compatibility navigation is also diagnostic only:

```text
pit_house_compatibility.recorded_case_count=1
pit_house_compatibility.required_case_count=5
pit_house_compatibility.pit_house_coexistence_claimed=false
pit_house_compatibility.blocks_native_control=false
pit_house_compatibility.blocks_native_visible=false
simulator_compatibility.recorded_artifact_count=0
simulator_compatibility.required_artifact_count=2
simulator_compatibility.simulator_telemetry_claimed=false
simulator_compatibility.bounded_simulator_ffb_claimed=false
simulator_compatibility.blocks_native_control=false
simulator_compatibility.blocks_native_visible=false
```

The current smoke-ready state is incomplete:

```text
failed gates:
- native_actuator_visible_smoke
- pit_house_coexistence
- simulator_telemetry
- simulator_ffb_bounded
```

## Missing Work

1. Fresh command-bound bench-clear for attempt 03.
2. Exact attempt-03 authorization receipt.
3. Exactly one attempt-03 controlled-angle output receipt.
4. If attempt 03 passes, native-visible verifier, manifest promotion, and lane
   audit.
5. If attempt 03 fails, preserve receipt and record no-output analysis.
6. Pit House coexistence matrix.
7. Real simulator telemetry proof.
8. Bounded simulator FFB smoke receipt.
9. Smoke-ready verification, manifest promotion, and lane audit.

## Claim Boundary

This audit does not move the lane. It records that:

- `native_response_ready` is proven.
- `native_visible_ready` is not proven.
- `smoke_ready` is not proven.
- `release_ready` is false.
- No new authorization or hardware-output permission exists.
