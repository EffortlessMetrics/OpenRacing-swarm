# Architecture Decision Records (ADRs)

This directory contains Architecture Decision Records for the Racing Wheel Software Suite. ADRs document important architectural decisions, their context, rationale, and consequences.

## Current ADRs

- [ADR-0001: Force Feedback Mode Matrix](0001-ffb-mode-matrix.md) - FFB delivery mechanisms and capability negotiation
- [ADR-0002: IPC Transport Layer](0002-ipc-transport.md) - Cross-platform communication between service and clients
- [ADR-0003: OWP-1 Protocol Specification](0003-owp1-protocol.md) - Open Wheel Protocol for device communication
- [ADR-0004: Real-Time Scheduling Architecture](0004-rt-scheduling-architecture.md) - 1kHz timing and RT guarantees
- [ADR-0005: Plugin Architecture](0005-plugin-architecture.md) - Safe and fast plugin system design
- [ADR-0006: Safety Interlocks and Fault Management](0006-safety-interlocks.md) - Comprehensive safety system design
- [ADR-0007: Multi-Vendor HID Protocol Architecture](0007-multi-vendor-hid-protocol-architecture.md) - SRP microcrates for vendor-specific HID protocols
- [ADR-0008: Game Auto-Configure and Telemetry Bridge](0008-game-auto-configure-telemetry-bridge.md) - Automatic game telemetry config and adapter lifecycle management
- [ADR-0009: Hardware Validation Evidence State Machine](0009-hardware-validation-evidence-state-machine.md) - Typed evidence ordering for hardware validation lanes

## Creating New ADRs

1. Copy `template.md` to `XXXX-title.md` where XXXX is the next sequential number
2. Fill in all sections, especially:
   - Context: What problem are we solving?
   - Decision: What are we doing?
   - Rationale: Why this approach?
   - Consequences: What are the trade-offs?
   - References: Link to specific requirements
3. Submit as PR for review
4. Update status to "Accepted" after approval

## ADR Lifecycle

- **Proposed**: Initial draft, under discussion
- **Accepted**: Approved and being implemented
- **Deprecated**: No longer recommended, but not forbidden
- **Superseded**: Replaced by a newer ADR

## Guidelines

- ADRs are immutable once accepted - create new ADRs to change decisions
- Reference specific requirement IDs from `requirements.md`
- Include implementation notes for complex decisions
- Add compliance and verification criteria
- Link related ADRs to show decision dependencies

## Tools

- Use `template.md` as starting point for new ADRs
- Validate ADR format with `cargo run -p openracing-tools --bin validate-adr -- --verbose`
- Generate ADR index with `cargo run -p openracing-tools --bin generate-docs-index --`

## Role in the source-of-truth stack

ADRs are the **durable decision** layer:

```text
Roadmap -> Proposal -> Spec -> ADR -> Plan -> Active goal -> PR -> Proof
```

An ADR owns architectural or operating decisions that should still matter months later, along with context, consequences, rejected alternatives, and follow-up specs or plans. ADRs must not become task lists, live status reports, or implementation queues.
