# Moza Validation Matrix

This matrix separates source-backed research coverage from real hardware validation. "Source verified" means a VID/PID, report shape, or behavior was confirmed from public sources, code tests, or existing research. "Validated" means Steven's hardware produced receipts in `ci/hardware/moza-r5/<date>/` and the bundle verifier passed for the relevant stage.

Use [moza-r5-artifact-checklist.md](moza-r5-artifact-checklist.md) before changing any row. Every promotion must map to concrete receipts, verifier output, manifest promotion, and a lane audit.

## Current Rows

| Lane | Stack | Platform | Transport | Evidence stage | Steven hardware validated | Simulator validated | High torque validated | Release ready |
|------|-------|----------|-----------|----------------|---------------------------|---------------------|-----------------------|---------------|
| `moza-r5-windows-usb` | R5 + KS/ES + SR-P + HBP | Windows | HID only | Not started | No | No | No | No |

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
| FFB mode | Staged: off, standard, then gated direct/low torque |
| High torque | False by default; not part of first smoke |
| Pit House | Separate coexistence receipts required |

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
| Real hardware smoke ready | Zero proof ready plus `init-off.json`, `init-standard.json`, `moza-status.json`, `device-status.json`, `support-bundle.json`, `low-torque-proof.json`, `pit-house-coexistence.json`, `simulator-telemetry-proof.json`, `simulator-ffb-smoke.json`, `wheelctl moza verify-bundle --stage smoke-ready`, `smoke-ready-verification.json`, `manifest-promotion-smoke-ready.json`, `lane-audit-smoke-ready.json` |

## Non-Claims

Until the receipts exist, this matrix does not claim:

- Moza R5 compatibility on Steven's hardware.
- KS or ES rim control correctness.
- SR-P clutch behavior in standalone or wheelbase-aggregated paths.
- HBP layout coverage on Steven's unit.
- Pit House coexistence safety.
- Direct/high-torque readiness.
- Release readiness.
