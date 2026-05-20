# OR-SPEC-0002: Moza R5 vendor authority test lane

Status: proposed
Owner: hardware
Created: 2026-05-20
Linked proposal: docs/proposals/OR-PROP-0001-moza-native-visible-lane.md
Linked ADRs: docs/adr/0009-hardware-validation-evidence-state-machine.md
Linked plan: docs/hardware/moza-r5-vendor-authority-test-plan.md
Linked issues: n/a
Linked PRs: n/a
Support-tier impact: no support-tier promotion; this defines non-claiming evidence gates through the first bounded authority attempt receipt.
Policy impact: no new policy exception

## Scope

This spec defines a test-first, safety-gated lane for Moza R5 vendor-control authority research and execution planning. It targets native OpenRacing control and explicitly avoids blind PIDFF retries without new protocol evidence.

The lane must progress through the following state machine:

```text
protocol_research_only
  ↓
registry_recorded
  ↓
wire_codec_fixture_recorded
  ↓
fake_transport_verified
  ↓
read_only_status_verified
  ↓
vendor_authority_plan_reviewed
  ↓
vendor_authority_authorized_once
  ↓
vendor_authority_attempt_recorded
  ↓
post_authority_pidff_response_recorded
  ↓
native_visible_motion_recorded
  ↓
closed_loop_motion_ladder_recorded
  ↓
smooth_90_and_return_recorded
```

Each state artifact MUST include: `claim_scope`, `native_control_evidence`, `hardware_output_authorized`, `next_allowed_action`, `blocked_actions`, and `required_artifacts`.

## Required behavior

- OpenRacing MUST treat Pit House and SimHub as witness/compatibility lanes only, never as native-control prerequisites.
- This lane MUST separate semantic command definition from serial wire framing.
- Commands `[70,0]`, `[33,6]`, `[41,2]`, and `[31,19]` MUST remain unauthorized for output unless an exact authorization receipt is present and consumed.
- No artifact may claim `native_visible_ready` or smoke-ready until strict native-visible verification accepts real hardware movement evidence.
- No new public microcrate is allowed for this lane; implementation must live in internal Moza module surfaces.

### Architecture rail

Preferred implementation location:

```text
crates/openracing-hardware-core/src/hid/vendors/moza/serial/
```

Fallback (if microcrate collapse is incomplete):

```text
crates/hid-moza-protocol/src/serial/
```

In either case, this lane is an internal module family and not a public support contract.

### Command registry contract

A semantic registry MUST exist at either:

- `ci/hardware/moza-r5/2026-05-13/moza-vendor-command-registry.json`, or
- `fixtures/moza/r5/vendor-command-registry.json`.

Required command families:

- Authority/state: `[70,0]`, `[70,1]`, `[33,6]`, `[33,7]`
- Gain/safety: `[41,2]`, `[40,2]`, `[41,13]`, `[40,13]`, `[41,18]`, `[40,18]`
- Temperatures: `[43,4]`, `[43,5]`, `[43,6]`
- Compatibility mode: `[31,19]`, `[31,23]`

The PR1 fixture may be a partial starter registry that records only the
authority/state family. It MUST mark itself as partial and MUST list the missing
required families. PR2 must complete the registry before any codec, transport,
probe, authorization, or hardware-write work builds on it.

Required fenced/forbidden classes:

- group 10 EEPROM manipulation
- firmware/update/DFU commands
- HID report `0xaf`
- unknown host-to-device commands

### Risk policy contract

The risk model MUST include:

```rust
enum MozaRiskClass {
    SafeObserve,
    VendorStatus,
    StandardPidff,
    VendorControlCandidate,
    VendorOutputCandidate,
    ConfigurationCandidate,
    FirmwareOrDfuForbidden,
    UnknownDoNotSend,
}
```

`FirmwareOrDfuForbidden` and `UnknownDoNotSend` MUST be non-encodable/non-sendable.

### Codec contract

The serial codec maturity MUST be modeled as:

