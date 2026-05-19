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
- `frontier=repeated_safe_undertravel_attempt_03_preflight_recorded`
- `hardware_output_authorized=false`

The first 1 degree controlled-angle attempt and the retry both failed safely in
the same response band, about `0.181277` degrees. Both preserved bounded writes,
final Stop All cleanup, post-stop stability, no direct report `0x20`, no high
torque, no serial config, and no firmware or DFU.

Attempt 03 is only preflighted. `native-controlled-angle-attempt-03-preflight.json`
is a no-output dry run for `bounded-pidff-effect-lifecycle-v1`; it is not an
authorization receipt and it proves no motion.

## Completion Audit Summary

The broader Moza objective remains incomplete:

| Requirement | Current evidence | Status |
| --- | --- | --- |
| Passive enumeration, descriptor capture, parser fixtures | Lane passive receipts, parser fixture validation, passive verifier | Proven |
| Zero, watchdog, disconnect, low-torque, native response | Zero/openracing-control/native-response receipts and verifiers | Proven |
| Native visible motion | `verify-bundle --stage native-visible-ready` | Blocked: `native_actuator_visible_smoke` |
| Attempt-03 authorization | `native-controlled-angle-attempt-03-authorization.json` | Missing |
| Attempt-03 output | `native-controlled-angle-attempt-03-smoke.json` | Missing |
| Pit House coexistence | `pit-house-coexistence.json` | Missing |
| Simulator telemetry | `simulator-telemetry-proof.json` | Missing |
| Bounded simulator FFB | `simulator-ffb-smoke.json` | Missing |
| Smoke-ready promotion and audit | smoke-ready verifier, promotion, audit | Not eligible |

Input topology remains partially semantic: brake, clutch, and handbrake are
parser-visible through generic R5 V1 extended fields, but role-specific semantic
mapping remains diagnostic with `readiness_claim=false`.

## Required Next Event

The next native-visible step is blocked until Steven provides fresh
command-bound bench-clear for exactly this attempt:

```text
bench clear for exactly one Moza controlled-angle attempt 03: target 1 degree, max 5%, timeout 2000 ms, strategy pidff-bounded-effect, profile bounded-pidff-effect-lifecycle-v1, R5 stable, KS attached securely, hands clear, wheel clear, prior undertravel receipts preserved
```

Only after that exact evidence exists may an agent create
`native-controlled-angle-attempt-03-authorization.json`. The authorization still
does not prove motion. A matching authorization is required before exactly one
`native-controlled-angle-attempt-03-smoke.json` output attempt.

## Do Not Do

- Do not create an authorization receipt from this handoff.
- Do not run hardware output.
- Do not rerun either previous 1 degree attempt.
- Do not raise force, extend dwell, or jump to 3, 30, or 90 degrees.
- Do not use direct report `0x20`.
- Do not use high torque.
- Do not run serial config, firmware, or DFU flows.
- Do not treat Pit House, SimHub, simulator, or passive sniff evidence as native
  OpenRacing motion proof.

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

cargo run --locked -p wheelctl --bin wheelctl -- moza bench-wizard `
  --lane ci/hardware/moza-r5/2026-05-13 `
  --json-out target/moza-bench-wizard-current.json `
  --md-out target/moza-bench-wizard-current.md `
  --json
```

`verify-bundle --stage native-visible-ready` is expected to exit with code `4`
until a passing visible-motion receipt exists.

