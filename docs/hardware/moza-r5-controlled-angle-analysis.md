# Moza R5 Controlled-Angle Analysis

This note classifies the real controlled-angle attempts on Steven's Moza R5 lane. It is a no-output analysis artifact, not a new hardware run and not a readiness promotion.

## Source Evidence

The primary lane artifacts are:

| Artifact | Purpose |
| --- | --- |
| [native-controlled-angle-smoke.json](../../ci/hardware/moza-r5/2026-05-13/native-controlled-angle-smoke.json) | Preserved failed 1 degree output receipt |
| [native-controlled-angle-authorization.json](../../ci/hardware/moza-r5/2026-05-13/native-controlled-angle-authorization.json) | Consumed exact authorization for that one attempt |
| [native-controlled-angle-plan.json](../../ci/hardware/moza-r5/2026-05-13/native-controlled-angle-plan.json) | Non-claiming controlled-angle design surface |
| [native-visible-verification.json](../../ci/hardware/moza-r5/2026-05-13/native-visible-verification.json) | Verifier receipt showing native-visible still blocked |
| [native-controlled-angle-failure-analysis.json](../../ci/hardware/moza-r5/2026-05-13/native-controlled-angle-failure-analysis.json) | This no-output analysis artifact |
| [native-controlled-angle-retry-preflight.json](../../ci/hardware/moza-r5/2026-05-13/native-controlled-angle-retry-preflight.json) | No-output preflight for the reviewed retry profile |
| [native-controlled-angle-retry-authorization.json](../../ci/hardware/moza-r5/2026-05-13/native-controlled-angle-retry-authorization.json) | Consumed exact authorization for the second attempt |
| [native-controlled-angle-retry-smoke.json](../../ci/hardware/moza-r5/2026-05-13/native-controlled-angle-retry-smoke.json) | Preserved failed retry output receipt |
| [native-controlled-angle-retry-failure-analysis.json](../../ci/hardware/moza-r5/2026-05-13/native-controlled-angle-retry-failure-analysis.json) | No-output retry analysis artifact |
| [native-pidff-semantics-diagnosis.json](../../ci/hardware/moza-r5/2026-05-13/native-pidff-semantics-diagnosis.json) | No-output PIDFF semantics/profile diagnosis |
| [native-controlled-angle-closed-loop-preflight.json](../../ci/hardware/moza-r5/2026-05-13/native-controlled-angle-closed-loop-preflight.json) | No-output closed-loop 1 degree profile preflight |
| [native-controlled-angle-closed-loop-smoke.json](../../ci/hardware/moza-r5/2026-05-13/native-controlled-angle-closed-loop-smoke.json) | Preserved failed closed-loop output receipt |
| [native-controlled-angle-closed-loop-failure-analysis.json](../../ci/hardware/moza-r5/2026-05-13/native-controlled-angle-closed-loop-failure-analysis.json) | No-output closed-loop failure analysis |

Related docs:

- [Moza R5 artifact checklist](moza-r5-artifact-checklist.md)
- [Moza R5 validation runbook](moza-r5-validation.md)
- [Moza validation matrix](moza-validation-matrix.md)
- [PIDFF semantics diagnosis](moza-r5-pidff-semantics-diagnosis.md)
- [Moza R5 live testing roadmap](moza-r5-live-testing-roadmap.md)
- [Moza R5 simulator smoke](moza-r5-simulator-smoke.md)
- [Moza R5 lane README](../../ci/hardware/moza-r5/README.md)

## Classification

The failure is classified as `safe_undertravel`.

| Field | Value | Reading |
| --- | ---: | --- |
| `target_degrees` | `1.0` | First visible-motion rung |
| `angle_delta_degrees` | `0.18127718013275285` | Measurable movement, below threshold |
| `undertravel_degrees` | `0.8187228198672471` | Remaining delta to the 1 degree target |
| `target_reached` | `false` | Native visible motion is not proven |
| `movement_observed` | `false` | Operator-visible movement is not claimed |
| `timeout_reached` | `true` | Profile did not reach target inside 2000 ms |
| `writes_ok` | `5` | PIDFF write path worked |
| `write_errors` | `0` | No transport write failure indicated |
| `steering_sample_count` | `984` | Feedback samples were available |
| `final_stop_all_sent` | `true` | Cleanup path worked |
| `post_stop_stable` | `true` | No runaway or post-stop instability indicated |

This receipt does not prove `native-visible-ready`, but it is useful evidence: the output path, feedback observation, and cleanup path remained controlled.

It also does not prove `smoke-ready`, simulator compatibility, Pit House coexistence, SimHub compatibility, high torque, or release readiness. Simulator smoke remains separate and still depends on later native-visible and simulator telemetry evidence.

## Likely Causes

The receipt points at profile authority or PIDFF effect semantics, not a broken lane:

- bounded profile authority too low
- effect shape not producing sustained motion
- PIDFF effect semantics require a different operation sequence
- feedback loop stops or settles before reaching the visible threshold

