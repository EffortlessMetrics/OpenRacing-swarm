# Hardware Lane Authoring Guide

This guide describes how to add a receipt-backed hardware validation lane
without confusing research, simulation, and real hardware evidence. The Moza R5
lane is the current reference implementation, but the same pattern applies to
future wheelbases, pedals, handbrakes, button boxes, and generic HID devices.

Hardware lane scaffolding is not hardware validation. A lane may claim a
validated stage only after a dated real-hardware receipt bundle exists, the
stage verifier passes, the manifest promotion records the new stage, and the
lane audit passes.

## Evidence Classes

Every artifact must make its evidence source obvious.

| Evidence source | Allowed use | Must not do |
|-----------------|-------------|-------------|
| `real` | Validate a dated hardware lane when produced by the documented command against the target device | Hide missing captures, reuse off-lane receipts, or skip verifier/audit gates |
| `virtual` | Exercise parser, output-barrier, watchdog, disconnect, and verifier behavior without hardware | Satisfy real hardware validation gates or advance a capability registry record |
| `synthetic` | Unit tests, parser fixtures, and malformed-input coverage | Appear under `ci/hardware/**` as a real receipt |
| `research` | Seed VID/PID tables, report-shape expectations, and docs | Become a compatibility claim without receipts |

Virtual and synthetic evidence must carry explicit non-real flags such as:

```json
{
  "hardware_source": "virtual",
  "real_hardware_validated": false
}
```

Do not make a verifier infer this from a path or file name alone.

## Lane Anatomy

A hardware family should have these pieces before the first real bench session:

```text
ci/hardware/<family>/
  README.md
  manifest.schema.json

docs/hardware/<family>-validation.md
docs/hardware/<family>-validation-matrix.md
docs/hardware/<family>-artifact-checklist.md
```

Use a dated child directory only for real hardware evidence:

```text
ci/hardware/<family>/<yyyy-mm-dd>/
  manifest.json
  ...
```

Do not commit placeholder receipts, virtual receipts, synthetic captures, or
hand-edited proof files under a dated `ci/hardware/**` lane.

## Claim Ceilings

Each PR must state what it proves and what it deliberately does not prove.

| PR type | Maximum claim |
|---------|---------------|
| Scaffold/docs/schema | Command and verifier structure only |
| Virtual replay | Software behavior only, never hardware validation |
| Passive receipts | Enumeration, descriptors, and input parsing only |
| Zero receipts | Zero output, watchdog, disconnect, and final-zero safety only |
| Low-torque receipts | Bounded low torque only, high torque still false |
| Simulator telemetry | Real telemetry recording only, no hardware output |
| Simulator FFB smoke | One bounded simulator-to-device smoke path, not release readiness |
| Status promotion | Exact passed row only; keep non-claims explicit |

Never promote "supported" or "release ready" from scaffold, synthetic fixtures,
virtual replay, or one smoke row.

## Stage Model

Use `openracing-hardware-core` for shared validation state instead of inventing
per-lane booleans.

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

Output-capable code must receive a capability token from the safety model and
must pass through the shared output write barrier. Deserializing receipt JSON is
not allowed to mint output capability by itself.

## Required Lane Phases

### 1. Research And Static Capabilities

Record public or code-derived facts separately from validation.

Required outputs:

- VID/PID constants or registry entries
- capability ceiling records with `validated_stages: []`
- protocol/parser docs that say what is known from sources
- validation matrix rows marked not started

Rules:

- Unknown devices default to passive-only.
- High torque defaults to false.
- Serial configuration, firmware, and DFU are out of scope unless a separate
  lane explicitly validates them.

### 2. Scaffold And Negative Verification

Add command surfaces, schema, docs, and verifier gates before hardware is
connected.

Required behavior:

- no fake receipts
- negative verifier fails cleanly with missing artifacts
- no HID output path is opened during negative verification
- docs explain the stop conditions

The root lane directory is never evidence by itself.

### 3. Passive Enumeration And Descriptor Capture

The first real hardware stage must observe only.

Receipts normally include:

```text
manifest.json
device-list.json
hid-list.json
descriptor.json
probe.json or <family>-probe.json
captures/*.jsonl
parser-fixture-validation.json
fixture-promotion.json
passive-verification.json
manifest-promotion-passive.json
lane-audit-passive.json
```

Rules:

