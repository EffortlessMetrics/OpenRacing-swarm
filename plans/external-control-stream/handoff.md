# External control-stream lane handoff

Status: active
Owner: platform/service
Created: 2026-07-17
Linked issue: #167
Linked proposal: docs/proposals/OR-PROP-0004-external-control-stream.md
Linked spec: docs/specs/OR-SPEC-0005-external-control-stream.md
Linked ADR: docs/adr/0010-non-rt-control-stream-boundary.md
Linked plan: plans/external-control-stream/implementation-plan.md

## Current state

The external-control-stream lane is active at
`.openracing/goals/active.toml`. The prior Moza goal was archived with its
blocked closeout at
`.openracing/goals/archive/2026-07-18-moza-native-visible-lane.toml` and
`plans/moza-native-visible-lane/closeout.md`; its hardware evidence and claim
boundaries remain preserved.

The domain contract (#168, merged PR #181), deterministic projector (#169,
merged PR #183), non-RT service slices (#177/#178, merged PRs #191/#188), and
versioned observe-only gRPC transport (#171, merged PR #193) are complete.
Diagnostics, capture, and deterministic replay (#172, merged PR #195) are also
complete at the hardware-free fixture/replay boundary. The next implementation
item is package composition and external contract publication (#179), followed
by artifact lifecycle and consumer smoke (#180). The merged transport and
diagnostics remain software contracts only; they do not imply Runbook product
support, output, FFB behavior, named controls, or physical/support claims.

PR #195 did not prove live capture/subscription from a running `wheeld`.
That live and packaged-artifact proof remains in #180 and must not be inferred
from the deterministic fixture/replay evidence.

## Work order

The activation gate is satisfied. Implement exactly one plan work item per PR
in order:

`#168 -> #169 -> #170 -> #171 -> #172 -> #179 -> #180`

The completed #170 epic is represented by #177 and #178. Items #168 through
#172 are now merged (#181, #183, #188/#191, #193, and #195 respectively).
Issue #173 and merged PR #176 define the deployment source truth; #174 remains
the parent epic. Issue #179 is the next one-PR implementation item, with open
PR #196 as candidate work that still requires review and exact-head proof.
Issue #180 remains blocked by #179. The external Runbook consumer is
`EffortlessMetrics/runbook-rs#41`, and remains a proof consumer rather than a
domain dependency.

## Deployment source-of-truth item

Issue #173 extended this lane with a package/release contract using the current
surfaces: `.github/workflows/release.yml`, `RELEASING.md`,
`packaging/linux/`, `packaging/windows/`, `packaging/macos/`, and
`crates/schemas/proto/` plus its Buf/build inputs. The current workflow builds
Linux and Windows artifacts; the macOS packaging directory exists but is not a
current workflow claim. That distinction must remain explicit until artifact
proof exists.

Issue #179 must first cover package composition, contract publication, and
coherence checks. Issue #180 must then cover installed `wheeld` feature
negotiation, descriptor / baseline / events / reset-gap / disconnect behavior,
schema and generated-client synchronization, prior-release upgrade and
rollback, missing or disabled service behavior, existing-client and safety
compatibility, and the input-only replay consumer. Neither item may broaden
output, FFB, high-torque, profile-mutation, physical-control, platform,
support, or readiness claims beyond its receipts.

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
