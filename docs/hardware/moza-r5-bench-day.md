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
  --hid-observe-only `
  --json-out "$LANE/device-list.json"

wheelctl moza probe `
  --json-out "$LANE/moza-probe.json"

hid-capture list `
  --vendor 0x346E `
  --json-out "$LANE/hid-list.json"

wheelctl moza descriptor `
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

## 5. Descriptor Fallback

If Windows cannot expose the raw R5 HID report descriptor, collect the report
descriptor bytes with USBTreeView, USBPcap/Wireshark enumeration traffic, Linux
sysfs, or an equivalent USB descriptor tool.
The fallback needs the actual HID Report Descriptor byte block. A USBTreeView
summary that only shows `wDescriptorLength`, or a descriptor read failure such
as `ERROR_INVALID_PARAMETER`, is not enough to satisfy descriptor trust. A
Windows `HidP KDR` collection/preparsed descriptor is also not the raw report
descriptor and must not be imported as lane evidence.

### Windows USBPcap enumeration fallback

Use this only if installing a Windows USB capture driver is acceptable for the
bench machine. It is passive USB observation, but it is still a system change.

Allowed:

- install/run USBPcap or Wireshark USB capture support
- capture USB enumeration while unplugging and replugging the R5
- extract the HID Report Descriptor bytes from the descriptor response

Not allowed:

- install Zadig
- replace the MOZA HID driver
- switch the R5 to WinUSB
- open Pit House firmware or update flows
- send HID output reports
- send HID feature reports
- touch serial configuration
- run firmware or DFU tools

Procedure:

```text
1. Close simulators and vendor update/configuration flows.
2. Start USBPcap/Wireshark capture on the USB controller that contains the R5.
3. Unplug and replug the R5 while capture is running.
4. Stop capture after enumeration completes.
5. Locate the HID Report Descriptor response for VID 0x346E, PID 0x0004 or
   0x0014, interface 2, usage page 0x0001, usage 0x0004.
6. Export only the HID Report Descriptor byte block as hex text or raw bytes.
```

The exported bytes must be the HID report descriptor payload, not the USB
device/configuration/interface descriptor, not a `wDescriptorLength` summary,
and not a Windows preparsed-data/KDR blob. If the capture cannot identify that
payload unambiguously, do not import it.

If the enumeration capture is saved as a `.pcapng`, extract the HID Report
Descriptor response with the checked-in helper. The helper is read-only: it only
asks `tshark` to read the capture file and write a compact hex text file.

```powershell
powershell -ExecutionPolicy Bypass -File scripts/extract_usbpcap_report_descriptor.ps1 `
  -InputPcapng "target/moza-r5-usbpcap-enumeration.pcapng" `
  -Output "target/moza-r5-report-descriptor.txt" `
  -InterfaceNumber 2
```

The helper fails closed if there is no HID Report Descriptor response or if more
than one response matches the selected interface. In that case, narrow the
capture or inspect the Wireshark packet list before importing anything into the
lane.

Use the selected R5 device only for the supplied hex:

```powershell
wheelctl moza descriptor `
  --device <r5-selector> `
  --report-descriptor-hex "<hex bytes from USBTreeView>" `
  --json-out "$LANE/descriptor.json"
```

If the descriptor bytes are easier to save as a text file, use the file form:

```powershell
wheelctl moza descriptor `
  --device <r5-selector> `
  --report-descriptor-hex-file "target/moza-r5-report-descriptor.txt" `
  --json-out "$LANE/descriptor.json"
```

If the descriptor bytes come from Linux sysfs as a raw binary
`report_descriptor` file, use native Linux or WSL2 with explicit USB
passthrough. Ordinary WSL2 does not expose Windows host HID devices under
`/sys/class/hidraw`. Use the binary file form:

```bash
mkdir -p target
descriptor=$(
  for node in /sys/class/hidraw/hidraw*; do
    if grep -qi 'HID_ID=.*:0000346E:00000004' "$node/device/uevent"; then
      printf '%s\n' "$node/device/report_descriptor"
      break
    fi
  done
)
test -n "$descriptor"
sudo cat "$descriptor" > target/moza-r5-report-descriptor.bin
wc -c target/moza-r5-report-descriptor.bin
```

Then import that binary file:

```powershell
wheelctl moza descriptor `
  --device <r5-selector> `
  --report-descriptor-bin-file "target/moza-r5-report-descriptor.bin" `
  --json-out "$LANE/descriptor.json"
```

Keep the vendor-wide Moza records in `descriptor.json`. The supplied descriptor
bytes should apply only to the selected R5 record.

When passive verification and audit are later green, record the read-only
pre-output ledger before starting any zero-torque work:

```powershell
wheelctl moza pre-output-readiness `
  --lane "$LANE" `
  --json-out "$LANE/pre-output-readiness.json"
```

If `ready_for_zero_torque` is false, stop. This receipt is a blocker summary,
not permission to run FFB.

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

If this capture still looks idle-like after one clean redo, stop recapturing
and inspect the physical/vendor state first. If Pit House is installed, open it
only in a normal non-update state and confirm whether the gas axis moves there.
If Pit House is not installed or is unavailable, do not install firmware tools
or enter update flows for this passive lane. Instead, power down the R5, reseat
the throttle pedal cable and the pedal-set-to-R5 cable, confirm the throttle is
on the expected SR-P throttle port or harness path, power the R5 back on, and
run one target-only gas check before replacing the lane capture:

```powershell
New-Item -ItemType Directory -Force -Path "target/moza-gas-check" | Out-Null

wheelctl moza capture-input `
  --device <r5> `
  --duration-ms 60000 `
  --json-out "target/moza-gas-check/r5-gas-after-reseat-60s.jsonl" `
  --json

wheelctl moza analyze-capture `
  --capture "target/moza-gas-check/r5-gas-after-reseat-60s.jsonl" `
  --json-out "target/moza-gas-check/r5-gas-after-reseat-analysis.json" `
  --json
```

Use the same gesture as the lane capture: 5 seconds idle, throttle
0->100->0 slowly several times, then 5 seconds idle. Do not move the wheel,
brake, clutch, handbrake, or rim controls. Replace
`$LANE/captures/r5-throttle-only-sweep.jsonl` only if the target-only analysis
shows parser-visible hub-control movement beyond the idle/trailer bytes. To
inspect the stored lane capture without assigning semantics to unlabeled bytes,
run:

```powershell
wheelctl moza analyze-capture `
  --capture "$LANE/captures/r5-throttle-only-sweep.jsonl" `
  --json-out "target/moza-passive-checks/r5-throttle-byte-delta.json" `
  --json
```

`analyze-capture` reads JSONL artifacts only. It reports raw byte and
little-endian word ranges without opening HID devices, sending output reports,
or claiming that a changing byte is throttle, clutch, handbrake, or a rim
control.

After several isolated captures, compare the whole lane against idle before
recapturing blindly:

```powershell
wheelctl moza analyze-lane `
  --lane "$LANE" `
  --json-out "target/moza-passive-checks/lane-analysis.json" `
  --json
```

`analyze-lane` reads stored JSONL artifacts only. It reports which required
captures decoded cleanly, which ones are missing, and which captures still lack
parser-visible control evidence compared with the lane idle capture.

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

Gesture: mount ES, press representative buttons one at a time, and exercise any
available non-output controls. ES does not have a hat/funky control, so do not
recapture solely to satisfy a hat/funky expectation.

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
