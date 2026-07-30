# External control-stream implementation plan

Status: active
Owner: platform/service
Linked proposal: docs/proposals/OR-PROP-0004-external-control-stream.md
Linked specs:
- docs/specs/OR-SPEC-0005-external-control-stream.md
Linked ADRs:
- docs/adr/0010-non-rt-control-stream-boundary.md
Active goal: .openracing/goals/active.toml

## Lane rules

The lane was activated through the documented active-goal lifecycle after the
Moza manifest was archived with its blocked closeout. Each item below remains
one independently shippable runtime PR. Do not combine items or broaden
hardware/support claims.

## Work item: device-input-domain-contract

Status: completed
Linked issue: #168
Merged PR: #181
Target seams: `crates/openracing-device-types/src/lib.rs`, focused crate tests,
engine compatibility re-exports only if required.

Goal: correct the canonical 0..=127 button contract and add transport-neutral
control descriptor/state/event types with raw/candidate/validated provenance.

Acceptance: one canonical `DeviceInputs`; button boundary tests; serializable
domain types without protobuf/tonic; no HID polling or output behavior.

Proof: `python scripts/cargo_fmt_workspace.py`, targeted
`cargo test --locked -p openracing-device-types`, engine API contract tests,
strict crate clippy, package-surface check, policy check, and `git diff --check`.

Claim boundary: decoded software-domain correctness only; no physical role,
Runbook, transport, output, or FFB claim.

Rollback: revert the domain type/re-export/test changes; leave service and IPC
unchanged.

## Work item: deterministic-input-projection

Status: completed
Linked issue: #169
Merged PR: #183
Target seams: the activated vendor-neutral domain module and focused virtual
snapshot/property tests.

Goal: project snapshots into baseline, button/hat edges, lossless rotary
events, reset, and disconnect with deterministic ordering and epoch sequences.

Acceptance: baseline emits no actions; buttons through 127; hat neutral;
duplicate snapshots; three queued rotary ticks remain +3; reset clears state;
malformed/out-of-order producer data is explicit.

Proof: exact plan-targeted domain tests, formatter, strict clippy,
package-surface, policy, and `git diff --check`.

Claim boundary: pure projection from decoded input; no service polling,
protobuf, Runbook, physical compatibility, or RT change.

Rollback: revert the projector and its fixtures without changing device owner
or transport.

## Work item: non-rt-input-service

Status: completed
Linked issue: #170
Implementation issues: #177, #178
Merged PRs: #188, #191
Target seams: `crates/service/src/device_service.rs`, a focused control-input
service module, lifecycle wiring, and virtual service tests.

Goal: collect from the existing input owner on a non-RT worker and broadcast
descriptor, baseline, events, reset, and disconnect through bounded subscribers.

Acceptance: no second HID handle; deterministic connect/reconnect/shutdown;
multiple subscribers; explicit lag reset/gap or typed termination; metrics for
drops/resets; `read_inputs() == None` handled.

Proof: service virtual-device matrix, formatter, strict service clippy/tests,
package-surface, policy, and `git diff --check`.

Claim boundary: non-RT collection from decoded/virtual inputs; no transport,
physical role, Runbook, output, or FFB claim.

Rollback: remove service registration and keep the existing device lifecycle
and profile behavior unchanged.

Issue #170 remains the parent epic for the completed service slices. The
implementation evidence is split across #177 (single-owner collection) and
#178 (bounded broadcasting), with #191 and #188 providing the merged PRs.

## Work item: versioned-control-stream-transport

Status: completed
Linked issue: #171
Merged PR: #193
Target seams: existing schema/proto and `crates/service/src/ipc_service.rs`,
with feature negotiation tests.

Goal: expose a versioned read-only gRPC control stream with descriptor,
baseline, ordered events, reset/disconnect, feature negotiation, and explicit
subscriber gap behavior.

Acceptance: no RT/network path coupling; deterministic subscribe/reconnect;
backward-compatible negotiation; no application-specific methods; existing
clients and safety behavior remain unchanged.

Proof: schema/protobuf checks, service IPC tests, package-surface, strict
clippy, policy, and `git diff --check`.

Claim boundary: packaged software transport contract only; no Runbook support,
physical control-role validation, or output/FFB claim.

Rollback: disable the negotiated feature and preserve existing IPC methods.

The transport implementation merged in #193. Its proof covers the versioned
observe-only schema, bounded replay and live subscription, feature negotiation,
device/kind filtering, sequence-preserving replay, and typed lag handling.

## Work item: control-diagnostics-capture-replay

