# Moza native visible implementation plan

Status: active
Owner: hardware
Linked proposal: docs/proposals/OR-PROP-0001-moza-native-visible-lane.md
Linked specs:
- docs/specs/OR-SPEC-0001-moza-native-visible-lane.md
Linked ADRs: docs/adr/0009-hardware-validation-evidence-state-machine.md
Active goal: .openracing/goals/active.toml

## Current state

The Moza R5 lane at `ci/hardware/moza-r5/2026-05-13` is `native_response_ready`.
The artifact index records frontier
`repeated_safe_undertravel_attempt_03_preflight_recorded`, highest passing stage
`native_response_ready`, next required stage `native_visible_ready`,
`native_actuator_response_proven=true`, `native_visible_motion_proven=false`,
and `release_ready=false`.

Two real controlled-angle output receipts are preserved. The first 1 degree
attempt sent five bounded PIDFF writes and the reviewed retry sent 33 bounded
PIDFF writes. Both had zero write errors, final Stop All sent, post-stop
stability, and about 0.181277 degrees of steering delta. They are useful safe
undertravel evidence, not visible-motion proof.

`native-pidff-lifecycle-trace.json` and
`native-pidff-effect-lifecycle-plan.json` record the no-output PIDFF diagnosis.
`native-controlled-angle-attempt-03-preflight.json` records the software-only
dry-run for `bounded-pidff-effect-lifecycle-v1`. No attempt-03 authorization or
output receipt exists yet.

## Work item: activate-source-of-truth

Status: completed
Linked proposal: docs/proposals/OR-PROP-0001-moza-native-visible-lane.md
Linked spec: docs/specs/OR-SPEC-0001-moza-native-visible-lane.md
Linked ADR: docs/adr/0009-hardware-validation-evidence-state-machine.md
Blocks: source-of-truth guided lane work
Blocked by: n/a

### Goal

Create the proposal, spec, implementation plan, active goal, and sprint status
update that identify the current Moza native-visible frontier without relying
on chat history.

### Production delta

Add source-of-truth docs and metadata. Refresh `docs/NOW_NEXT_LATER.md` so it
no longer names passive or zero-torque Moza work as the current hardware step.

### Non-goals

No hardware output, no authorization receipt, no verifier promotion, no hardware
artifact replacement, and no output code change.

### Acceptance

- `.openracing/goals/active.toml` points to this plan and spec.
- `docs/NOW_NEXT_LATER.md` names the current native-visible frontier.
- The claim boundary says attempt-03 preflight is non-authorizing.

### Proof commands

```powershell
python scripts/policy_file.py
cargo run --locked -p openracing-tools --bin package-surface -- --check
git diff --check
```

### Rollback

Remove the added source-of-truth files and restore `docs/NOW_NEXT_LATER.md` to
the previous text.

## Work item: attempt-03-authorization

Status: blocked
Linked proposal: docs/proposals/OR-PROP-0001-moza-native-visible-lane.md
Linked spec: docs/specs/OR-SPEC-0001-moza-native-visible-lane.md
Linked ADR: docs/adr/0009-hardware-validation-evidence-state-machine.md
Blocks: attempt-03-output
Blocked by: fresh command-bound operator bench-clear for attempt 03

### Goal

Create one exact authorization receipt for attempt 03 only after the operator
provides fresh bench-clear evidence for the exact command.

### Production delta

Run no-output readiness and native-visible verification first. Then create
`native-controlled-angle-attempt-03-authorization.json` only if the command shape
matches the preflight and the operator evidence is command-bound.

Expected command shape after fresh bench-clear:

```powershell
wheelctl moza authorize-controlled-angle-output `
  --lane ci/hardware/moza-r5/2026-05-13 `
  --device hid-0x346E-0x0004-if2-0x0001-0x0004 `
  --operator Steven `
  --bench-clear-evidence "<fresh command-bound attempt-03 bench-clear>" `
  --prior-response-proof ci/hardware/moza-r5/2026-05-13/native-actuator-visible-smoke.json `
  --prior-actuator-proof ci/hardware/moza-r5/2026-05-13/native-actuator-profile-smoke.json `
  --steering-proof ci/hardware/moza-r5/2026-05-13/steering-angle-stream-proof.json `
  --controlled-angle-preflight ci/hardware/moza-r5/2026-05-13/native-controlled-angle-attempt-03-preflight.json `
  --planned-output ci/hardware/moza-r5/2026-05-13/native-controlled-angle-attempt-03-smoke.json `
  --target-degrees 1 `
  --profile bounded-pidff-effect-lifecycle-v1 `
  --strategy pidff-bounded-effect `
  --max-percent 5 `
  --timeout-ms 2000 `
  --json-out ci/hardware/moza-r5/2026-05-13/native-controlled-angle-attempt-03-authorization.json `
  --json
```

### Non-goals

No hardware output, no motion claim, no force increase, no longer dwell, no
larger angle, and no external compatibility claim.

### Acceptance

- Authorization binds lane, device, profile, target, max percent, timeout,
  strategy, prior proofs, preflight, planned output path, and operator evidence.
