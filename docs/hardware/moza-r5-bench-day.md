# Moza R5 Passive Bench Day

This runbook is for the first real passive receipt session for Steven's Moza
stack: R5 wheelbase, KS wheel, ES wheel, SR-P pedals, and HBP handbrake.

It is observation-only. Do not run torque, direct mode, high torque, serial
configuration, firmware, or DFU commands from this runbook.

For the higher-level live-testing boundary, safety ladder, and stop conditions,
see [moza-r5-live-testing-roadmap.md](moza-r5-live-testing-roadmap.md).

## 1. Physical Setup

- Put the wheelbase on a stable rig or bench mount.
- Mount one rim at a time and confirm it is mechanically seated.
- Connect the R5 over USB directly or through a reliable powered hub.
- Connect SR-P pedals and HBP handbrake through the R5 base/hub for the primary
  lane. Direct USB SR-P or HBP captures are optional secondary coverage only when
  those devices are intentionally connected that way.
- Keep Pit House closed for the first enumeration pass.
- Keep hands clear of the wheel during enumeration and capture setup.
- Do not start a simulator.
- Do not enable high torque in Pit House or OpenRacing.

## 2. Pull Main

```powershell
cd H:\Code\Rust\OpenRacing
git checkout main
git pull --ff-only
```

Before creating the receipt lane, run the observe-only hardware doctor:

```powershell
wheelctl hardware doctor `
  --json-out target/hardware-doctor.json
```

This checks local HID enumeration, descriptor fallback expectations, known
VID/PID visibility, and Pit House process state where the platform supports it.
The receipt is diagnostic only; it is not hardware validation evidence and does
not belong under the dated `ci/hardware/moza-r5/<date>/` lane.

## 3. Create Dated Branch And Lane

```powershell
$DATE = Get-Date -Format "yyyy-MM-dd"
$LANE = "ci/hardware/moza-r5/$DATE"

git checkout -b "hardware/moza-r5-passive-$DATE"

wheelctl moza init-lane `
  --lane $LANE `
  --wheelbase-pid 0x0014 `
  --operator Steven
```

Use `0x0004` instead of `0x0014` if the R5 enumerates as the V1 PID.

## 4. Enumerate Devices

Run these before any capture work:

```powershell
wheelctl device list `
  --json-out "$LANE/device-list.json"

wheelctl moza probe `
  --json-out "$LANE/moza-probe.json"

hid-capture list `
  --vendor 0x346E `
  --json-out "$LANE/hid-list.json"

wheelctl moza descriptor `
  --vendor 0x346E `
  --json-out "$LANE/descriptor.json"
```

Stop if `moza-probe.json` or `hid-list.json` shows no Moza devices. Fix USB,
power, Windows device visibility, driver state, or Pit House interaction before
continuing. Do not create captures from an empty or non-Moza lane.

The passive lane must observe:

- R5 wheelbase PID `0x0014` or `0x0004`
- standalone SR-P PID `0x0003`, only if direct-plug topology is declared
- standalone HBP PID `0x0022`, only if direct-plug topology is declared
- Moza VID `0x346E`

Standalone SR-P PID `0x0003` and standalone HBP PID `0x0022` are expected only
for optional direct-plug captures. They are not required when pedals and
handbrake are attached through the R5 base/hub.

Record the physical device graph as observed, not as an assumed kit checklist.
For the primary lane, the R5 HID endpoint provides the wheelbase hub path for
steering, rim controls, pedals, and handbrake. Optional standalone HID endpoints
provide additional direct-plug coverage only when they are actually visible.

For passive work, multiple visible endpoints may be recorded. For any later
output-capable work, more than one visible motor-capable endpoint must require an
explicit operator-selected endpoint before output is allowed.

## 5. Descriptor Hex Fallback

If Windows cannot expose the raw R5 HID report descriptor, collect the report
descriptor bytes with USBTreeView or an equivalent USB descriptor tool.

Use the selected R5 device only for the supplied hex:

```powershell
wheelctl moza descriptor `
  --device <r5-selector> `
  --vendor 0x346E `
  --report-descriptor-hex "<hex bytes from USBTreeView>" `
  --json-out "$LANE/descriptor.json"
```

Keep the vendor-wide Moza records in `descriptor.json`. The supplied descriptor
hex should apply only to the selected R5 record.

## 6. Passive Captures

Each capture must be produced by `wheelctl moza capture-input`. Do not use FFB,
feature-report init, serial config, firmware, DFU, or simulator commands.

### R5 Idle

Gesture: touch nothing.

```powershell
wheelctl moza capture-input `
  --device <r5> `
  --duration-ms 5000 `
  --json-out "$LANE/captures/r5-idle.jsonl"
```

### R5 Steering Sweep

Gesture: slowly rotate full left, center, full right, center.

```powershell
wheelctl moza capture-input `
  --device <r5> `
  --duration-ms 10000 `
  --json-out "$LANE/captures/r5-steering-sweep.jsonl"
```

### Throttle Through R5 Hub

