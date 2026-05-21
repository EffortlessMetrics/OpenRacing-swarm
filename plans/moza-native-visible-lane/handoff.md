# Moza Native Visible Lane Handoff

Status: blocked
Last verified: 2026-05-21
Lane: `ci/hardware/moza-r5/2026-05-13`
Active goal: `.openracing/goals/active.toml`

This handoff exists because the active goal has no `ready` work item. The next
implementation steps are blocked by real hardware or external evidence, and
agents must not invent more no-output churn to move the lane. No-output work is
only useful when it consumes checked-in evidence, tightens a gate, or preserves a
claim boundary.

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

The vendor-authority rail has now also run exactly one `estop_set_ffb` attempt
through the exact authorization gate. The consumed attempt sent only
`7E02461C0001F0`, consumed `vendor-authority-authorization.json`, and closed
hardware output authorization again. The separately authorized post-authority
PIDFF response receipt then recorded a lower response than the preserved
baseline: baseline `0.18127718013275285` degrees, post-authority
`0.032959487296864154` degrees, delta change `-0.1483176928358887` degrees.
`vendor-post-authority-pidff-response.json` classifies this as
`post_authority_pidff_response_regressed`. It is diagnostic evidence only, not
native-control or native-visible proof.

`native-controlled-angle-attempt-03-failure-analysis.json` classifies attempt 03
as safe undertravel and keeps native visible motion unclaimed.
`native-pidff-standard-path-diagnosis.json` classifies the standard PIDFF
controlled-angle path as ineffective in the current R5 device mode after three
same-band undertravel attempts. `native-controlled-angle-closed-loop-failure-analysis.json`
classifies the feedback-bounded attempt as safe undertravel and keeps native
visible motion unclaimed. `vendor-post-authority-pidff-response.json` extends
that diagnosis: the tested vendor-authority frame did not improve the comparable
standard PIDFF response under the same 5 percent / 2000 ms envelope.

Six passive sniff scenario plans now exist under
`ci/hardware/sniff/moza-r5/2026-05-13`. The `pit-house-open-idle` and
`pit-house-full-controls` scenarios have checked-in non-claiming sniff
receipts, classified summaries, and bundle manifests. The remaining
`pit-house-setting-change`, `simhub-open-idle`, `simhub-output-session`, and
`simulator-session-start-stop` scenarios remain navigation-only until matching
pcap receipts and summaries exist. No passive sniff artifact authorizes
hardware output or satisfies native-visible, smoke-ready, or coexistence gates.
No further hardware output is authorized.

`vendor-protocol-evidence-review.json` now records a no-output review across the
checked-in passive sniff summaries, command registry, consumed vendor-authority
attempt, and post-authority PIDFF comparison. It classifies the current state as
`estop_set_ffb_regressed_and_protocol_enable_path_still_undecoded`, records that
the current summaries do not expose a decoded output candidate, and keeps
`planned_next_output.allowed=false`.

The same review now distinguishes absence of decoded candidates from a decode
gap: the checked-in Pit House summaries include host-to-device USB transfers
that are not mapped to report IDs or payload candidates in the stored summary.
That gap is non-claiming protocol evidence and points the next no-output work at
raw pcap/tshark payload export review, not another output attempt.

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
| Consumed vendor-authority attempt | `vendor-authority-attempt.json` | Recorded exact one-frame non-claiming attempt |
| Post-authority PIDFF response | `vendor-post-authority-pidff-smoke.json`; `vendor-post-authority-pidff-response.json`; [post-authority PIDFF response diagnosis](../../docs/hardware/moza-r5-post-authority-pidff-response.md) | Recorded regression versus baseline; no native-visible claim |
| Vendor protocol evidence review | `vendor-protocol-evidence-review.json` | Recorded no-output review and host-to-device decode gap; protocol evidence still insufficient for any output plan |
| Passive sniff protocol evidence | `pit-house-open-idle`, `pit-house-full-controls` sniff receipts, summaries, and bundle manifests | Recorded non-claiming evidence |
| Remaining passive sniff plans | `pit-house-setting-change`, `simhub-open-idle`, `simhub-output-session`, `simulator-session-start-stop` sniff plans | Plan-only, non-claiming |
| Pit House coexistence | `pit-house-coexistence.json` | Missing |
| Simulator telemetry | `simulator-telemetry-proof.json` | Missing |
| Bounded simulator FFB | `simulator-ffb-smoke.json` | Missing |
| Smoke-ready promotion and audit | smoke-ready verifier, promotion, audit | Not eligible |

Input topology remains partially semantic: steering, throttle, brake, HBP
handbrake, KS rim controls, and ES rim controls are parser-proven. SR-P clutch
is parser-visible through two live R5 V1 extended auxiliary fields, but the
role-specific clutch mapping remains diagnostic with `readiness_claim=false`.

## Required Next Event

The next operator step remains review-only: current evidence has recorded the
post-authority PIDFF regression and reviewed the checked-in protocol evidence
without finding a decoded enable path. It also records that existing Pit House
host-to-device traffic is not yet mapped to report IDs in the checked-in
summaries. It emits no hardware output command and no authorization command.
The next implementation work should continue vendor-specific protocol
investigation, such as raw pcap/tshark payload export review, remaining passive
sniff scenarios, or decoded report review, before any future motion ladder plan.

Passive USB sniff captures may produce non-claiming `sniff-receipt.json`,
`sniff-summary.json`, and bundle manifest artifacts, but those are
protocol/coexistence evidence, not native readiness evidence. Preserve all four
controlled-angle undertravel receipts, the consumed vendor-authority attempt,
the post-authority PIDFF response receipts, the protocol evidence review, and
their analysis/diagnosis artifacts. Do not create another authorization or
output receipt from verifier guidance. Any future output family requires decoded
protocol evidence, a
reviewed vendor-control plan, fresh command-bound bench clear, and a new exact
authorization.

## Do Not Do

- Do not create another authorization receipt from this handoff.
- Do not run hardware output.
- Do not rerun attempt 03, the closed-loop attempt, or either previous 1 degree
  attempt.
- Do not retry `estop_set_ffb` or reuse the consumed vendor-authority attempt.
- Do not rerun the post-authority PIDFF response capture.
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