Status: completed
Linked issue: #172
Merged PR: #195
Prerequisite: `versioned-control-stream-transport` merged as PR #193
Target seams: diagnostics/capture/replay surfaces and a deterministic fixture
handoff for a separate input-only consumer proof.

Goal: provide bounded diagnostics, versioned capture, deterministic replay,
and a reusable fixture for an input-only consumer proof without making Runbook
an OpenRacing dependency.

Acceptance: deterministic descriptor/baseline/events/reset/disconnect replay;
visible epoch and sequence diagnostics; versioned capture fixtures; no output,
FFB, or profile mutation. Live `wheeld` subscription/capture and the
package-based consumer smoke remain explicit deployment proof in #180.

Proof: focused replay and CLI tests, package-surface validation, policy, strict
clippy, and `git diff --check`.

Claim boundary: diagnostic and replay evidence only; not physical compatibility,
named controls, release readiness, or hardware/output support.

Rollback: remove the capture consumer and retain the transport contract.

PR #195 delivered the hardware-free `wheelctl controls`
list/monitor/capture/replay core, versioned JSON capture format, and
deterministic fixture through the production `ControlProjector`. It did not
prove live capture/subscription from a running `wheeld`, a packaged-artifact
consumer, named controls, or physical support. The live and packaged proof is
owned by #180 rather than being implied by this completed item.

## Work item: control-stream-package-composition

Status: completed
Source-of-truth issue: #173
Parent implementation epic: #174
Linked issue: #179
Merged PR: #196 (`e625296e`)
Target seams: `RELEASING.md`, claimed platform inputs under `packaging/`,
release package validation, and the external contract assets rooted in
`crates/schemas/proto/` and the deterministic #172 replay fixture.

Goal: include matching stream-capable binaries, a versioned external-consumer
contract, compatibility metadata, checksums, and deterministic replay fixtures
in each claimed package without introducing a parallel Runbook installer.

Acceptance: every claimed package contains coherent daemon, contract, and
fixture assets; validation catches missing or mismatched assets; feature
metadata is truthful; external consumers need no engine/HID/FFB dependency;
existing APIs and safety behavior remain compatible; and no output, torque,
FFB, physical-control, or support-tier claim is broadened.

Proof: focused contract-bundle and package-validation tests, formatter, strict
Clippy for changed Rust seams, package-surface validation, policy, changelog
validation, exact package checks for each claimed platform, and
`git diff --check`.

Claim boundary: package composition and external contract publication only.
This item does not prove installed lifecycle, upgrade/rollback, real hardware,
named controls, Runbook product support, output, FFB, or platform support
beyond exact package receipts.

Rollback: remove the package contract assets and validation wiring while
retaining the existing runtime transport, diagnostics, schema versions, client
behavior, and safety defaults.

PR #196 merged as `e625296e`. Its exact-head checks and review resolution are
the proof for this package-composition item. The implementation publishes the
Linux contract bundle and validates its coherence; it does not prove installed
lifecycle, upgrade/rollback, or consumer smoke.

## Work item: control-stream-artifact-lifecycle-proof

Status: ready
Parent implementation epic: #174
Linked issue: #180
Prerequisite: `control-stream-package-composition` completed by PR #196
Target seams: `.github/workflows/release.yml`, installed-package test harnesses,
claimed platform packages, prior-release fixtures, and the external input-only
consumer smoke.

Goal: prove `control_stream_v1` from built package artifacts across restart,
upgrade/rollback, disabled or unavailable service behavior, and an input-only
consumer without making Runbook an OpenRacing dependency.

Acceptance: packaged `wheeld` negotiates the feature truthfully; descriptor,
baseline, events, reset/gap, disconnect, reconnect, and restart behavior remain
deterministic; upgrade/rollback preserve profiles, configuration, and safety
state; existing clients remain compatible; and the virtual/replay consumer
smoke uses packaged artifacts rather than source-tree binaries.

Proof: installed/package binary feature negotiation; descriptor-to-baseline-to-
events; reset/gap/disconnect and restart; prior-release upgrade/rollback;
missing/disabled feature; existing-client and safety behavior; input-only
consumer replay; package-surface/status validation; and `git diff --check`.

Claim boundary: artifact lifecycle and virtual/replay consumer evidence only.
This item does not prove real hardware compatibility, named control roles,
simulator behavior, output, FFB, high torque, profile mutation, Runbook product
support, or platform support beyond exact receipts.

Rollback: remove the lifecycle proof wiring and retain the package composition,
runtime transport, diagnostics, client behavior, and safety defaults.

Issue #173 defined the deployment/release contract in merged PR #176, and #174
remains the parent epic. Neither implementation item is folded into the
archived Moza goal.
