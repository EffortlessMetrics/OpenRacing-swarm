# ADR-0009: Hardware Validation Evidence State Machine

**Status:** Accepted  
**Date:** 2026-05-09  
**Authors:** Architecture Team, Hardware Team  
**Reviewers:** Safety Team, Platform Team  
**Related ADRs:** ADR-0006 (Safety Interlocks and Fault Management), ADR-0007 (Multi-Vendor HID Protocol Architecture), ADR-0008 (Game Auto-Configure and Telemetry Bridge)

## Context

OpenRacing is moving hardware validation from research notes into receipt-backed lanes. The Moza R5 lane is the first concrete example, but the same proof pattern applies to future wheelbases, pedals, handbrakes, and virtual hardware backends.

Without a shared evidence model, each lane can accidentally encode safety state as loose booleans in JSON or command-specific flags. That makes it too easy for future code to arm output paths from partial evidence, stale artifacts, or synthetic fixtures that look like real hardware receipts.

The validation sequence is ordered:

1. enumerate the device
2. trust the descriptor/signature
3. verify passive captures through parsers
4. prove zero output
5. arm and verify bounded low torque
6. arm simulator smoke from telemetry
7. verify simulator-to-output smoke and final zero

This sequence must be hardware-family-neutral. Moza, Simagic, Fanatec, Logitech, Heusinkveld, generic pedals, and virtual backends should all use the same safety rails while keeping vendor protocols in their own crates.

## Decision

Add `openracing-hardware-core` as the shared crate for hardware validation state and evidence primitives.

The crate owns:

- `HardwareValidationStage`
- `HardwareTransition`
- typed evidence wrappers for enumeration, descriptor trust, passive verification, zero output, low torque, simulator telemetry, simulator smoke, and final zero
- a typestate flow for ordered validation tokens
- a runtime `HardwareValidationMachine` mirror for receipt verifiers

The ordered stages are:

```text
Disconnected
Enumerated
DescriptorTrusted
PassiveVerified
ZeroOutputVerified
LowTorqueArmed
LowTorqueVerified
SimulatorSmokeArmed
SmokeReady
```

The typestate structs have private fields and are only advanced by methods that consume the previous state and typed evidence. For example, `LowTorqueArmed` can only be produced from `ZeroOutputVerified`, and `SmokeReady` can only be produced after low torque, simulator telemetry, simulator smoke, and final-zero evidence.

The runtime machine exposes the same transition table for code that must evaluate receipts loaded from disk. Invalid runtime transitions return explicit errors instead of silently promoting a lane.

This crate does not open HID devices, parse vendor reports, send FFB output, or define device-specific constants.

## Rationale

- **Evidence before output**: output-capable states require prior typed evidence instead of free-form booleans.
- **Reusable across hardware families**: the crate models validation stages, not Moza-specific behavior.
- **Verifier-friendly**: receipt validators can use the runtime machine while safety-sensitive code can use the typestate flow.
- **Claim discipline**: virtual and synthetic evidence are represented explicitly, so later gates can refuse to promote them as real hardware validation.
- **No RT coupling**: this is non-RT validation infrastructure and does not enter the 1 kHz path.

## Consequences

### Positive

- Future hardware lanes share one transition table and vocabulary.
- Invalid stage promotion is caught centrally.
- Typestate tokens make accidental low-torque or smoke-ready construction harder in ordinary Rust APIs.
- Real, virtual, and synthetic evidence can be carried through one model without conflating their claim levels.

### Negative

- Adding a new crate increases workspace metadata and Hakari surface.
- Existing Moza lane verifier code will need a follow-up integration PR before it benefits from the shared machine.
- The runtime machine still requires reviewers to check that a receipt's evidence source is appropriate for the claim being made.

### Neutral

- The crate is allocation-capable because it stores evidence lineage; it is not part of RT execution.
- Vendor-specific parser, descriptor, and output rules remain in their existing protocol crates.

## Alternatives Considered

1. **Keep the state machine inside `wheelctl`**: Rejected because validation state is not an operator CLI concern and would be hard to share with service, receipt CI, and virtual hardware tests.
2. **Put the state machine in `engine`**: Rejected because the evidence model is non-RT validation infrastructure and should not increase engine coupling.
3. **Use JSON schema only**: Rejected because schemas validate shape, not ordered capability transitions.
4. **Let each hardware lane define its own stages**: Rejected because it invites drift and inconsistent claim ceilings across vendors.

## Implementation Notes

- `openracing-hardware-core` has no device-family constants.
- Typestate structs do not implement `Deserialize`; external code cannot materialize output-capable tokens from receipt JSON.
- `ValidationLineage` and `HardwareValidationMachine` are serializable for diagnostics and verifier output.
- Evidence wrappers validate that artifact paths are non-empty and carry an `EvidenceSource` of `real_hardware`, `virtual`, or `synthetic`.
- Follow-up PRs should wire Moza verifier gates, virtual HID replay, and device capability registry code to this shared crate.

## Compliance & Verification

- Unit tests cover the full ordered valid path.
- Unit tests iterate every stage/transition pair and reject every invalid runtime transition.
- Unit tests verify final-zero evidence is required for low-torque verification and smoke-ready promotion.
- `cargo test -p openracing-hardware-core` must pass.
- `cargo clippy -p openracing-hardware-core --all-targets -- -D warnings` must pass.
- `cargo hakari verify` must pass after adding the crate.

## References

- Requirements: SAFE-01, SAFE-02, SAFE-05, DM-01, DM-03, NFR-02
- Related Documentation: `docs/hardware/moza-r5-validation.md`
- Related Documentation: `docs/hardware/moza-r5-negative-verifier.md`
- Related ADRs: ADR-0006, ADR-0007, ADR-0008
