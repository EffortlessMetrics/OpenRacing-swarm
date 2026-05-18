# OR-PROP-0001: Moza native visible motion lane

Status: active
Owner: hardware
Created: 2026-05-18
Target milestone: native-visible-ready
Linked specs: docs/specs/OR-SPEC-0001-moza-native-visible-lane.md
Linked ADRs: docs/adr/0009-hardware-validation-evidence-state-machine.md
Linked plan: plans/moza-native-visible-lane/implementation-plan.md
Support/status impact: Moza R5 lane may advance from native_response_ready to native_visible_ready only with receipt-backed visible motion.
Policy impact: no new policy exception

## Problem

The Moza R5 lane has proven native response but not native visible motion. OpenRacing can enumerate the bench, parse passive captures, prove zero-output safety, initialize the native path, stream steering feedback, send bounded PIDFF records, observe measurable steering response, and clean up with Stop All. Two controlled-angle attempts still stayed in the same approximate 0.181277 degree response band, so the current blocker is PIDFF lifecycle/profile effectiveness rather than transport, steering feedback, or cleanup.

Without an activated source-of-truth lane, future agents can fall back to stale sprint notes or chat history and accidentally treat old passive, zero, Pit House, SimHub, simulator, or open-loop movement work as the next native-control step.

## Users and surfaces

This lane affects:

- operators running `wheelctl moza` on the Moza R5 + KS/ES + SR-P + HBP bench;
- reviewers checking `ci/hardware/moza-r5/2026-05-13` receipts;
- docs under `docs/hardware/`;
- verifier output for `native-visible-ready` and later `smoke-ready`;
- future work on controlled movement, external compatibility, simulator telemetry, and bounded simulator FFB.

## Success criteria

- The repo names the current frontier as repeated safe undertravel with attempt-03 preflight recorded.
- No source-of-truth artifact authorizes output by itself.
- A later hardware attempt is possible only through exact command-bound authorization and a fresh operator bench-clear.
- A passing native-visible receipt can promote `native_visible_ready` without requiring Pit House, SimHub, simulator telemetry, simulator FFB, or passive sniff artifacts.
- Smoke-ready remains broader external/system evidence and is not implied by native-visible success.

## Proposed shape

Activate a Moza native-visible source-of-truth lane with a proposal, behavior spec, implementation plan, and active goal. The active goal records that source-of-truth activation is complete, that attempt-03 authorization and output are blocked until operator conditions are met, and that promotion is blocked until a real visible-motion receipt passes.

## Alternatives considered

- Keep using `docs/NOW_NEXT_LATER.md` only. Rejected because it had stale passive and zero-torque bullets while the lane was already at `native_response_ready`.
- Put the current frontier only in the hardware docs. Rejected because agents are explicitly instructed to use `.openracing/goals/active.toml` for current work when it exists.
- Generate another output attempt directly from the plan. Rejected because the current evidence requires PIDFF lifecycle discipline and exact authorization, not blind retries.

## Specs to create or update

- `docs/specs/OR-SPEC-0001-moza-native-visible-lane.md`

## ADRs needed

- Existing ADR: `docs/adr/0009-hardware-validation-evidence-state-machine.md`

No new ADR is required for this source-of-truth activation because the durable validation state-machine decision already exists.

## Implementation campaign shape

1. Activate source-of-truth docs and metadata.
2. Keep attempt-03 authorization blocked until fresh command-bound bench-clear.
3. Run exactly one attempt-03 output only with a matching authorization receipt.
4. Promote native-visible only if the verifier accepts the receipt.
5. Continue controlled movement and external/simulator work as separate receipt-gated lanes.

## Evidence plan

- `python scripts/policy_file.py`
- `cargo run --locked -p openracing-tools --bin package-surface -- --check`
- `git diff --check`
- `wheelctl moza verify-bundle --stage native-visible-ready` for the current expected failure and later promotion proof.

## Risks

- Operators may treat an active goal as output permission. The active goal and spec therefore state that it authorizes no output.
- External compatibility evidence may be conflated with native control. The spec keeps Pit House, SimHub, simulator, and sniff artifacts outside native-visible gates.
- Attempt-specific artifacts may be overwritten. The plan requires indexed attempt-03 paths.

## Non-goals

- No hardware output.
- No authorization receipt.
- No direct report `0x20`, high torque, serial config, firmware, or DFU.
- No Pit House, SimHub, simulator, or passive sniff requirement for native visible motion.
- No smoke-ready or release-ready claim.

## Exit criteria

This proposal is complete when the lane either promotes to `native_visible_ready` from a verifier-accepted receipt, or records a stop decision explaining why the current standard PIDFF path is ineffective on this device/mode.

## Claim boundary

This proposal only defines and activates the Moza native-visible lane. It does not claim visible motion, controlled movement repeatability, external compatibility, simulator telemetry, bounded simulator FFB, smoke-ready, release-ready, high torque, or general Moza product support.