- no output reports
- no feature-report init unless the stage explicitly allows it
- no serial config
- no firmware or DFU command
- parser fixtures promoted from real captures must scrub local paths and serials
- descriptor CRC/source must be preserved for later trust decisions

### 4. Zero Output And Safety Proofs

Before any non-zero command, prove zero and failure handling.

Receipts normally include:

```text
init-off.json
init-standard.json
zero-torque-proof.json
watchdog-proof.json
disconnect-proof.json
zero-verification.json
manifest-promotion-zero.json
lane-audit-zero.json
```

Rules:

- zero output must not enable motor torque
- watchdog expiry must clear output
- disconnect must record safe failure and final-zero attempt where possible
- off/standard init must not send direct torque or high-torque commands

### 5. Bounded Low-Torque Output

Low torque is the first non-zero hardware output stage.

Required gates:

- same-lane passive receipts
- same-lane zero, watchdog, disconnect receipts
- same-lane off and standard init receipts
- descriptor trust or explicit operator override
- explicit low-torque confirmation
- hard max-output clamp
- final-zero proof

Abort to zero on HID write error, watchdog fault, mode mismatch, or operator
stop.

### 6. Vendor App Coexistence

If a vendor app can share device control, test it separately.

Evidence should include:

- app closed
- app open/idle
- direct/high-risk mode blocked or explicitly acknowledged
- app mode change during run fails safe
- firmware/update page refuses high-risk tests

Operator notes alone are not enough. Use process/window snapshots, screenshots,
videos, or command receipts as lane artifacts.

### 7. Simulator Telemetry And Bounded Smoke

Validate telemetry before hardware output.

Telemetry-only proof must show:

- real telemetry source
- normalized snapshot count
- frame freshness/order
- no hardware output
- no FFB writes

Bounded FFB smoke must show:

- real prerequisite hardware receipts
- output records linked to telemetry sequence and scalar
- max output within the lane cap
- watchdog active
- stop, pause, game exit, and mode mismatch clear output
- final record is zero
- high torque false

## Shared Metadata

Use the shared capture metadata vocabulary where possible:

```json
{
  "vendor_id": "0x346E",
  "product_id": "0x0014",
  "interface_number": 0,
  "usage_page": "0x0001",
  "usage": "0x0004",
  "report_descriptor_crc32": "0x00000000",
  "capture_kind": "idle",
  "hardware_source": "real",
  "real_hardware_validated": true
}
```

The exact IDs and usage values are lane-specific. The evidence-source and
validation fields are not optional.

## PR Sequence

Use small PRs with separate claim ceilings:

```text
docs/hardware: add <family> validation lane scaffold
cli: add observe-only <family> probe/capture commands
test: promote real <family> captures into parser fixtures
safety: add <family> zero-output proof receipts
safety: add gated <family> low-torque proof receipts
safety: add <family> vendor-app coexistence receipts
sim: add <family> simulator telemetry receipts
sim: add <family> bounded FFB smoke receipts
docs: mark <family> lane hardware smoke validated
```

Do not mix real receipts with broad refactors, dependency upgrades, or unrelated
CI topology work.

## CI Expectations

Normal PRs should rely on Linux correctness gates and receipt verification.
Hosted Windows or macOS jobs are compatibility signals unless the lane is
testing platform-specific behavior.

Hardware receipt CI validates artifacts already produced on the rig. It must not
pretend to emulate real hardware on a hosted runner.

## Reviewer Checklist

Before merging any hardware-lane PR, verify:

- every new claim maps to a concrete artifact
- the artifact path is dated and lane-local when claiming real hardware
- virtual and synthetic files cannot satisfy real gates
- no receipt was hand-edited to bypass a producer command
- no serial, firmware, DFU, high-torque, or direct-output behavior was added by
  accident
- output writes pass through the shared safety barrier
- verifier failures are treated as blockers
- the validation matrix keeps non-claims visible
- `release_ready` remains false unless a separate release readiness process has
  been completed

## What Never Belongs In A Scaffold PR

- dated `ci/hardware/**` receipts
- real hardware compatibility claims
- high-torque enablement
- firmware or DFU commands
- serial configuration writes
- non-zero output tests
- simulator FFB smoke claims
- broad "supported" language

Scaffolding should make the future bench session safer and shorter. It should
not make the project sound more validated than it is.
