# Moza Validation Matrix

This matrix separates source-backed research coverage from real hardware validation. "Source verified" means a VID/PID, report shape, or behavior was confirmed from public sources, code tests, or existing research. "Validated" means Steven's hardware produced receipts in `ci/hardware/moza-r5/<date>/` and the bundle verifier passed for the relevant stage.

Use [moza-r5-artifact-checklist.md](moza-r5-artifact-checklist.md) before changing any row. Every promotion must map to concrete receipts, verifier output, manifest promotion, and a lane audit.

## Current Rows

| Lane | Stack | Platform | Transport | Evidence stage | Steven hardware validated | Simulator validated | High torque validated | Release ready |
|------|-------|----------|-----------|----------------|---------------------------|---------------------|-----------------------|---------------|
| `moza-r5-windows-usb` | R5 + KS/ES + SR-P + HBP | Windows | HID only | Native response ready; 5 percent PIDFF response recorded twice; visible motion and external compatibility remain unclaimed | Bounded low torque plus actuator-response receipts | No | No | No |

## Lane Detail: `moza-r5-windows-usb`

| Field | Value |
|-------|-------|
| Wheelbase | Moza R5 |
| Wheelbase PIDs | `0x0004`, `0x0014` |
| Rims | KS, ES via wheelbase reports |
| Pedals | SR-P standalone USB and wheelbase aggregation |
| Handbrake | HBP standalone USB |
| OS | Windows |
| Transport | HID input/FFB only |
| Serial config | Out of scope |
| FFB mode | Staged: off, standard, PIDFF low torque, native response, native visible motion, then external simulator FFB |
| High torque | False by default; not part of first smoke |
| Pit House | Not installed on current bench; separate coexistence receipts required before smoke-ready claims |

## Current Validated Claims

The checked-in `ci/hardware/moza-r5/2026-05-13/` lane currently supports only these hardware-backed claims:

- Steven's R5 V1 PID `0x0004`, interface 2, descriptor, and wheelbase-hub topology are observed.
- Steering, throttle, KS controls, and ES controls have parser-visible passive evidence.
- Brake, clutch, and handbrake have parser-visible generic auxiliary evidence through the R5 hub.
- Parser fixture validation, fixture promotion, passive verification, passive manifest promotion, and passive audit pass.
- PIDFF Device Control Stop All Effects `0x0C` is descriptor-trusted as a zero-output strategy; the direct Moza report `0x20` remains unavailable from descriptor metadata.
- Zero-torque, watchdog, and bounded disconnect receipts exist; zero-stage verification, zero manifest promotion, and zero audit pass.
- Staged `init-off.json` and `init-standard.json` receipts pass for the lane endpoint.
- Bounded PIDFF low torque is proven at 1 percent for 150 ms with final Stop All cleanup.
- The native 1 percent actuator-profile smoke proves the OpenRacing PIDFF output rail and Stop All cleanup path.
- The preserved 5 percent / 2000 ms response-only PIDFF receipt and the 2026-05-17 bounded-shaped PIDFF micro-profile receipt each measured about 0.181 degrees of steering delta with successful writes and Stop All cleanup, so they are actuator-response evidence above the response floor.
- Those receipts remain below the 1 degree visible-motion threshold, so no visible-motion or smoke-ready success is claimed. The 2026-05-18 real 1 degree controlled-angle attempt is also preserved as failed visible-motion evidence: it sent five bounded PIDFF writes, sent final Stop All, stayed post-stop stable, and timed out before target after about 0.181 degrees of movement. The reviewed `bounded-pidff-micro-step-v2` retry is preserved separately in `native-controlled-angle-retry-smoke.json`; it sent 33 bounded PIDFF writes, sent final Stop All, stayed post-stop stable, and remained in the same 0.181 degree response band. The [controlled-angle analysis](moza-r5-controlled-angle-analysis.md) classifies these as safe undertravel, and [PIDFF semantics diagnosis](moza-r5-pidff-semantics-diagnosis.md) records that repeated micro-step writes changed write count but not motion. The [PIDFF lifecycle trace](moza-r5-pidff-lifecycle-trace.md) is recorded in `native-pidff-lifecycle-trace.json`; it decodes the preserved command logs as repeated setup/start/Stop-All cycles. The [PIDFF effect lifecycle plan](moza-r5-pidff-effect-lifecycle-plan.md) is recorded in `native-pidff-effect-lifecycle-plan.json`; it names `bounded-pidff-effect-lifecycle-v1` for software implementation and preflight, but that profile is not implemented and authorizes no output. The consumed follow-up plan does not authorize 5 percent / 3000 ms, 5 percent / 30000 ms, 30 degree, or 90 degree right/left output; any third controlled-angle attempt requires the planned profile to be implemented, dry-run/preflighted, separately authorized, and backed by fresh command-bound bench-clear.
- No direct mode, direct report `0x20`, simulator output, SimHub validation, Pit House coexistence, high torque, or release readiness is claimed.

