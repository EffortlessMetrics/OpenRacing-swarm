# OR-SPEC-0001: Moza native visible lane

Status: active
Owner: hardware
Created: 2026-05-18
Linked proposal: docs/proposals/OR-PROP-0001-moza-native-visible-lane.md
Linked ADRs: docs/adr/0009-hardware-validation-evidence-state-machine.md
Linked plan: plans/moza-native-visible-lane/implementation-plan.md
Linked issues: n/a
Linked PRs: n/a
Support-tier impact: native-visible evidence may raise the Moza R5 lane within checked-in hardware support docs, but not to smoke-ready or release-ready.
Policy impact: no new policy exception

## Scope

This spec defines the native-visible lane contract for the Moza R5 + KS/ES + SR-P + HBP hardware lane at `ci/hardware/moza-r5/2026-05-13`.

The current frontier is:

```text
native_response_ready
-> controlled_angle_attempt_03_recorded
-> native_visible_ready still blocked
```

## Required behavior

The lane MUST preserve the first controlled-angle receipt, the reviewed retry receipt, attempt 03, their authorization receipts, and their failure analysis artifacts. These attempts are evidence because they show bounded PIDFF writes, steering feedback, final Stop All cleanup, and post-stop stability, but they MUST NOT satisfy native visible motion because all stayed below the 1 degree visible threshold.

The lane MUST keep `native_visible_motion_proven=false` until a same-lane visible-motion receipt passes the verifier. A dry-run, preflight, plan, lifecycle trace, artifact index, bench wizard receipt, passive sniff bundle, Pit House receipt, SimHub receipt, simulator telemetry proof, or simulator FFB smoke receipt MUST NOT satisfy native-visible readiness.

The verifier MUST allow native-visible progression without requiring Pit House, SimHub, simulator telemetry, simulator FFB, or passive sniff artifacts. Those artifacts belong to external compatibility, support, protocol research, or smoke-ready work.

Any future output attempt MUST be exact-command authorized before it runs. Authorization MUST bind the lane, device selector, target degrees, max percent, timeout, strategy, profile, prior proofs, planned output path, and fresh command-bound bench-clear evidence. Authorization MUST NOT be inferred from generic "bench clear" text, a dry-run, an active goal, a plan, or a verifier failure.

An output attempt MUST run at most once per authorization. It MUST preserve the receipt, record whether target motion was reached, record write attempts and errors, send final Stop All, record post-stop stability, and refuse direct report `0x20`, high torque, serial config, firmware, and DFU paths.

Native-visible promotion MUST run the native-visible verifier, manifest promotion, and lane audit. It MUST NOT claim smoke-ready, release-ready, high-torque validation, simulator validation, or external compatibility.

## Current attempt-03 contract

Attempt 03 is based on:

- `native-pidff-lifecycle-trace.json`
- `native-pidff-effect-lifecycle-plan.json`
- `native-controlled-angle-attempt-03-preflight.json`

The only profile named by the current no-output preflight is:

```text
bounded-pidff-effect-lifecycle-v1
```

Attempt 03 has been run exactly once. `native-controlled-angle-attempt-03-authorization.json` is consumed, `native-controlled-angle-attempt-03-smoke.json` records safe undertravel, `native-controlled-angle-attempt-03-failure-analysis.json` records the no-output attempt classification, and `native-pidff-standard-path-diagnosis.json` records the standard PIDFF controlled-angle path diagnosis. The attempt authorizes no further output.

The command-bound bench-clear evidence consumed by attempt 03 was:

```text
bench clear for exactly one Moza controlled-angle attempt 03: target 1 degree, max 5%, timeout 2000 ms, strategy pidff-bounded-effect, profile bounded-pidff-effect-lifecycle-v1, R5 stable, KS attached securely, hands clear, wheel clear, prior undertravel receipts preserved
```

Generic `bench clear` text is not sufficient for this attempt.

## Acceptance examples

### Current verifier failure

Given the current lane, `wheelctl moza verify-bundle --stage native-visible-ready` SHOULD fail because the native visible motion gate is still blocked after attempt 03. The failure SHOULD NOT include a generated output command.

### Passing visible-motion receipt

Given a future same-lane receipt with target reached, visible threshold met, no write errors, final Stop All sent, post-stop stable, and all forbidden paths false, the verifier MAY pass `native-visible-ready`.

### Non-claiming diagnostics

Given `native-pidff-lifecycle-trace.json`, `native-pidff-effect-lifecycle-plan.json`, `native-pidff-standard-path-diagnosis.json`, `native-controlled-angle-attempt-03-preflight.json`, `native-controlled-angle-attempt-03-authorization.json`, the failed `native-controlled-angle-attempt-03-smoke.json`, `native-controlled-angle-attempt-03-failure-analysis.json`, `index.md`, or a bench-wizard receipt, the verifier MUST keep native-visible blocked unless a real output receipt also passes.

Given the standard PIDFF diagnosis, the next investigation path MUST be
no-output Moza vendor-specific enable/control research. It MAY sniff Pit House
or SimHub, decode vendor reports, map report IDs, identify enable/gain/mode
handshakes, and design a reviewed vendor-control plan, but it MUST NOT create an
authorization receipt or hardware-output receipt by itself.

Given plan-only passive sniff artifacts under
`ci/hardware/sniff/moza-r5/<date>/`, artifact-index MAY surface the plans as
`present_non_claiming`, but it MUST keep the scenario unrecorded until matching
`sniff-receipt.json` and `sniff-summary.json` artifacts exist. Plan-only sniff
artifacts MUST NOT satisfy native-visible, smoke-ready, or release-ready gates.

Given a stored verifier receipt that is syntactically valid but failed its requested stage, the artifact index MAY mark the file itself as `pass`, but it MUST expose the claim status as `stage_failed`. Artifact validity MUST NOT be treated as native-visible readiness.

## Proof requirements

Source-of-truth or docs-only changes MUST run:

```powershell
python scripts/policy_file.py
cargo run --locked -p openracing-tools --bin package-surface -- --check
git diff --check
```

Verifier or CLI behavior changes MUST also run focused `wheelctl` tests named by the implementation plan.

Hardware output work MUST run no-output readiness first and MUST stop after exactly one authorized command.

## Implementation mapping

- Active goal: `.openracing/goals/active.toml`
- Plan: `plans/moza-native-visible-lane/implementation-plan.md`
- Lane index: `ci/hardware/moza-r5/2026-05-13/index.md`
- CLI surface: `crates/cli/src/commands/moza.rs`
- Operator docs: `docs/hardware/moza-r5-validation.md`

## Non-goals

- No hardware output from this spec.
- No automatic rerun guidance.
- No force increase, dwell extension, 30 degree, or 90 degree attempt.
- No direct report `0x20`, high torque, serial config, firmware, or DFU.
- No Pit House, SimHub, simulator, or passive sniff dependency for native control.
- No smoke-ready or release-ready promotion.