```rust
enum MozaSerialCodecStatus {
    SemanticOnly,
    FixtureDecodeOnly,
    RoundTripVerified,
    HardwareWriteEligible,
}
```

Initial required state is `SemanticOnly`. No hardware-write eligibility exists until fixture decode and round-trip verification gates pass.

### Read-only status probe contract

First hardware-capable step is read-only status probing. A read-only query may
still transmit a host-to-device request frame, so it MUST distinguish query
traffic from output/configuration writes. It MUST set:

```json
{
  "sent_read_only_query_commands": true,
  "sent_output_writes": false,
  "sent_configuration_writes": false,
  "sent_firmware_or_dfu_commands": false,
  "hardware_output_authorized": false,
  "native_control_evidence": false
}
```

### Authorization contract

Any vendor write MUST be exact-command authorized, hash-bound, command-bound, consumable once, and rejection-tested for payload drift, unknown commands, and generic bench-clear phrases.
The authorization receipt MAY set `hardware_output_authorized=true`, but it MUST
also keep `native_control_evidence=false` and `native_visible_ready=false` until
a later consumed hardware attempt records real evidence. The authorization tool
MUST NOT open HID, open serial, send read-only queries, or send
output/configuration/firmware writes while creating the receipt.

### Vendor authority smoke contract

The first authority smoke profile MUST be bounded and non-motion-claiming. A
software smoke dry-run MAY validate an exact authorization receipt, re-decode the
bound frame, and prove that the next hardware command is still blocked. The
dry-run MUST NOT open HID, open serial, send read-only queries, consume
authorization, or send output/configuration/firmware writes. It MUST keep
`native_control_evidence=false`, `hardware_output_authorized=false`,
`native_visible_ready=false`, `authorization_consumed=false`, `commands_sent=[]`,
and `planned_next_output.allowed=false`.

### First bounded hardware authority attempt contract

The first hardware authority attempt MUST consume exactly one matching
authorization receipt and exactly one matching smoke dry-run receipt. The
consumed attempt receipt MUST record the exact command id, risk class, tuple,
frame hash, payload hash, serial identity verification, and a single authorized
frame send.

The attempt receipt MUST close the authorization gate after the attempt by
recording `authorization_consumed=true` and `hardware_output_authorized=false`.
It MUST keep `native_control_evidence=false`, `native_visible_ready=false`,
`smoke_ready=false`, `sent_firmware_or_dfu_commands=false`,
`sent_unknown_commands=false`, `direct_hid_report_0xaf_sent=false`, and
`high_torque_enabled=false`. A retry MUST require a fresh bench-clear, a fresh
exact authorization receipt, a fresh smoke dry-run receipt, and a fresh attempt
receipt path; a consumed attempt receipt is evidence, not reusable
authorization.

The executable attempt command MUST require explicit
`--confirm-bounded-vendor-authority-attempt`, validate the exact authorization
and smoke dry-run receipts, verify the R5 USB serial identity before opening the
port, send only the exact hash-bound frame once, and write the consumed attempt
receipt. Verifiers, bench wizards, and generators MUST NOT emit or auto-run the
hardware attempt command.

### Post-authority PIDFF response and motion ladder

After authority smoke, the lane may compare baseline vs post-authority PIDFF response and then proceed through a closed-loop motion ladder:
`0.25°`, `0.5°`, `1°`, `3°`, `5°`, `15°`, `30°`, `90°` with return and strict abort/cleanup conditions.

## Test requirements

The lane MUST define unit, codec, fake transport, CLI, schema, verifier, and ignored hardware tests as staged in the linked plan. Hardware tests MUST default to ignored and require exact authorization.

## Non-goals

- No firmware/update/DFU control paths.
- No HID report `0xaf` sends.
- No direct USB control-transfer experiments.
- No WinUSB/Zadig driver replacement paths.
- No persistent config writes without restore plans.
- No high-torque enablement.
- No unknown host-to-device sends.
- No group 10 EEPROM manipulation.
- No “blind PIDFF retry only” progression.
