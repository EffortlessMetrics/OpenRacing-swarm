# Moza R5 Controlled-Angle Analysis

This note classifies the first real controlled-angle attempt on Steven's Moza R5 lane. It is a no-output analysis artifact, not a new hardware run and not a readiness promotion.

## Source Evidence

The primary lane artifacts are:

| Artifact | Purpose |
| --- | --- |
| [native-controlled-angle-smoke.json](../../ci/hardware/moza-r5/2026-05-13/native-controlled-angle-smoke.json) | Preserved failed 1 degree output receipt |
| [native-controlled-angle-authorization.json](../../ci/hardware/moza-r5/2026-05-13/native-controlled-angle-authorization.json) | Consumed exact authorization for that one attempt |
| [native-controlled-angle-plan.json](../../ci/hardware/moza-r5/2026-05-13/native-controlled-angle-plan.json) | Non-claiming controlled-angle design surface |
| [native-visible-verification.json](../../ci/hardware/moza-r5/2026-05-13/native-visible-verification.json) | Verifier receipt showing native-visible still blocked |
| [native-controlled-angle-failure-analysis.json](../../ci/hardware/moza-r5/2026-05-13/native-controlled-angle-failure-analysis.json) | This no-output analysis artifact |

Related docs:

- [Moza R5 artifact checklist](moza-r5-artifact-checklist.md)
- [Moza R5 validation runbook](moza-r5-validation.md)
- [Moza validation matrix](moza-validation-matrix.md)
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

The no-output profile diagnosis is now recorded in the lane plan as
`bounded-pidff-micro-step-v2`.

1. Preserve `native-controlled-angle-smoke.json`.
2. Inspect the PIDFF set-effect, set-constant-force, effect-start, and Stop All sequence.
3. Use the revised bounded profile while keeping the first retry conservative: `target_degrees=1`, `max_percent=5`, `timeout_ms=2000`, repeated bounded PIDFF micro-steps, feedback stop, overshoot guard, final Stop All, and post-stop stability recording.
4. Keep `native-controlled-angle-plan.json` non-claiming: `planned_next_output.allowed=false` and `hardware_output_authorized=false`.
5. Authorize exactly one second attempt only after the reviewed profile lands and fresh command-bound bench-clear names `bounded-pidff-micro-step-v2`.

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