Gesture: 5 seconds idle, throttle 0->100->0 slowly, 5 seconds idle. Do not move
the wheel, brake, clutch, or handbrake.

```powershell
wheelctl moza capture-input `
  --device <r5> `
  --duration-ms 15000 `
  --json-out "$LANE/captures/r5-throttle-only-sweep.jsonl"
```

### Brake Through R5 Hub

Gesture: 5 seconds idle, brake 0->100->0 slowly, 5 seconds idle. Do not move the
wheel, throttle, clutch, or handbrake.

```powershell
wheelctl moza capture-input `
  --device <r5> `
  --duration-ms 15000 `
  --json-out "$LANE/captures/r5-brake-only-sweep.jsonl"
```

### Clutch Through R5 Hub

Gesture: 5 seconds idle, clutch 0->100->0 slowly, 5 seconds idle. Do not move
the wheel, throttle, brake, or handbrake.

```powershell
wheelctl moza capture-input `
  --device <r5> `
  --duration-ms 15000 `
  --json-out "$LANE/captures/r5-clutch-only-sweep.jsonl"
```

### HBP Through R5 Hub

Gesture: 5 seconds idle, handbrake 0->100->0 slowly, 5 seconds idle. Do not move
the wheel, throttle, brake, or clutch.

```powershell
wheelctl moza capture-input `
  --device <r5> `
  --duration-ms 15000 `
  --json-out "$LANE/captures/r5-handbrake-only-sweep.jsonl"
```

### Aggregated Idle After Controls

Gesture: touch nothing after the isolated control captures.

```powershell
wheelctl moza capture-input `
  --device <r5> `
  --duration-ms 5000 `
  --json-out "$LANE/captures/r5-aggregated-idle-after-controls.jsonl"
```

### Optional SR-P Standalone USB

Capture this only when SR-P is intentionally connected as a standalone USB
endpoint and the manifest topology declares `standalone_usb` evidence.

```powershell
wheelctl moza capture-input `
  --device <srp> `
  --duration-ms 10000 `
  --json-out "$LANE/captures/srp-standalone-sweep.jsonl"
```

Skip this artifact when SR-P is attached only through the R5 base/hub.

### Optional HBP Standalone USB

Capture this only when HBP is intentionally connected as a standalone USB
endpoint and the manifest topology declares `standalone_usb` evidence.

```powershell
wheelctl moza capture-input `
  --device <hbp> `
  --duration-ms 10000 `
  --json-out "$LANE/captures/hbp-standalone-sweep.jsonl"
```

Skip this artifact when HBP is attached only through the R5 base/hub.

### KS Wheel Controls

Gesture: mount KS, press representative buttons, move clutch paddles, and rotate
encoders. Keep steering movement minimal unless needed to keep reports flowing.

```powershell
wheelctl moza capture-input `
  --device <r5-with-ks> `
  --duration-ms 10000 `
  --json-out "$LANE/captures/ks-controls.jsonl"
```

### ES Wheel Controls

Gesture: mount ES, press representative buttons, move the hat/funky input, and
exercise any available directional controls.

```powershell
wheelctl moza capture-input `
  --device <r5-with-es> `
  --duration-ms 10000 `
  --json-out "$LANE/captures/es-controls.jsonl"
```

## 7. Validate Passive Evidence

```powershell
wheelctl moza validate-captures `
  --lane $LANE `
  --json-out "$LANE/parser-fixture-validation.json"

wheelctl moza promote-fixtures `
  --lane $LANE `
  --fixture-dir "crates/hid-moza-protocol/fixtures/moza-r5-$DATE" `
  --json-out "$LANE/fixture-promotion.json"

wheelctl moza verify-bundle `
  --lane $LANE `
  --stage passive `
  --json-out "$LANE/passive-verification.json"

wheelctl moza promote-manifest `
  --lane $LANE `
  --stage passive `
  --json-out "$LANE/manifest-promotion-passive.json"

wheelctl moza audit-lane `
  --lane $LANE `
  --stage passive `
  --json-out "$LANE/lane-audit-passive.json"
```

If any command fails, inspect the receipt's `missing_requirements`,
`next_commands`, or validation summary. Recapture the missing gesture or device
instead of editing receipts by hand.

## 8. Passive PR

Commit only passive evidence and promoted passive fixtures:

```powershell
git status --short
git add "$LANE" crates/hid-moza-protocol/fixtures/moza-r5-$DATE
git commit -m "hardware: add Moza R5 passive validation receipts"
git push -u origin "hardware/moza-r5-passive-$DATE"
gh pr create --title "hardware: add Moza R5 passive validation receipts"
```

The PR claim ceiling is passive observation only.

## Do Not Run Yet

Do not run these during the passive bench day:

- `wheelctl moza init`
- `wheelctl moza zero`
- `wheelctl moza watchdog-proof`
- `wheelctl moza disconnect-proof`
- `wheelctl moza torque-test`
- `wheelctl moza simulator-ffb-smoke`
- any direct-mode command
- any high-torque command
- any serial configuration command
- any firmware or DFU command

Those belong to later receipt PRs after passive evidence has merged.
