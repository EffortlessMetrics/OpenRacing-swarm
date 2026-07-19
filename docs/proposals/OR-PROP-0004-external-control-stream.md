# OR-PROP-0004: External control-stream lane

Status: active
Owner: platform/service
Created: 2026-07-17
Target milestone: n/a
Linked specs: docs/specs/OR-SPEC-0005-external-control-stream.md
Linked ADRs: docs/adr/0010-non-rt-control-stream-boundary.md
Linked plan: plans/external-control-stream/implementation-plan.md
Active goal: .openracing/goals/active.toml
Support/status impact: no public support claim
Policy impact: no policy change

## Problem

External applications need a read-only view of decoded control-surface input
without becoming a second hardware owner or entering the 1 kHz force-feedback
path. The repository has `DeviceInputs` and service/IPC boundaries, but no
source-of-truth contract for projecting those inputs into a stable stream.

## Users and surfaces

The lane serves non-real-time service consumers and diagnostic/proof tools. It
touches the vendor-neutral input domain, `ApplicationDeviceService`, and the
existing `WheelServiceImpl`/IPC boundary in later work. A Runbook consumer is a
proof consumer only; the OpenRacing contract is generic.

## Proposed shape

Define a hybrid stream containing a descriptor, an initial baseline, ordered
button/hat/rotary events, and explicit reset/disconnect items. Preserve raw,
candidate, and validated semantic identity as distinct provenance states.
Collect from the existing input owner on a non-RT worker and provide bounded,
observable backpressure behavior to later subscribers.

## Deployment and release extension

Issue #173 adds a separate package/release work item after the runtime chain.
The implementation must reconcile the existing release surfaces rather than
invent a Runbook-specific installer:

- `.github/workflows/release.yml` currently builds Linux and Windows release
  artifacts;
- `RELEASING.md` defines the tag, changelog, artifact, and upgrade workflow;
- `packaging/linux/` owns the Linux package scripts, service, rules, and
  distribution metadata;
- `packaging/windows/` owns the portable and MSI packaging scripts and WiX
  definition;
- `packaging/macos/` contains the current DMG, bundle, entitlements, and
  uninstall surfaces; and
- `crates/schemas/proto/` plus `crates/schemas/build.rs` own the versioned IPC
  schema and generated-client boundary.

The deployment item must establish that installed `wheeld` advertises
`control_stream_v1` only when its backing service is present, preserve existing
upgrade and rollback behavior, and publish schema/client artifacts from the
same versioned source. Platform support and release readiness remain bounded by
the proof actually produced for each existing packaging lane.

## Decisions to lock

- No application or network I/O occurs in the 1 kHz FFB path.
- A physical device has one input owner; the stream never opens a second HID
  handle merely for a consumer.
- Initial baselines never synthesize button presses or rotary actions.
- Initial scope is input-only: buttons, hat/D-pad, and rotary deltas.
- Steering, pedals, dashboards, LEDs, haptics, FFB, and output control are out
  of scope for the initial contract.
- Runtime work is split into five independently shippable plan items and does
  not activate this lane by changing `.openracing/goals/active.toml`.
- Deployment/package work is a separate plan item. It must not broaden support,
  readiness, output, FFB, high-torque, profile-mutation, or control-claiming
  semantics.

## Alternatives considered

- **Application-specific API:** rejected because it would make a generic device
  contract depend on one consumer.
- **Second HID reader:** rejected because it creates competing ownership and
  lifecycle/race risks.
- **Latest-state polling over a socket:** rejected because it can lose rotary
  ticks and has no explicit reset or gap semantics.
- **Adding transport in the domain crate:** rejected because domain types must
  remain reusable without protobuf, tonic, or network dependencies.

## Non-goals

- Implementing runtime collection, projection, gRPC, capture, replay, or
  release/package behavior in this source-of-truth scaffold.
- Editing the release workflow, packaging scripts, installed binaries, schema,
  or generated client artifacts.
- Opening hardware or assigning named physical controls without evidence.
- Changing torque safety, FFB behavior, profiles, or public support tiers.

## Claim boundary

This proposal creates an activation-ready source-of-truth lane. It does not
prove a released stream, installed-binary negotiation, package compatibility,
upgrade/rollback, physical-device compatibility, named paddle/rotary roles,
Runbook support, simulator behavior, or any hardware output capability.
