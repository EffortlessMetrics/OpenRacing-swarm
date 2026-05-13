# Moza R5 Live Testing Roadmap

This roadmap defines the live-testing boundary for Steven's Moza stack:

- Moza R5 wheelbase, PID `0x0004` or `0x0014`
- KS wheel
- ES wheel
- SR-P pedals
- HBP handbrake
- Windows over USB HID

Primary topology for the first lane is the common user setup: KS or ES rim,
pedals, and handbrake connected through the R5 base/hub and reported through the
R5 aggregated HID input stream. Direct USB SR-P or HBP captures are useful
secondary coverage only when those devices enumerate separately; their absence is
not a blocker for the primary through-base lane.

The verifier should validate logical control coverage over observed endpoints,
not a single fixed kit shape. A lane proves that a logical role was observed from
a concrete endpoint through a declared connection path:

```text
logical role -> observed endpoint -> connection path -> capture evidence -> semantic status
```

For the primary Moza R5 lane, the R5 HID endpoint is the wheelbase hub and
expected source for steering, rim controls, throttle, brake, clutch when
present, and handbrake. Standalone USB pedals, standalone USB handbrakes, button
boxes, shifters, or mixed-vendor devices can be added later as separate observed
endpoints that provide specific logical roles.

The declared `semantic_status` is explicit: `deferred` is a planned lane role,
`unavailable` is missing capture or endpoint evidence, `missing` is a parsed
capture without parser-visible role movement, `generic_aux` is visible generic
R5 V1 extended movement without a semantic field name, and `proven` is
parser-visible role-specific evidence.

If multiple output-capable endpoints are visible, passive enumeration may record
all of them. Any later output-capable test must require one explicit selected
endpoint; it must never choose a motor by list order.

The first live test is not "does OpenRacing work." The first live test is:

```text
Can OpenRacing see the Moza stack,
capture passive input,
and prove that it did not send output?
```

The repository is ready for passive live testing. It is not ready for live FFB
testing. A direct-drive wheelbase is a motor; every non-zero output path remains
hazardous until the prerequisite receipt gates prove otherwise.

## Current Boundary

Software validation rails exist for the Moza lane:

- `wheelctl hardware doctor`
- staged hardware safety state model
- virtual HID replay
- shared capture metadata and provenance boundaries
- generic passive HID parser
- output write barrier
- capability registry
- property tests
- telemetry replay fixtures
- virtual FFB output logs
- hardware lane authoring guide
- Moza receipt verifier and lane audit gates

These rails make OpenRacing better at collecting, checking, and refusing to
overclaim evidence. They are not real hardware evidence.

Real hardware validation has not started until a dated directory exists under:

```text
ci/hardware/moza-r5/<date>/
```

and that directory contains real receipts produced from the target hardware.

Virtual or synthetic evidence can exercise code paths. It must never advance a
real hardware lane, satisfy a dated `ci/hardware/**` gate, or justify a support
claim.

## First Session Scope

The first live session is no-output and passive-only.

Allowed:

- hardware doctor
- device enumeration
- Moza probe
- HID list
- descriptor capture
- idle input capture
- steering sweep capture
- through-R5 pedal sweep capture
- through-R5 handbrake sweep capture
- wheel button and control capture
- parser fixture validation
- passive bundle verification
- passive lane audit

Not allowed:

- FFB
- torque
- low torque
- high torque
- direct mode
- simulator-to-wheel output
- serial configuration
- firmware
- DFU
- Pit House firmware or update-page testing

If any command asks for output, torque, direct mode, or an operator override,
stop the session and review the runbook.

## Physical Setup

Before running live commands:

- Mount the wheelbase securely.
- Confirm the rim is mechanically seated.
- Stabilize pedals and handbrake.
- Route cables away from moving parts.
- Keep hands clear unless performing a passive input gesture.
- Keep loose tools away from the wheelbase and rim.
- Keep the power switch or power cable reachable.
- Keep the USB cable reachable.
- Close the simulator, or disable simulator FFB.
- Keep Pit House in a known state.
- Do not open firmware, update, or calibration flows.

Do not rely on software as the emergency stop. The real stop is power removal.
For passive testing, the wheel should not move by itself. If it does, remove
power and stop.

## Gate 0: Visibility

Gate 0 proves only that Windows and OpenRacing can see the Moza stack. It does
not create a hardware validation lane.

Run:

```powershell
cd H:\Code\Rust\OpenRacing

cargo run -p wheelctl -- hardware doctor `
  --json-out target\hardware-doctor.json

cargo run -p wheelctl -- moza probe --json

$hidList = Join-Path $env:TEMP "moza-hid-list-openracing.json"

cargo run -p racing-wheel-hid-capture --bin hid-capture -- `
  list `
  --vendor 0x346E `
  --json-out $hidList

Get-Content -LiteralPath $hidList
```

Expected safety properties:

