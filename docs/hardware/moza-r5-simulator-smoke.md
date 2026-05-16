# Moza R5 Simulator Compatibility Smoke Runbook

This runbook is for the optional simulator-adapter compatibility path. It is
not a prerequisite for native OpenRacing movement control. That native claim
must be proven separately by R5 HID input, native steering feedback,
OpenRacing force generation, bounded PIDFF output, and cleanup receipts without
SimHub or Pit House.

This runbook prepares the first simulator path for the Moza R5 lane. It is not
a release procedure and it does not prove broad simulator support.

Run simulator work in two stages:

1. telemetry-only proof
2. bounded simulator-to-Moza FFB smoke

Do not run the bounded FFB smoke until passive, zero-output, watchdog,
disconnect, final-zero, staged init, service/status, low-torque, and simulator
telemetry receipts exist in the same dated lane. Pit House coexistence is a
separate smoke-ready promotion gate; it does not block the first
OpenRacing-controlled bounded FFB smoke when Pit House is not installed or not
running.

## Preferred First Source

Use the SimHub bridge first if it is available. It is the lowest-friction first
telemetry source because it avoids game-specific install and anti-cheat
variables during the initial smoke path.

Use a direct game adapter only after the SimHub bridge flow is understood.

## Prerequisites

Before telemetry-only proof:

- `wheelctl telemetry record --help` works
- either a live SimHub UDP JSON source is available or a normalized telemetry
  source file exists
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
- `simulator-telemetry-proof.json` passed
- `moza-status.json`, `device-status.json`, and `support-bundle.json` are
  refreshed and still diagnostic/no-output receipts
- the selected output strategy has descriptor trust. The live R5 V1 lane uses
  `--strategy pidff-bounded-effect`; direct report `0x20` remains
  verifier-distinct and blocked until descriptor metadata proves that report
- high torque is disabled
- Pit House is closed, not installed, or otherwise not in a firmware/update
  flow. Do not claim Pit House coexistence from this smoke.
- wheel is mounted safely
- e-stop / stop path is available

## Telemetry-Only Proof

Record normalized snapshots from the chosen source. For the first Windows bench
path, prefer live SimHub JSON UDP on port `5555`; this records telemetry only
and opens no HID or output path.

```powershell
$DATE = Get-Date -Format "yyyy-MM-dd"
$LANE = "ci/hardware/moza-r5/$DATE"

wheelctl telemetry record `
  --game simhub-bridge `
  --telemetry-source simhub_bridge `
  --live-simhub `
  --port 5555 `
  --out "$LANE/simulator-telemetry-recording.jsonl" `
  --duration-ms 30000
```

If the live recorder reports `0 packet(s)`, no UDP traffic reached
OpenRacing. Confirm the SimHub bridge/export is running and sending JSON UDP to
this host on port `5555` before retrying. If packets arrive but the recorder
reports parse errors, verify that the sender is emitting SimHub JSON fields such
as `SpeedMs`, `Rpms`, `Gear`, `Throttle`, `Brake`, `Steer`, and `FFBValue`.
The recorder intentionally writes no lane artifact until at least one valid
normalized snapshot is captured.

If live SimHub is not available, `wheelctl telemetry record` can also stamp an
existing JSON/JSONL file containing normalized telemetry snapshots by using
`--input <normalized-telemetry-source.jsonl>` instead of `--live-simhub`.

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
receipts listed above. This step proves one bounded OpenRacing-controlled
simulator output path. It is not a Pit House coexistence claim and it is not a
release-ready claim.

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
  --strategy pidff-bounded-effect `
  --descriptor-trusted `
  --watchdog-timeout-ms 100 `
  --stop-cleared-output `
  --pause-cleared-output `
  --game-exit-cleared-output `
  --json-out "$LANE/simulator-ffb-smoke.json"
```

Do not use `--explicit-operator-override` for the live R5 V1 PIDFF smoke path.
If a future lane deliberately validates the direct report `0x20` path, keep that
receipt verifier-distinct from PIDFF and document the descriptor evidence before
running it.

The output log must prove:

- the declared output strategy; live R5 V1 uses PIDFF bounded effects, not direct report `0x20`
- no direct report `0x20` records when `--strategy pidff-bounded-effect` is used
- PIDFF Set Effect / Constant Force / Effect Operation records followed by Stop All cleanup
- `max_output_percent <= 5`
- high torque false
- watchdog active
- stop clears output
- pause clears output
- game exit clears output
- mode mismatch clears output if Pit House changes mode
- final zero attempted and sent
- the final record is zero output / PIDFF Stop All
- output records link back to telemetry sequences and `ffb_scalar`
- writer provenance includes the exact lane manifest HID endpoint selector

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
