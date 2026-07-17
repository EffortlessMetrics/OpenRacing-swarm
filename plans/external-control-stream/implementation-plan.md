# External control-stream implementation plan

Status: proposed
Owner: platform/service
Linked proposal: docs/proposals/OR-PROP-0004-external-control-stream.md
Linked specs:
- docs/specs/OR-SPEC-0005-external-control-stream.md
Linked ADRs:
- docs/adr/0010-non-rt-control-stream-boundary.md
Active goal: n/a; this scaffold does not replace the active Moza hardware goal

## Lane rules

Issue #167 is a docs-only activation scaffold. Runtime work remains blocked
until maintainers explicitly activate this lane with a new `.openracing/goals/active.toml`.
Each item below is one independently shippable runtime PR. Do not combine items
or broaden hardware/support claims.

## Work item: device-input-domain-contract

Status: ready after lane activation
Linked issue: #168
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

Status: blocked by `device-input-domain-contract` and lane activation
Linked issue: #169
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

Status: blocked by `deterministic-input-projection` and lane activation
Linked issue: #170
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

## Work item: versioned-control-stream-transport

Status: blocked by `non-rt-input-service` and lane activation
Linked issue: #171
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

## Work item: control-diagnostics-capture-replay

Status: blocked by `versioned-control-stream-transport` and lane activation
Linked issue: #172
Target seams: diagnostics/capture/replay surfaces and a separate input-only
Runbook proof consumer; exact paths to be finalized by the activated plan.

Goal: provide bounded diagnostics, capture, deterministic replay, and an
input-only consumer proof without making Runbook an OpenRacing dependency.

Acceptance: descriptor/baseline/events/reset/disconnect replay; gap and
sequence diagnostics; redacted receipts; packaged-artifact proof; no output,
FFB, or profile mutation.

Proof: focused replay/receipt tests, package validation, policy, strict clippy,
and `git diff --check` plus the activated plan's consumer proof.

Claim boundary: diagnostic and replay evidence only; not physical compatibility,
named controls, release readiness, or hardware/output support.

Rollback: remove the capture consumer and retain the transport contract.

## Work item: control-stream-deployment-release-contract

Status: blocked by `control-diagnostics-capture-replay` and lane activation
Linked issue: #173
Follow-up implementation issue: #174
Target seams: `.github/workflows/release.yml`, `RELEASING.md`,
`packaging/linux/`, `packaging/windows/`, `packaging/macos/`,
`crates/schemas/proto/`, `crates/schemas/buf.yaml`,
`crates/schemas/buf.gen.yaml`, and `crates/schemas/build.rs`.

Goal: extend the existing package/release lane so installed `wheeld` can
negotiate `control_stream_v1` only when the backing service exists, while
keeping schema/client artifacts versioned, existing clients and safety
behavior compatible, and upgrade/rollback behavior explicit.

Acceptance: the implementation uses the current release workflow and package
inputs; includes the feature only in installed `wheeld` artifacts; handles
missing or disabled service support explicitly; preserves descriptor, baseline,
events, reset/gap, and disconnect semantics; synchronizes schema and generated
client artifacts; proves prior-release upgrade and rollback; and documents
platform claims only for lanes with receipts. Observe-only behavior must not
enable output, FFB, high torque, profile mutation, or physical-control claims.
The active goal and support/readiness status remain unchanged.

Proof: installed/package binary feature negotiation; descriptor-to-baseline-to-
events; reset/gap/disconnect; schema and generated-client synchronization;
prior-release upgrade/rollback; missing/disabled feature; existing clients and
safety behavior; input-only Runbook replay consumer; package-surface/status
validation; and `git diff --check`.

Claim boundary: package and release evidence only. This item does not prove
hardware compatibility, named control roles, output, FFB, high torque, profile
mutation, Runbook product support, or platform support beyond its receipts.

Rollback: remove the package feature and retain the existing release assets,
schema versions, client behavior, and safety defaults.

Issues #173 and #174 are separate from the source-of-truth scaffold. #173
defines this deployment/release contract; #174 may implement the package and
runtime changes only after the runtime chain and this contract are ready. They
are not folded into the active Moza goal.