- The prior undertravel receipts remain preserved.

### Proof commands

```powershell
wheelctl moza pre-output-readiness --lane ci/hardware/moza-r5/2026-05-13 --json-out target/moza-pre-output-before-attempt-03.json --json
wheelctl moza verify-bundle --lane ci/hardware/moza-r5/2026-05-13 --stage native-visible-ready --json-out target/moza-native-visible-before-attempt-03.json --json
```

### Rollback

Delete the new authorization receipt if no output has consumed it. If output has
run, preserve all receipts and record analysis instead of deleting evidence.

## Work item: attempt-03-output

Status: blocked
Linked proposal: docs/proposals/OR-PROP-0001-moza-native-visible-lane.md
Linked spec: docs/specs/OR-SPEC-0001-moza-native-visible-lane.md
Linked ADR: docs/adr/0009-hardware-validation-evidence-state-machine.md
Blocks: native-visible-promotion or attempt-03-analysis
Blocked by: matching attempt-03 authorization receipt

### Goal

Run exactly one 1 degree, 5 percent, 2000 ms controlled-angle attempt using
`bounded-pidff-effect-lifecycle-v1`.

### Production delta

Create `native-controlled-angle-attempt-03-smoke.json` from one hardware output
command. The command must stop on target, timeout, wrong-way/overshoot guard, no
steering samples, write error, or cleanup condition, and must always send final
Stop All.

### Non-goals

No rerun, no 3/5/10/30/90 degree step, no force increase, no dwell extension,
no direct report `0x20`, no high torque, no serial config, no firmware, and no
DFU.

### Acceptance

- Exactly one output command runs.
- Receipt records target status, angle delta, writes, write errors, final Stop
  All, post-stop stability, authorization consumption, and forbidden path
  booleans.

### Proof commands

```powershell
wheelctl moza controlled-angle-smoke `
  --device hid-0x346E-0x0004-if2-0x0001-0x0004 `
  --lane ci/hardware/moza-r5/2026-05-13 `
  --prior-actuator-proof ci/hardware/moza-r5/2026-05-13/native-actuator-profile-smoke.json `
  --steering-proof ci/hardware/moza-r5/2026-05-13/steering-angle-stream-proof.json `
  --authorization-proof ci/hardware/moza-r5/2026-05-13/native-controlled-angle-attempt-03-authorization.json `
  --target-degrees 1 `
  --profile bounded-pidff-effect-lifecycle-v1 `
  --max-percent 5 `
  --timeout-ms 2000 `
  --strategy pidff-bounded-effect `
  --confirm-controlled-angle `
  --json-out ci/hardware/moza-r5/2026-05-13/native-controlled-angle-attempt-03-smoke.json `
  --json
```

### Rollback

Do not delete a hardware-output receipt. If the attempt fails, preserve it and
add analysis.

## Work item: native-visible-promotion

Status: blocked
Linked proposal: docs/proposals/OR-PROP-0001-moza-native-visible-lane.md
Linked spec: docs/specs/OR-SPEC-0001-moza-native-visible-lane.md
Linked ADR: docs/adr/0009-hardware-validation-evidence-state-machine.md
Blocks: controlled movement ladder
Blocked by: passing attempt-03 native-visible receipt

### Goal

Promote the lane to `native_visible_ready` only if the verifier accepts a real
visible-motion receipt.

### Production delta

Create native-visible verification, manifest promotion, and lane audit receipts.

### Non-goals

No smoke-ready, release-ready, high-torque, Pit House, SimHub, simulator
telemetry, simulator FFB, or passive sniff claim.

### Acceptance

- `verify-bundle --stage native-visible-ready` passes.
- Manifest promotion records native visible state without simulator or release
  claims.
- Lane audit passes for native-visible.

### Proof commands

```powershell
wheelctl moza verify-bundle --lane ci/hardware/moza-r5/2026-05-13 --stage native-visible-ready --json-out ci/hardware/moza-r5/2026-05-13/native-visible-verification.json --json
wheelctl moza promote-manifest --lane ci/hardware/moza-r5/2026-05-13 --stage native-visible-ready --json-out ci/hardware/moza-r5/2026-05-13/manifest-promotion-native-visible.json --json
wheelctl moza audit-lane --lane ci/hardware/moza-r5/2026-05-13 --stage native-visible-ready --json-out ci/hardware/moza-r5/2026-05-13/lane-audit-native-visible.json --json
```

### Rollback

If promotion was incorrect, add a corrective PR that preserves the faulty
receipt and demotes the manifest with analysis. Do not erase evidence.

## Later work

- Repeat 1 degree, then 3, 5, 10, 30, 90 right, and 90 return controlled
  movement in separately authorized rungs.
- Refresh no-output KS/SR-P/HBP input captures as needed.
- Use passive USB sniffing for Pit House, SimHub, and simulator protocol
  research without readiness claims.
- Record Pit House coexistence, simulator telemetry, and bounded simulator FFB
  as external/smoke gates after native-visible work is settled.
