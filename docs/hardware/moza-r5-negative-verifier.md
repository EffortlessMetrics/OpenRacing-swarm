# Moza R5 Negative Verifier Check

The root lane scaffold under `ci/hardware/moza-r5/` is not hardware evidence.
It contains lane documentation and schema files only. A dated lane directory is
required before any Moza R5 validation claim can pass.

## Command

Run from the repository root:

```powershell
cargo run -p wheelctl -- moza verify-bundle `
  --lane ci/hardware/moza-r5 `
  --stage smoke-ready `
  --json
```

Expected result without a dated receipt bundle:

- non-zero process exit
- `success: false`
- missing `manifest.json`
- missing enumeration receipts
- missing descriptor receipt
- missing passive captures
- missing parser validation and fixture-promotion receipts
- missing init, zero, watchdog, disconnect, low-torque, Pit House, simulator
  telemetry, and simulator FFB receipts
- `no_hid_device_opened: true`
- `no_ffb_writes: true`
- `no_serial_config_commands: true`
- `no_firmware_or_dfu_commands: true`

This is the correct no-hardware behavior. Do not weaken it.

## Verified Shape

On 2026-05-08, the command failed with:

```json
{
  "success": false,
  "lane": "ci/hardware/moza-r5",
  "requested_stage": "smoke_ready",
  "missing_artifacts": 26,
  "invalid_artifacts": 0,
  "failed_gates": 19,
  "no_hid_device_opened": true,
  "no_ffb_writes": true,
  "no_serial_config_commands": true,
  "no_firmware_or_dfu_commands": true
}
```

Representative missing artifacts included:

- `manifest.json`
- `device-list.json`
- `moza-probe.json`
- `hid-list.json`
- `descriptor.json`
- `captures/r5-idle.jsonl`
- `captures/r5-steering-sweep.jsonl`
- `captures/srp-wheelbase-aggregated-sweep.jsonl`
- `captures/srp-standalone-sweep.jsonl`
- `captures/hbp-standalone-sweep.jsonl`
- `captures/ks-controls.jsonl`
- `captures/es-controls.jsonl`
- `parser-fixture-validation.json`
- `fixture-promotion.json`
- `init-off.json`
- `init-standard.json`
- `zero-torque-proof.json`
- `watchdog-proof.json`
- `disconnect-proof.json`
- `low-torque-proof.json`
- `pit-house-coexistence.json`
- `simulator-telemetry-proof.json`
- `simulator-ffb-smoke.json`

## Interpretation

A passing smoke-ready verifier requires real receipts in a dated lane such as
`ci/hardware/moza-r5/2026-05-08/`. The scaffold itself cannot satisfy passive,
zero-output, low-torque, Pit House, simulator telemetry, or simulator FFB gates.

Use this negative check before hardware work if there is any doubt about whether
the repository currently contains real Moza evidence. A failure like the one
above is expected until Steven's Moza stack is connected and the dated receipt
bundle is collected.
