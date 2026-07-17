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

## Proof expectations

Runtime PRs use their exact plan commands and include focused virtual/fake tests.
The docs-only scaffold is validated with:

```text
cargo run --locked -p openracing-tools --bin validate-adr -- --verbose
cargo run --locked -p openracing-tools --bin package-surface -- --check
git diff --check
```

## Non-goals and claim boundary

This spec does not implement or release a stream, prove physical-device
compatibility, assign named paddles/rotaries, integrate Runbook as a product
dependency, or claim simulator, output, FFB, or hardware readiness.