```text
no_hid_device_opened=true
no_ffb_writes=true
no_output_reports=true
no_feature_reports=true
no_serial_config_commands=true
no_firmware_or_dfu_commands=true
```

Expected visibility result:

```text
devices is not empty
VID 0x346E appears
R5 and related Moza HID interfaces are visible
```

If `wheelctl moza probe` and `hid-capture list --vendor 0x346E` are both empty,
stop OpenRacing testing. That is a USB, power, driver, Pit House, or Windows
device-state issue, not a receipt-lane issue. Do not create fake lane artifacts.

## Gate 1: Passive Receipts

Start Gate 1 only after Gate 0 succeeds.

Use [moza-r5-bench-day.md](moza-r5-bench-day.md) for the copy-paste passive
runbook. The first receipt PR should be:

```text
hardware: add Moza R5 passive validation receipts
```

Required artifacts:

```text
ci/hardware/moza-r5/<date>/manifest.json
ci/hardware/moza-r5/<date>/device-list.json
ci/hardware/moza-r5/<date>/moza-probe.json
ci/hardware/moza-r5/<date>/hid-list.json
ci/hardware/moza-r5/<date>/descriptor.json
ci/hardware/moza-r5/<date>/captures/r5-idle.jsonl
ci/hardware/moza-r5/<date>/captures/r5-steering-sweep.jsonl
ci/hardware/moza-r5/<date>/captures/srp-wheelbase-aggregated-sweep.jsonl
ci/hardware/moza-r5/<date>/captures/r5-throttle-only-sweep.jsonl
ci/hardware/moza-r5/<date>/captures/r5-brake-only-sweep.jsonl
ci/hardware/moza-r5/<date>/captures/r5-clutch-only-sweep.jsonl
ci/hardware/moza-r5/<date>/captures/r5-handbrake-only-sweep.jsonl
ci/hardware/moza-r5/<date>/captures/r5-aggregated-idle-after-controls.jsonl
ci/hardware/moza-r5/<date>/captures/ks-controls.jsonl
ci/hardware/moza-r5/<date>/captures/es-controls.jsonl
ci/hardware/moza-r5/<date>/parser-fixture-validation.json
ci/hardware/moza-r5/<date>/fixture-promotion.json
ci/hardware/moza-r5/<date>/passive-verification.json
ci/hardware/moza-r5/<date>/manifest-promotion-passive.json
ci/hardware/moza-r5/<date>/lane-audit-passive.json
```

These artifacts are the current evidence set for the primary through-R5 topology.
The verifier work that consumes them should reason about logical controls rather
than treating every filename as a universal hardware requirement.

Optional direct-plug evidence may add `srp-standalone-sweep.jsonl` or
`hbp-standalone-sweep.jsonl` when those devices are connected and visible as
standalone USB HID devices. Do not create placeholder standalone artifacts for a
through-base topology.

Maximum claim:

```text
passive observation
descriptor capture
input capture
parser validation
```

Explicit non-claims:

```text
zero output
low torque
FFB
simulator validation
high torque
serial configuration
firmware/DFU
release readiness
```

## Safety Ladder After Passive

Do not skip steps.

```text
1. Passive receipts
2. Zero-output safety receipts
3. Watchdog, disconnect, and final-zero receipts
4. Bounded low-torque proof
5. Pit House coexistence
6. Simulator telemetry only
7. Bounded simulator-to-Moza FFB smoke
```

The first output test must be zero output only:

- watchdog active
- final zero required
- operator ready to remove power
- no simulator involved
- no non-zero torque

The first non-zero output test must be bounded low torque only:

- same-lane passive and zero receipts already present
- descriptor trusted or explicit operator override recorded
- hard output cap
- final-zero proof
- no high torque

Simulator FFB smoke is last. It requires passive, zero, low-torque, Pit House,
and simulator telemetry receipts first. The virtual FFB output log flow is a
planning and rehearsal tool only; it is not real simulator-to-device evidence.

## Stop Conditions

Stop immediately if:

- the wheel moves unexpectedly
- the device disappears or reappears repeatedly
- Pit House changes mode
- a firmware or update prompt appears
- probe shows an unknown or wrong VID/PID
- descriptor capture cannot be completed or trusted
- any command wants output before passive receipts exist
- any receipt claims more than the current stage
- watchdog or final-zero proof fails

For live wheelbase sessions, "stop" means remove power, not only Ctrl+C.

## Related Documents

- [Moza R5 Passive Bench Day](moza-r5-bench-day.md)
- [Moza R5 Hardware Validation Lane](moza-r5-validation.md)
- [Moza R5 Artifact Checklist](moza-r5-artifact-checklist.md)
- [Moza R5 Negative Verifier](moza-r5-negative-verifier.md)
- [Moza R5 Simulator Smoke](moza-r5-simulator-smoke.md)
- [Hardware Lane Authoring Guide](hardware-lane-authoring.md)
