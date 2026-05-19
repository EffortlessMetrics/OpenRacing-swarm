# Moza Native Visible Lane Handoff

Status: blocked
Last verified: 2026-05-19
Lane: `ci/hardware/moza-r5/2026-05-13`
Active goal: `.openracing/goals/active.toml`

This handoff exists because the active goal has no `ready` work item. The next
implementation steps are blocked by real hardware or external evidence, and
agents must not invent more no-output work to move the lane.

## Verified Frontier

The lane is currently `native_response_ready`.

Current verified state:

- `ready_for_native_control=true`
- `native_actuator_response_proven=true`
- `native_visible_motion_proven=false`
- `native_control_blocking_items=[]`
- `frontier=closed_loop_undertravel_recorded`
- `hardware_output_authorized=false`

The first 1 degree controlled-angle attempt, the reviewed retry, attempt 03,
and the closed-loop attempt all failed safely below the visible-motion
threshold. Attempt 03 used `bounded-pidff-effect-lifecycle-v1`, consumed the
exact command-bound authorization, sent four bounded PIDFF writes, timed out
before target, sent final Stop All, stayed post-stop stable, and recorded no
direct report `0x20`, no high torque, no serial config, and no firmware or DFU.
The closed-loop attempt used `closed-loop-pidff-angle-v1`, consumed its exact
command-bound authorization, recomputed bounded PIDFF force from live
steering-angle error, sent 672 bounded reports with zero write errors, timed out
at `angle_delta_degrees=0.13183794918745662`, sent final Stop All/final zero,
and stayed post-stop stable.

`native-controlled-angle-attempt-03-failure-analysis.json` classifies attempt 03
as safe undertravel and keeps native visible motion unclaimed.
`native-pidff-standard-path-diagnosis.json` classifies the standard PIDFF
controlled-angle path as ineffective in the current R5 device mode after three
same-band undertravel attempts. `native-controlled-angle-closed-loop-failure-analysis.json`
classifies the feedback-bounded attempt as safe undertravel and keeps native
visible motion unclaimed. Five passive sniff plan artifacts now exist under
`ci/hardware/sniff/moza-r5/2026-05-13`; they are plan-only protocol research
artifacts, not receipts or readiness claims. No further hardware output is
authorized.

## Completion Audit Summary

The broader Moza objective remains incomplete:

| Requirement | Current evidence | Status |
| --- | --- | --- |
| Passive enumeration, descriptor capture, parser fixtures | Lane passive receipts, parser fixture validation, passive verifier | Proven |
| Zero, watchdog, disconnect, low-torque, native response | Zero/openracing-control/native-response receipts and verifiers | Proven |
| Native visible motion | `verify-bundle --stage native-visible-ready` | Blocked: `native_actuator_visible_smoke` |
| Attempt-03 authorization | `native-controlled-angle-attempt-03-authorization.json` | Recorded and consumed |
| Attempt-03 output | `native-controlled-angle-attempt-03-smoke.json` | Recorded safe undertravel |
| Attempt-03 analysis | `native-controlled-angle-attempt-03-failure-analysis.json` | Recorded no-output classification |
| Standard PIDFF path diagnosis | `native-pidff-standard-path-diagnosis.json` | Recorded no-output architecture diagnosis |
| Closed-loop controlled-angle output | `native-controlled-angle-closed-loop-smoke.json` | Recorded safe undertravel |
| Closed-loop failure analysis | `native-controlled-angle-closed-loop-failure-analysis.json` | Recorded no-output classification |
| Vendor-control sniff plans | `ci/hardware/sniff/moza-r5/2026-05-13/*/sniff-plan.json` | Plan-only, non-claiming |
| Pit House coexistence | `pit-house-coexistence.json` | Missing |
| Simulator telemetry | `simulator-telemetry-proof.json` | Missing |
| Bounded simulator FFB | `simulator-ffb-smoke.json` | Missing |
| Smoke-ready promotion and audit | smoke-ready verifier, promotion, audit | Not eligible |

Input topology remains partially semantic: brake, clutch, and handbrake are
parser-visible through generic R5 V1 extended fields, but role-specific semantic
mapping remains diagnostic with `readiness_claim=false`.

## Required Next Event

The next native-visible step is no-output Moza vendor-specific protocol
investigation before any future output family. Passive USB sniff captures may
produce non-claiming `sniff-receipt.json`, `sniff-summary.json`, and bundle
manifest artifacts, but those are protocol/coexistence evidence, not native
readiness evidence. Preserve all four controlled-angle undertravel receipts and
their authorization, smoke, verification, analysis, standard-PIDFF diagnosis,
closed-loop failure analysis, and sniff plan artifacts. Do not create another
authorization or output receipt from verifier guidance. Any future output family
requires decoded protocol evidence, a reviewed vendor-control plan, fresh
command-bound bench clear, and a new exact authorization.

## Do Not Do

- Do not create another authorization receipt from this handoff.
- Do not run hardware output.
- Do not rerun attempt 03, the closed-loop attempt, or either previous 1 degree
  attempt.
- Do not keep iterating standard PIDFF profile variants without new protocol
  evidence.
- Do not raise force, extend dwell, or jump to 3, 5, 30, or 90 degrees.
- Do not use direct report `0x20`.
- Do not use high torque.
- Do not run serial config, firmware, or DFU flows.
- Do not treat Pit House, SimHub, simulator, or passive sniff evidence as native
  OpenRacing motion proof.
- Do not commit raw `.pcapng` captures unless a separate review approves the
  raw capture, size, sensitivity, and operator consent.

## Verification Commands

Use these no-output commands to refresh the handoff state:

```powershell
cargo run --locked -p wheelctl --bin wheelctl -- moza pre-output-readiness `
  --lane ci/hardware/moza-r5/2026-05-13 `
  --json-out target/moza-pre-output-current.json `
  --json

cargo run --locked -p wheelctl --bin wheelctl -- moza verify-bundle `
  --lane ci/hardware/moza-r5/2026-05-13 `
  --stage native-visible-ready `
  --json-out target/moza-native-visible-current.json `
  --json

cargo run --locked -p wheelctl --bin wheelctl -- moza artifact-index `
  --lane ci/hardware/moza-r5/2026-05-13 `
  --json-out target/moza-artifact-index-current.json `
  --md-out ci/hardware/moza-r5/2026-05-13/index.md `
  --json

cargo run --locked -p wheelctl --bin wheelctl -- moza bench-wizard `
  --lane ci/hardware/moza-r5/2026-05-13 `
  --json-out target/moza-bench-wizard-current.json `
  --md-out target/moza-bench-wizard-current.md `
  --json
```

`verify-bundle --stage native-visible-ready` is expected to exit with code `4`
until a passing visible-motion receipt exists.
