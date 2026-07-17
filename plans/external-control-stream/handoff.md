# External control-stream lane handoff

Status: proposed; activation required
Owner: platform/service
Created: 2026-07-17
Linked issue: #167
Linked proposal: docs/proposals/OR-PROP-0004-external-control-stream.md
Linked spec: docs/specs/OR-SPEC-0005-external-control-stream.md
Linked ADR: docs/adr/0010-non-rt-control-stream-boundary.md
Linked plan: plans/external-control-stream/implementation-plan.md

## Current state

The five-artifact source-of-truth scaffold is ready for review. The repository's
active goal remains `moza-native-visible-lane`; it is hardware-blocked at the
native-visible receipt gate and was not replaced or edited by this lane.

No runtime code, dependency, HID handle, network stream, output report, FFB
behavior, or public support claim changed in the scaffold.

## Activation gate

Do not begin #168 until maintainers explicitly activate this lane through the
documented active-goal lifecycle. Activation must preserve the current Moza
goal's archive/closeout state and must not leave multiple active manifests.

After activation, implement exactly one plan work item per PR in order:

`#168 -> #169 -> #170 -> #171 -> #172`

Deployment source-truth/package follow-ups are #173 and #174. The external
Runbook consumer is `EffortlessMetrics/runbook-rs#41`, and remains a proof
consumer rather than a domain dependency.

## Required review questions

- Does the domain remain generic and transport-neutral?
- Is the existing physical-device input owner reused?
- Are baselines, rotary losslessness, reset/gap, sequence, and provenance
  explicit?
- Are RT, output, FFB, named-control, and support claims still excluded?
- Does each runtime PR name exact files and matching proof?

## Proof for this scaffold

```text
cargo run --locked -p openracing-tools --bin validate-adr -- --verbose
cargo run --locked -p openracing-tools --bin package-surface -- --check
git diff --check
```

Missing tooling or skipped checks must remain visible in the PR report and are
not equivalent to a passing runtime or release proof.