## Research Coverage Already Present

| Area | Current evidence | Validation meaning |
|------|------------------|--------------------|
| VID/PID identity | Moza VID `0x346E`, R5 PIDs `0x0004` and `0x0014`, SR-P `0x0003`, HBP `0x0022` | Source verified only |
| Topology | Wheelbase aggregation and standalone peripheral categories | Source verified only |
| Direct torque encoder | Report `0x20`, zero payload, motor-enable flag rules, torque clamp tests | Code verified only |
| Protocol handler | Handshake state, FFB mode selection, high-torque gates, retry/failure states | Code verified only |
| Parser surfaces | Wheelbase, SR-P, and HBP report parsing | Synthetic/test fixture verified only |
| Pit House risk | HID/serial sharing and mode flip-flop risk documented | Research only |

## Promotion Rules

| Promotion | Required receipts |
|-----------|-------------------|
| Passive capture ready | `manifest.json`, `device-list.json`, `hid-list.json`, `moza-probe.json`, `descriptor.json`, all passive input captures, parser validation, fixture promotion, `wheelctl moza verify-bundle --stage passive`, `passive-verification.json`, `manifest-promotion-passive.json`, `lane-audit-passive.json` |
| Zero proof ready | Passive capture ready plus `zero-torque-proof.json`, `watchdog-proof.json`, `disconnect-proof.json`, `wheelctl moza verify-bundle --stage zero`, `zero-verification.json`, `manifest-promotion-zero.json`, `lane-audit-zero.json` |
| OpenRacing control ready | Zero proof ready plus `init-off.json`, `init-standard.json`, `moza-status.json`, `device-status.json`, `support-bundle.json`, `low-torque-proof.json`, `steering-angle-stream-proof.json`, `native-actuator-profile-smoke.json`, `wheelctl moza verify-bundle --stage openracing-control-ready`, `openracing-control-verification.json`, `manifest-promotion-openracing-control.json`, `lane-audit-openracing-control.json`; this stage does not require SimHub or Pit House |
| Native actuator response ready | OpenRacing control ready plus `native-actuator-visible-smoke.json` carrying measured response above the response floor, `wheelctl moza verify-bundle --stage native-response-ready`, `native-response-verification.json`, `manifest-promotion-native-response.json`, `lane-audit-native-response.json`; this stage does not require SimHub, Pit House, simulator telemetry, or simulator FFB |
| Native visible motion ready | Native actuator response ready plus a visible-motion receipt that passes the stricter operator-visible movement gate, `wheelctl moza verify-bundle --stage native-visible-ready`, `native-visible-verification.json`, `manifest-promotion-native-visible.json`, `lane-audit-native-visible.json`; this stage does not require SimHub or Pit House |
| Real hardware smoke ready | Native visible motion ready plus `pit-house-coexistence.json`, `simulator-telemetry-proof.json`, `simulator-ffb-smoke.json`, `wheelctl moza verify-bundle --stage smoke-ready`, `smoke-ready-verification.json`, `manifest-promotion-smoke-ready.json`, `lane-audit-smoke-ready.json` |

## Non-Claims

Until the later receipts exist, this matrix does not claim:

- Direct mode or direct report `0x20` readiness.
- Simulator-scale FFB output safety.
- Pit House coexistence safety.
- Simulator telemetry validation.
- Simulator-to-Moza FFB smoke coverage.
- Standalone SR-P or standalone HBP USB coverage for Steven's lane.
- Direct/high-torque readiness.
- Release readiness.