The receipt does not indicate a write-path failure, unavailable steering feedback, overshoot, wrong-way motion, cleanup failure, high torque, direct report `0x20`, serial config, firmware, or DFU activity.

## Required Next Work

The first controlled-angle receipt and the reviewed retry receipt are both
preserved. The retry used `bounded-pidff-micro-step-v2` and stayed in the same
0.181 degree response band despite increasing PIDFF write count from 5 to 33.

1. Preserve `native-controlled-angle-smoke.json` and `native-controlled-angle-retry-smoke.json`.
2. Treat `native-pidff-semantics-diagnosis.json` as the current no-output diagnosis artifact.
3. Treat `native-pidff-lifecycle-trace.json` as the current decoded lifecycle artifact.
4. Use the trace to inspect the set-effect, set-constant-force, effect-start, Stop All, gain, duration, and device-control sequence before any future exact authorization.
5. Treat `native-pidff-effect-lifecycle-plan.json` as the current no-output profile plan.
6. Keep `native-controlled-angle-plan.json` non-claiming: `planned_next_output.allowed=false` and `hardware_output_authorized=false`.

## Forbidden Next Steps

Do not use this analysis to authorize:

- blind rerun
- force escalation without a reviewed plan
- `5% / 3000 ms` dwell
- `5% / 30000 ms` dwell
- `90 degree` test
- high torque
- direct report `0x20`
- serial config
- firmware or DFU
- Pit House or SimHub as a native-motion prerequisite
- passive sniff artifacts as native readiness evidence

Native visible motion remains blocked on the `native_actuator_visible_smoke` gate until a later authorized receipt reaches the visible threshold and satisfies the verifier.

## Retry Result

The second controlled-angle attempt used the reviewed
`bounded-pidff-micro-step-v2` profile with the same `target_degrees=1`,
`max_percent=5`, and `timeout_ms=2000` limits. It also failed as
`safe_undertravel_retry_same_response_band`.

| Field | Value | Reading |
| --- | ---: | --- |
| `angle_delta_degrees` | `0.18127718013275285` | Same measured response band as the first attempt |
| `target_reached` | `false` | Native visible motion is still not proven |
| `timeout_reached` | `true` | Retry did not reach target inside 2000 ms |
| `writes_ok` | `33` | Retry profile increased PIDFF write count without write errors |
| `write_errors` | `0` | No transport write failure indicated |
| `steering_sample_count` | `690` | Feedback samples were available |
| `final_stop_all_sent` | `true` | Cleanup path worked |
| `post_stop_stable` | `true` | No runaway or post-stop instability indicated |

The retry receipt is useful evidence, but it does not authorize a third
attempt. The no-output PIDFF semantics diagnosis and lifecycle trace are now
recorded; `native-pidff-lifecycle-trace.json` decodes the command lifecycle
from the preserved receipts without opening HID or sending writes. Do not raise
force, extend dwell, or move to larger angle targets from this receipt.

## PIDFF Semantics Diagnosis

The no-output PIDFF diagnosis is now recorded in
`native-pidff-semantics-diagnosis.json`. It classifies the current evidence as
`same_response_band_despite_micro_step_replay`: the retry increased writes from
5 to 33 but did not increase measured steering delta beyond the same 0.181
degree band.

Those artifacts still authorize no output. The reviewed
[PIDFF effect lifecycle plan](moza-r5-pidff-effect-lifecycle-plan.md) names
`bounded-pidff-effect-lifecycle-v1` as the next software profile. It is
implemented for no-output preflight and exact-command binding, but it is not an
authorization receipt.

## Closed-Loop Native Preflight

The current no-output successor profile is `closed-loop-pidff-angle-v1`,
recorded in `native-controlled-angle-closed-loop-preflight.json`. It is still
bounded to `target_degrees=1`, `max_percent=5`, `timeout_ms=2000`, and
`strategy=pidff-bounded-effect`.

Unlike the earlier open-loop or fixed-lifecycle attempts, this profile samples
the start angle, computes target error from live steering feedback, recomputes
conservative PIDFF constant-force commands from that error, returns toward the
start angle, and sends final Stop All. The preflight opened no HID device, sent
no output reports, and wrote no FFB commands; it proves only software planning
and receipt shape.

The first real closed-loop attempt is now preserved as
`native-controlled-angle-closed-loop-smoke.json`. It still failed safely as
undertravel: `target_reached=false`, `timeout_reached=true`,
`angle_delta_degrees=0.13183794918745662`, `writes_ok=672`,
`write_errors=0`, `final_stop_all_sent=true`, `final_zero_sent=true`, and
`post_stop_stable=true`. The no-output classification is recorded in
`native-controlled-angle-closed-loop-failure-analysis.json`.

Pit House, SimHub, and simulator telemetry are not prerequisites for this
native-control path. They remain separate coexistence and external-compatibility
evidence. The closed-loop receipt did not promote `native-visible-ready`; the
verifier still fails until real hardware evidence reaches the visible-motion
threshold. Further output requires new protocol evidence, a reviewed plan, fresh
command-bound bench-clear, and a new exact authorization.
