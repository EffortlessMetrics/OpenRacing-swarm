# OR-PROP-0004: External control-stream lane

Status: proposed
Owner: platform/service
Created: 2026-07-17
Target milestone: n/a
Linked specs: docs/specs/OR-SPEC-0005-external-control-stream.md
Linked ADRs: docs/adr/0010-non-rt-control-stream-boundary.md
Linked plan: plans/external-control-stream/implementation-plan.md
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

- Implementing runtime collection, projection, gRPC, capture, or replay.
- Opening hardware or assigning named physical controls without evidence.
- Changing torque safety, FFB behavior, profiles, or public support tiers.

## Claim boundary

This proposal creates an activation-ready source-of-truth lane. It does not
prove a released stream, physical-device compatibility, named paddle/rotary
roles, Runbook support, simulator behavior, or any hardware output capability.
