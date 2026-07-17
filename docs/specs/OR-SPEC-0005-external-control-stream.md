# OR-SPEC-0005: External control stream

Status: proposed
Owner: platform/service
Created: 2026-07-17
Linked proposal: docs/proposals/OR-PROP-0004-external-control-stream.md
Linked ADRs: docs/adr/0010-non-rt-control-stream-boundary.md
Linked plan: plans/external-control-stream/implementation-plan.md
Linked issues: #167, #168, #169, #170, #171, #172, #173, #174
Linked PRs: n/a
Support-tier impact: no public support claim
Policy impact: no policy change

## Terms

`ControlSurfaceDescriptor` identifies an input-capable device and its
available controls. Every control has a stable raw identity; an optional
semantic identity carries explicit `raw`, `candidate`, or `validated` status.

An `initial_snapshot` establishes current state and comparison state. It is
not an actionable press, hat action, or rotary action. A `ControlEvent` is an
ordered change after the baseline. A `reset` ends an input epoch and requires a
new baseline. `disconnect` ends the stream for that device.

## Boundary and ownership

- The contract is vendor-neutral and contains no application-specific names.
- `DeviceInputs` remains the canonical decoded snapshot type in
  `crates/openracing-device-types/src/lib.rs` until an activated plan item
  changes it.
- `HidDevice::read_inputs()` is a non-real-time snapshot boundary.
- `ApplicationDeviceService` in `crates/service/src/device_service.rs` remains
  the device lifecycle/input owner.
- `WheelServiceImpl` in `crates/service/src/ipc_service.rs` is a later IPC
  adapter, not part of the domain contract.
- No stream collection, network I/O, locks, or allocation is added to the 1 kHz
  engine/FFB path.

## Stream contract

Each stream item carries a device identity, input epoch, monotonic sequence, and
monotonic timestamp. The item union is:

1. `descriptor` — available controls and their provenance metadata;
2. `initial_snapshot` — current button/hat/rotary state, with no synthesized
   actions;
3. `event` — a button edge, hat transition (including neutral), or rotary delta;
4. `reset` — explicit reason and new epoch requirement;
5. `disconnect` — terminal device lifecycle signal.

Subsequent snapshots do not emit unchanged state. Simultaneous changes use a
documented deterministic order. Rotary motion is lossless: multiple ticks
between consumer reads are queued, accumulated monotonically, or recoverable
by sequence; latest-state polling alone is insufficient.

## Backpressure and failure

Later service work uses bounded queues and never blocks the RT/FFB producer. A
lagged subscriber receives an explicit gap/reset with a fresh baseline or a
typed recoverable termination. Silent loss of button or rotary events is not a
valid behavior. Reset and disconnect clear comparison state and preserve epoch
boundaries.

## Initial control scope

Included: buttons in the canonical range, hat/D-pad transitions, and rotary
delta events. Excluded: steering, pedals, dashboards, LEDs, haptics, FFB,
output reports, profile mutation, and semantic claims about named physical
controls.

## Acceptance criteria

- Baseline-only first observation and explicit reset behavior are testable.
- Button boundary cases, hat neutral transitions, deterministic ordering, and
  duplicate snapshots are covered.
- Three rotary ticks before consumer drain remain observable as an effective
  `+3` (or equivalent ordered events).
- Raw/candidate/validated semantics remain distinguishable.
- Device ownership, reconnect, lag, gap/reset, and shutdown behavior are
  deterministic in later service work.
- No protobuf, gRPC, WebSocket, Runbook, network, or RT code is added before
  its plan item.

## Deployment and release contract

The deployment work item is separate from the five runtime work items and must
use the repository's existing release surfaces:

- `.github/workflows/release.yml` and `RELEASING.md` define release assembly,
  versioning, changelog, checksums, and release publication;
- `packaging/linux/` contains the Linux package and service inputs;
- `packaging/windows/` contains the portable and MSI installer inputs;
- `packaging/macos/` contains the current DMG and bundle inputs; and
- `crates/schemas/proto/`, `crates/schemas/buf.yaml`,
  `crates/schemas/buf.gen.yaml`, and `crates/schemas/build.rs` define the
  versioned schema and generated-client boundary.

The package/release implementation must prove all of the following before it
claims deployment support:

1. Installed `wheeld` includes and negotiates `control_stream_v1` only when the
   backing control-stream service exists; missing or disabled service support
   remains an explicit, safe capability result.
2. Descriptor, baseline, ordered events, reset/gap, and disconnect behavior are
   preserved through the installed binary and each claimed package lane.
3. Protobuf/schema and generated client artifacts are synchronized, versioned,
   and compatible with existing clients.
4. Upgrades preserve profiles and the documented rollback path restores the
   prior service/package behavior.
5. Platform and artifact claims are limited to the workflows and packaging
   inputs that produced evidence. Existing release automation is extended only
   where the proof requires it; a Runbook-specific installer is out of scope.

Observe-only control-stream behavior must not enable output, FFB, high torque,
profile mutation, or physical-control claiming. This work does not alter the
active goal or public support/readiness status.

## Proof expectations

Runtime PRs use their exact plan commands and include focused virtual/fake tests.
The docs-only scaffold is validated with:

```text
cargo run --locked -p openracing-tools --bin validate-adr -- --verbose
cargo run --locked -p openracing-tools --bin package-surface -- --check
git diff --check
```

The deployment implementation must additionally produce artifact-backed proof
for installed-binary feature negotiation, schema/package synchronization,
upgrade/rollback, disabled-feature behavior, existing-client/safety behavior,
and the input-only Runbook replay consumer.

## Non-goals and claim boundary

This spec does not implement or release a stream in the scaffold, edit release
automation or packaging in the scaffold, prove physical-device compatibility,
assign named paddles/rotaries, integrate Runbook as a product dependency, or
claim simulator, output, FFB, hardware readiness, or platform support beyond
the produced receipts.
