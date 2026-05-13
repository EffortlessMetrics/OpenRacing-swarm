# Moza R5 Simulator Smoke Runbook

This runbook prepares the first simulator path for the Moza R5 lane. It is not
a release procedure and it does not prove broad simulator support.

Run simulator work in two stages:

1. telemetry-only proof
2. bounded simulator-to-Moza FFB smoke

Do not run the bounded FFB smoke until passive, zero-output, low-torque, Pit
House, watchdog, disconnect, and final-zero safety receipts exist in the same
dated lane.

## Preferred First Source

Use the SimHub bridge first if it is available. It is the lowest-friction first
telemetry source because it avoids game-specific install and anti-cheat
variables during the initial smoke path.

Use a direct game adapter only after the SimHub bridge flow is understood.

## Prerequisites

Before telemetry-only proof:

- `wheelctl telemetry record --help` works
- a normalized telemetry source file exists
- no hardware output is enabled
- no `wheeld --hardware-lane` writer is running for output

Before bounded FFB smoke:

- passive lane verification passed
- zero lane verification passed
- `init-off.json` and `init-standard.json` passed
- `zero-torque-proof.json` passed
- `watchdog-proof.json` passed
- `disconnect-proof.json` passed
- `low-torque-proof.json` passed
- Pit House coexistence receipts needed before smoke are present
- descriptor trust is established or an explicit operator override is recorded
- high torque is disabled
- wheel is mounted safely
- e-stop / stop path is available

## Telemetry-Only Proof

Record normalized snapshots from the chosen source. The recorder input is a
JSON or JSONL file containing normalized telemetry snapshots.

```powershell
$DATE = Get-Date -Format "yyyy-MM-dd"
$LANE = "ci/hardware/moza-r5/$DATE"

wheelctl telemetry record `
  --game simhub-bridge `
  --telemetry-source simhub_bridge `
  --input "<normalized-telemetry-source.jsonl>" `
  --out "$LANE/simulator-telemetry-recording.jsonl" `
  --duration-ms 30000
```

Then build the telemetry-only proof receipt:

```powershell
wheelctl moza simulator-telemetry-proof `
  --lane $LANE `
  --game simhub-bridge `
  --telemetry-source simhub_bridge `
  --recorder-artifact simulator-telemetry-recording.jsonl `
  --duration-ms 30000 `
  --json-out "$LANE/simulator-telemetry-proof.json"
```

The receipt must prove:

- `hardware_output_enabled=false`
- `no_ffb_writes=true`
- normalized snapshot records exist
- recorder provenance matches the game/source/session
- no serial config commands
- no firmware or DFU commands

Telemetry-only proof does not validate Moza output.

## Virtual FFB Log Dry Run

Before a real output writer is armed, the checked-in telemetry fixtures can
exercise the output-barrier and final-zero log shape with virtual evidence:

```powershell
wheelctl telemetry virtual-ffb-log `
  --input fixtures/telemetry/simhub/basic-lap.jsonl `
  --out target/virtual/simulator-ffb-output.virtual.jsonl `
  --session-id virtual-smoke-dry-run `
  --max-percent 2 `
  --watchdog-timeout-ms 100 `
  --json
```

This command opens no HID device, sends no real FFB writes, and refuses output
paths under `ci/hardware/**`. The resulting JSONL is useful for software tests
and operator rehearsal only; it is not a Moza receipt and cannot satisfy
`wheelctl moza simulator-ffb-smoke`.

## Bounded FFB Smoke

Run this only after the same dated lane contains the hardware prerequisite
receipts listed above.

Start the service writer for the same lane:

```powershell
wheeld --hardware-lane $LANE
```

Then run the bounded smoke receipt command:

```powershell
wheelctl moza simulator-ffb-smoke `
  --lane $LANE `
  --game simhub-bridge `
  --telemetry-source simhub_bridge `
  --output-log-artifact simulator-ffb-output.jsonl `
  --descriptor-trusted `
  --watchdog-timeout-ms 100 `
  --stop-cleared-output `
  --pause-cleared-output `
  --game-exit-cleared-output `
  --json-out "$LANE/simulator-ffb-smoke.json"
```

Use `--explicit-operator-override` only when descriptor trust cannot be
established and the operator is intentionally accepting that limited smoke-test
risk. Do not use it for high torque.

The output log must prove:

- bounded direct torque reports only
- `max_output_percent <= 5`
- high torque false
- watchdog active
- stop clears output
- pause clears output
- game exit clears output
- mode mismatch clears output if Pit House changes mode
- final zero attempted and sent
- the final record is zero output
- output records link back to telemetry sequences and `ffb_scalar`

## Final Smoke-Ready Verification

After telemetry proof, bounded FFB smoke, and Pit House mode-change evidence are
present:

```powershell
wheelctl moza verify-bundle `
  --lane $LANE `
  --stage smoke-ready `
  --json-out "$LANE/smoke-ready-verification.json"

wheelctl moza promote-manifest `
  --lane $LANE `
  --stage smoke-ready `
  --json-out "$LANE/manifest-promotion-smoke-ready.json"

wheelctl moza audit-lane `
  --lane $LANE `
  --stage smoke-ready `
  --json-out "$LANE/lane-audit-smoke-ready.json"
```

## Non-Claims

This lane still does not claim:

- release readiness
- high-torque validation
- serial configuration support
- firmware or DFU support
- broad Moza support
- broad simulator support
- anti-cheat compatibility

The claim ceiling is one receipt-backed, bounded simulator smoke path for the
specific dated Moza R5 lane.
