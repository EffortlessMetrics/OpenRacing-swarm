# ADR-0010: Non-real-time external control-stream boundary

**Status:** Proposed
**Date:** 2026-07-17
**Authors:** Platform Team, Service Team
**Reviewers:** Engineering Team
**Related ADRs:** ADR-0002 (IPC Transport), ADR-0007 (Multi-Vendor HID Protocol Architecture)

## Context

OpenRacing already decodes device input snapshots and owns device lifecycle in
non-real-time service code. External consumers need ordered input changes, but
adding a second HID owner, polling from the FFB loop, or collapsing rotary ticks
into latest state would weaken ownership, timing, or correctness boundaries.

The source-of-truth lane must distinguish domain projection from service
collection and transport, while preserving explicit evidence ceilings for raw,
candidate, and validated semantics.

The relevant repository requirements are NFR-01 (real-time determinism),
XPLAT-01 (platform I/O boundaries), and DM-01 (vendor-neutral device data).
The control-stream-specific contract IDs are defined by the linked proposal and
specification rather than being added to the legacy requirement catalog here.

## Decision

Define a vendor-neutral control-stream domain outside RT/FFB-specific code. The
domain models descriptor, baseline, ordered event, reset, and disconnect items;
device identity, input epoch, sequence, and monotonic timestamp; and raw,
candidate, or validated semantic provenance.

Use the existing decoded-input owner as the only physical-device owner. Collect
and broadcast on a bounded non-RT service path. Add transport only in a later
plan item, after the pure domain and service contracts have proof.

Initial controls are buttons, hat/D-pad, and rotary deltas. Steering, pedals,
dashboards, LEDs, haptics, FFB, and output are not part of this contract.

## Consequences

Positive:

- The 1 kHz FFB path remains free of network I/O, blocking, and stream policy.
- One device owner avoids duplicate handles and lifecycle races.
- Baselines, resets, gaps, and provenance are observable instead of inferred.
- Pure projection can be tested with virtual snapshots before transport exists.

Tradeoffs:

- Runtime work spans multiple PR-sized seams and requires explicit backpressure
  policy.
- Consumers must handle reset/gap outcomes rather than assuming lossless latest
  state.
- A validated semantic label remains an evidence-backed status, not a name that
  can be inferred from a control position.

## Constraints

- No application-specific API or Runbook dependency in the domain crate.
- No second HID open solely for stream consumers.
- No lane activation or replacement of `.openracing/goals/active.toml` in this
  scaffold.
- No public support claim until the relevant package, compatibility, and proof
  work exists.

## Rationale

Keeping projection and collection outside the real-time engine limits the
failure surface of the FFB loop. A single owner avoids duplicate HID handles,
while explicit baseline, epoch, sequence, and provenance fields prevent a
consumer from treating an inferred or stale control identity as validated.

## References

- Requirements: NFR-01, XPLAT-01, DM-01
- Proposal: `docs/proposals/OR-PROP-0004-external-control-stream.md`
- Specification: `docs/specs/OR-SPEC-0005-external-control-stream.md`
- Existing input domain: `crates/openracing-device-types/src/lib.rs`
- Existing service owner: `crates/service/src/device_service.rs`
- Existing IPC adapter: `crates/service/src/ipc_service.rs`

## Verification

The scaffold is checked with ADR validation, package-surface validation, and
`git diff --check`. Each later plan item adds focused virtual/service/transport
proof and states what it does not establish.
