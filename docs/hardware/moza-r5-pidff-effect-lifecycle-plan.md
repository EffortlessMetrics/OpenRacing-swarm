# Moza R5 PIDFF Effect Lifecycle Plan

`ci/hardware/moza-r5/2026-05-13/native-pidff-effect-lifecycle-plan.json`
is a no-output plan for the next standard PIDFF profile to review after
repeated safe undertravel on Steven's R5. This document is
`docs/hardware/moza-r5-pidff-effect-lifecycle-plan.md`. Neither artifact
authorizes hardware output.

The plan is based on the preserved receipts:

- `native-controlled-angle-smoke.json`
- `native-controlled-angle-retry-smoke.json`
- `native-pidff-semantics-diagnosis.json`
- `native-pidff-lifecycle-trace.json`

The lifecycle trace shows that `bounded-pidff-micro-step-v2` repeated the
current setup/start/Stop-All lifecycle: Set Effect, Set Constant Force, Effect
Operation Start, then Device Control Stop All, repeated eight times with one
final Stop All. That changed the write count from 5 to 33, but the measured
movement stayed in the same 0.181277 degree band.

The planned profile name is:

```text
bounded-pidff-effect-lifecycle-v1
```

The hypothesis is that the next useful change is the standard PIDFF effect
lifecycle, not more force or longer dwell. The profile should preserve the same
caps:

```text
target-degrees = 1
max-percent = 5
timeout-ms = 2000
strategy = pidff-bounded-effect
```

The proposed profile should prepare one effect lifecycle, avoid Stop All between
feedback samples unless a stop condition triggers, update bounded constant force
while steering feedback moves toward the 1 degree target, then send final Stop
All during cleanup.

The profile is now implemented for software preflight and exact-command binding,
but that still does not authorize hardware output. Before any output is allowed,
operators must generate a no-output dry-run receipt, create a matching
attempt-specific authorization receipt, and record fresh command-bound
bench-clear. Use attempt-specific artifacts so the preserved first and retry
receipts are not overwritten:

```text
native-controlled-angle-attempt-03-preflight.json
native-controlled-angle-attempt-03-authorization.json
native-controlled-angle-attempt-03-smoke.json
```

This plan authorizes no later hardware attempt. Direct report `0x20`, high
torque, serial config, firmware, DFU, longer dwell, and larger angle targets
remain forbidden by this plan.
