# Moza R5 Hardware Validation Lane

This document defines the first OpenRacing real-hardware validation lane for Steven's Moza stack:

- Moza R5 wheelbase, PID `0x0004` or `0x0014`
- Moza KS wheel
- Moza ES wheel
- Moza SR-P pedals through the R5 hub, with standalone USB as optional direct-plug evidence
- Moza HBP handbrake through the R5 hub, with standalone USB as optional direct-plug evidence
- Windows over USB HID

OpenRacing is still in pre-validation until this lane has real receipts. The existing Moza VID/PID table, parsers, protocol handler, direct torque encoder, and safety gates are research-backed implementation scaffolding. They are not a claim that Steven's hardware has passed.

Moza R5 is the first fully exercised consumer of the generic
[staged device bring-up rail](staged-device-bringup-rail.md). The Moza adapter
declares R5 VID/PIDs, wheelbase-hub topology, logical controls, descriptor
expectations, parser fixtures, and output eligibility while the rail keeps
stage ordering common across future device families.

## Lane Identity

```yaml
lane: moza-r5-windows-usb
hardware:
  wheelbase: Moza R5
  wheelbase_pid: "0x0014" # or "0x0004", matching the observed R5 unit
  rims:
    - KS
    - ES
  pedals:
    - SR-P
  handbrake: HBP
platform:
  os: Windows
  transport:
    hid: true
    serial_config: false
claims:
  ffb: staged
  high_torque: false
  pit_house_coexistence: tested_separately
```

Only the HID interface is in scope for this lane. Serial/CDC ACM configuration, firmware update, DFU, and Pit House tuning are out of scope.

## Evidence Layout

Each hardware run writes receipts under a dated directory:

```text
ci/hardware/moza-r5/<date>/
  manifest.json
  device-list.json
  hid-list.json
  descriptor.json
  moza-probe.json
  captures/
    r5-idle.jsonl
    r5-steering-sweep.jsonl
    r5-throttle-only-sweep.jsonl
    r5-brake-only-sweep.jsonl
    r5-clutch-only-sweep.jsonl
    r5-handbrake-only-sweep.jsonl
    r5-aggregated-idle-after-controls.jsonl
    ks-controls.jsonl
    es-controls.jsonl
  parser-fixture-validation.json
  fixture-promotion.json
  passive-verification.json
  manifest-promotion-passive.json
  lane-audit-passive.json
  pre-output-readiness.json
  init-off.json
  init-standard.json
  moza-status.json
  device-status.json
  support-bundle.json
  zero-torque-proof.json
  watchdog-proof.json
  disconnect-proof.json
  zero-verification.json
  manifest-promotion-zero.json
  lane-audit-zero.json
  low-torque-proof.json
  steering-angle-stream-proof.json
  native-actuator-profile-smoke.json
  native-actuator-visible-smoke.json
  openracing-control-verification.json
  manifest-promotion-openracing-control.json
  lane-audit-openracing-control.json
  pit-house-coexistence.json
  simulator-telemetry-proof.json
  simulator-ffb-smoke.json
  smoke-ready-verification.json
  manifest-promotion-smoke-ready.json
  lane-audit-smoke-ready.json
```

The manifest must validate against [manifest.schema.json](../../ci/hardware/moza-r5/manifest.schema.json). The bundle verifier is intentionally stricter than file presence; it checks for no overclaiming, Moza R5 VID/PID observation in every enumeration receipt, complete R5 descriptor metadata, no passive FFB writes, no serial-config/firmware/DFU commands, fixture promotion safety, real zero-torque logs, bounded low-torque logs, final zero, and simulator smoke safety fields.

Use [moza-r5-artifact-checklist.md](moza-r5-artifact-checklist.md) as the prompt-to-artifact audit before any status update. It maps each claim to its producer command, required receipt, verifier gate, and current non-claim state.

Create the dated lane directory and pre-validation manifest before any hardware action:

```powershell
wheelctl moza init-lane --lane ci/hardware/moza-r5/<date> --wheelbase-pid 0x0014 --operator Steven
```

## Phase 1: Passive Enumeration And Descriptor Capture

Passive commands must not open an output path or send feature/FFB reports.

```powershell
wheelctl device list --hid-observe-only --json-out ci/hardware/moza-r5/<date>/device-list.json
wheelctl moza probe --json-out ci/hardware/moza-r5/<date>/moza-probe.json
hid-capture list --vendor 0x346E --json-out ci/hardware/moza-r5/<date>/hid-list.json
wheelctl moza descriptor --json-out ci/hardware/moza-r5/<date>/descriptor.json
```

Optional status preflight:

```powershell
wheeld --hardware-lane moza-r5
wheeld --hardware-lane ci/hardware/moza-r5/<date>
wheelctl moza status --device <r5> --lane ci/hardware/moza-r5/<date> --json-out ci/hardware/moza-r5/<date>/moza-status.json
wheelctl device status <r5> --moza-lane ci/hardware/moza-r5/<date> --json-out ci/hardware/moza-r5/<date>/device-status.json --json
wheelctl support-bundle --device <r5> --moza-lane ci/hardware/moza-r5/<date> --output ci/hardware/moza-r5/<date>/support-bundle.json
```

`wheeld --hardware-lane moza-r5` labels service-side Moza readiness as part of `DeviceStatus`; if `--hardware-lane` points at a lane directory or `descriptor.json`, the service also reports descriptor CRC/source/trust from the receipt. When a lane directory contains stored `passive-verification.json`, `zero-verification.json`, `openracing-control-verification.json`, or `smoke-ready-verification.json`, the service reports the highest stored receipt stage in `safety_state`/`safety_reason` as diagnostic context only; when `zero-verification.json`, `init-off.json`, and `init-standard.json` all pass, the state may say the low-torque gate receipts are observed while torque readiness remains disabled. It does not initialize Moza protocol or send reports. `wheelctl moza status` summarizes Moza HID identity, whether the selected device is output-capable, and the lane verifier state if `--lane` is supplied. `wheelctl device status --moza-lane --json-out` writes the service-facing `device-status.json` receipt with the same descriptor and stored-stage overlay when the status has a Moza VID/PID. `wheelctl support-bundle --device <r5> --moza-lane` writes `support-bundle.json` with device status snapshots and a Moza artifact index. These status paths leave `ffb_ready=false` and `safe_to_send_torque=false` until explicit init, zero, and torque receipts exist.

Before any output-adjacent command, write a read-only pre-output ledger:

```powershell
wheelctl moza pre-output-readiness --lane ci/hardware/moza-r5/<date> --json-out ci/hardware/moza-r5/<date>/pre-output-readiness.json
```

This command opens no HID device and sends no reports. It reports
`ready_for_zero_torque`, `ready_for_native_control`,
`ready_for_external_compatibility`, and legacy `ready_for_ffb` separately.
`ready_for_zero_torque` must remain false until passive verification, passive
audit, fixture promotion, descriptor trust, at least one implemented trusted
zero-output strategy, and status/support no-output receipts all pass. It also
inventories zero-output strategy candidates from the trusted descriptor without
executing them. The live R5 V1 descriptor exposes standard PIDFF Device Control
report `0x0C`; `wheelctl moza zero --strategy pidff-stop-all` may use that report
as a zero-output Stop All Effects proof when the same lane descriptor metadata is
trusted. Direct report `0x20` remains required for the direct low-torque
strategy. PIDFF bounded low torque is a separate strategy and must not be
inferred from Stop All alone; it needs its own bounded-effect writer and receipt
proof before real hardware writes. The 1 percent native actuator-profile smoke
proves the OpenRacing-owned PIDFF output rail and cleanup path; smoke-ready
also requires a separate 5 percent native actuator visible-motion receipt that
proves actual steering delta from R5 input. `ready_for_native_control` is the
OpenRacing-owned movement path and must not depend on SimHub, Pit House, or
direct report `0x20`; the 2026-05-13 lane has reached this native-control state
without claiming visible-motion success. `ready_for_external_compatibility`
tracks optional simulator bridge and vendor-app coexistence receipts.
`ready_for_ffb` remains the simulator-smoke preflight and stays false until a
passing visible-motion receipt and simulator telemetry are present.

If Windows cannot expose the raw HID report descriptor, paste descriptor hex
from USBTreeView, USBPcap/Wireshark enumeration traffic, or an equivalent
descriptor tool, save the descriptor bytes as a text file, or import a raw
binary Linux sysfs `report_descriptor` dump:

```powershell
wheelctl moza descriptor --device <r5> --report-descriptor-hex "<hex bytes>" --json-out ci/hardware/moza-r5/<date>/descriptor.json
wheelctl moza descriptor --device <r5> --report-descriptor-hex-file target/moza-r5-report-descriptor.txt --json-out ci/hardware/moza-r5/<date>/descriptor.json
wheelctl moza descriptor --device <r5> --report-descriptor-bin-file target/moza-r5-report-descriptor.bin --json-out ci/hardware/moza-r5/<date>/descriptor.json
```

The descriptor fallback needs the actual HID Report Descriptor byte block. A
USBTreeView device/interface summary that only shows `wDescriptorLength`, a
descriptor read failure such as `ERROR_INVALID_PARAMETER`, or a Windows `HidP
KDR` collection/preparsed descriptor is useful failure evidence but does not
satisfy descriptor trust. Use a report-descriptor hex block such as `0000: 05
01 09 04 ...`, Linux `/sys/bus/hid/devices/.../report_descriptor` bytes, or
another descriptor tool that exposes the raw HID report descriptor.

On Windows, USBPcap/Wireshark is acceptable only as an enumeration-capture
source for the raw HID report descriptor bytes. Capture the unplug/replug
enumeration traffic for the R5, extract the HID Report Descriptor response for
the exact R5 interface, and import only that byte block with
`wheelctl moza descriptor`. Do not install Zadig, replace the MOZA HID driver,
switch the device to WinUSB, open firmware/update flows, send HID output
reports, send HID feature reports, touch serial configuration, or run
firmware/DFU tools. If the capture yields only USB device/interface descriptor
fields, a `wDescriptorLength` value, or a Windows preparsed-data/KDR blob, keep
the descriptor gate red.

On Linux, a connected R5 V1 descriptor can be exported without sending reports.
Use native Linux or a WSL2 instance with explicit USB passthrough; ordinary WSL2
does not expose Windows host HID devices under `/sys/class/hidraw`:

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

Use the vendor-wide `wheelctl moza descriptor` command for the lane receipt so `descriptor.json` contains the observed Moza records. When Windows cannot expose the raw R5 report descriptor, rerun it with `--device <r5>` and `--report-descriptor-hex`, `--report-descriptor-hex-file`, or `--report-descriptor-bin-file`; the receipt preserves the vendor-wide Moza records and applies the supplied descriptor bytes only to the one selected R5 record. `hid-capture descriptor --vendor 0x346E` is still an accepted lower-level producer for the same receipt shape, but the lane runbook uses the wheelctl command so all Moza receipts share one command surface.

Required R5 descriptor fields:

```json
{
  "vendor_id": "0x346E",
  "product_id": "0x0004|0x0014",
  "product_name": "...",
  "serial_number_present": true,
  "manufacturer": "...",
  "interface_number": 0,
  "usage_page": "...",
  "descriptor_source": "linux_sysfs|operator_supplied_hex",
  "report_metadata_source": "protocol_expected|report_descriptor_parsed",
  "report_descriptor_crc32": "...",
  "input_report_lengths": [],
  "output_report_ids": [],
  "output_reports": [{"report_id": "0x20", "report_len": 8}],
  "feature_report_ids": []
}
```

The passive verifier requires the R5 VID/PID to appear in `device-list.json`, `moza-probe.json`, `hid-list.json`, and `descriptor.json`, then validates the manifest-declared topology endpoints and logical-control evidence. `hardware-doctor.json` is required as an observe-only platform safety receipt; on Windows it may include redacted PnP topology such as HID and serial-class interfaces, but it must not open HID handles, send output or feature reports, touch serial config, or run firmware/DFU commands. The default Moza path is `wheelbase_hub`: steering, rim controls, pedals, and handbrake are proven through the R5 aggregated HID endpoint. Standalone SR-P (`0x0003`) and HBP (`0x0022`) records are optional direct-plug evidence only when topology declares `connection: "standalone_usb"`. The manifest `hardware` section records declared inventory; only the R5 wheelbase identity is mandatory, and required role evidence is selected by `topology.logical_controls` rather than by a fixed KS/ES/SR-P/HBP kit checklist. Each declared logical control also carries `semantic_status`: new lanes use `deferred`, parser-backed role-specific fields use `proven`, generic R5 V1 extended movement uses `generic_aux`, parsed captures without control proof use `missing`, and absent evidence uses `unavailable`. The descriptor record must include a descriptor source (`linux_sysfs` or `operator_supplied_hex`), descriptor CRC, serial-presence flag, manufacturer, interface number, usage page, descriptor-derived R5 input lengths, and observed descriptor-derived output/feature report metadata for the selected PID. Descriptor commands parse supplied or sysfs descriptor bytes into report lengths and IDs; they set `report_metadata_source: "report_descriptor_parsed"` only when that metadata came from descriptor bytes. Protocol-expected report metadata is passive evidence only. Direct zero-output and direct-mode descriptor trust require descriptor-derived report metadata in lane `descriptor.json` plus stored `report_descriptor_hex` whose CRC, parsed report IDs, and parsed `0x20` output report length match the receipt; otherwise the direct report `0x20` path is blocked. PIDFF Stop All zero output may still satisfy zero-stage readiness when descriptor metadata proves that PIDFF report, but it does not prove a nonzero PIDFF effect encoder. Later low-torque and simulator FFB smoke must use either a descriptor-proven direct `0x20` receipt path, a separately verified PIDFF bounded-effect receipt path, or a deliberate explicit operator override recorded in the output receipt. Passive receipts must come from the expected observation commands, have `success: true`, and declare `no_ffb_writes: true`, `no_serial_config_commands: true`, and `no_firmware_or_dfu_commands: true`; pure observation receipts (`moza-probe.json`, `hid-list.json`, `hardware-doctor.json`, `descriptor.json`, `parser-fixture-validation.json`, and `fixture-promotion.json`) must also declare `no_hid_device_opened: true`.

The passive capture verifier is stricter than parse success. Every capture JSONL report line must include `command: "wheelctl moza capture-input"` plus per-line no-output assertions (`no_ffb_writes`, `no_output_reports`, `no_feature_reports`, `no_serial_config_commands`, and `no_firmware_or_dfu_commands`), device path/interface/usage metadata, VID/PID, report ID, report length, and raw report bytes. The manifest's `hardware.wheelbase_pid` is a hard consistency gate: all R5 enumeration records, wheelbase-hub captures, promoted wheelbase parser fixtures, later output receipts, service receipts, and simulator writer receipts must match that exact R5 PID (`0x0004` or `0x0014`). Default passive evidence uses isolated through-R5 captures for throttle, brake, clutch, and handbrake. Standalone captures are pinned only when declared in topology: `srp-standalone-sweep.jsonl` must contain SR-P PID `0x0003`, and `hbp-standalone-sweep.jsonl` must contain HBP PID `0x0022`. `ks-controls.jsonl` must be full-length wheelbase reports with observed KS button/direction movement. `es-controls.jsonl` must be full-length wheelbase reports with observed ES button movement; ES does not have a hat/funky control, so passive verification does not require hat/funky variation for that rim.

Done when:

- The R5 appears as VID `0x346E`, PID `0x0004` or `0x0014`.
- The R5 endpoint appears in every enumeration/descriptor receipt, and any standalone endpoints appear only when declared in topology.
- KS and ES identity is inferred from wheelbase input reports, not USB PID.
- Direct-plug SR-P and HBP endpoints are observed only when that topology is declared.
- Descriptor CRC is stored for later allowlist work.

## Phase 2: Passive Input Capture

No FFB output is allowed in this phase.

```powershell
wheelctl moza capture-input --device <r5> --duration-ms 5000 --json-out ci/hardware/moza-r5/<date>/captures/r5-idle.jsonl
wheelctl moza capture-input --device <r5> --duration-ms 10000 --json-out ci/hardware/moza-r5/<date>/captures/r5-steering-sweep.jsonl
wheelctl moza capture-input --device <r5> --duration-ms 10000 --json-out ci/hardware/moza-r5/<date>/captures/r5-throttle-only-sweep.jsonl
wheelctl moza capture-input --device <r5> --duration-ms 10000 --json-out ci/hardware/moza-r5/<date>/captures/r5-brake-only-sweep.jsonl
wheelctl moza capture-input --device <r5> --duration-ms 10000 --json-out ci/hardware/moza-r5/<date>/captures/r5-clutch-only-sweep.jsonl
wheelctl moza capture-input --device <r5> --duration-ms 10000 --json-out ci/hardware/moza-r5/<date>/captures/r5-handbrake-only-sweep.jsonl
wheelctl moza capture-input --device <r5> --duration-ms 5000 --json-out ci/hardware/moza-r5/<date>/captures/r5-aggregated-idle-after-controls.jsonl
wheelctl moza capture-input --device <r5> --duration-ms 10000 --json-out ci/hardware/moza-r5/<date>/captures/ks-controls.jsonl
wheelctl moza capture-input --device <r5> --duration-ms 10000 --json-out ci/hardware/moza-r5/<date>/captures/es-controls.jsonl
```

When an isolated capture parses but does not satisfy the declared role
variation, inspect the stored artifact before recapturing blindly:

```powershell
wheelctl moza analyze-capture --capture ci/hardware/moza-r5/<date>/captures/r5-throttle-only-sweep.jsonl --json-out target/moza-passive-checks/r5-throttle-byte-delta.json --json
wheelctl moza analyze-lane --lane ci/hardware/moza-r5/<date> --json-out target/moza-passive-checks/lane-analysis.json --json
wheelctl moza sync-role-status --lane ci/hardware/moza-r5/<date> --json-out target/moza-passive-checks/role-status-sync.json --json
```

The analysis receipts are diagnostic only. They record raw byte and
little-endian word ranges from JSONL reports and make no semantic role claim.
`analyze-lane` also compares required captures with the lane idle capture and
reports missing parser-visible control evidence before fixture promotion.
`sync-role-status` copies those derived per-role statuses into `manifest.json`
so stale `deferred` or over-optimistic fields are not hand-edited; it still does
not promote receipts or make the passive bundle pass.

Done when:

- Steering moves monotonically through full left/right sweeps.
- Throttle, brake, clutch, and handbrake normalize correctly for observed paths.
- HBP movement is proven for the declared connection path.
- KS controls map to stable button/direction fields, and ES controls map to stable button fields.
- SR-P clutch exposure is resolved for the declared connection path.

## Phase 3: Parser Fixture Promotion

Real captures become regression fixtures only after all lines replay through the parser.

```powershell
wheelctl moza validate-captures --lane ci/hardware/moza-r5/<date> --json-out ci/hardware/moza-r5/<date>/parser-fixture-validation.json
wheelctl moza promote-fixtures --lane ci/hardware/moza-r5/<date> --fixture-dir crates/hid-moza-protocol/fixtures/moza-r5-<date> --json-out ci/hardware/moza-r5/<date>/fixture-promotion.json
cargo test -p racing-wheel-hid-moza-protocol promoted_capture_fixtures_replay_through_moza_parser
wheelctl moza verify-bundle --lane ci/hardware/moza-r5/<date> --stage passive --json-out ci/hardware/moza-r5/<date>/passive-verification.json
wheelctl moza promote-manifest --lane ci/hardware/moza-r5/<date> --stage passive --json-out ci/hardware/moza-r5/<date>/manifest-promotion-passive.json
wheelctl moza audit-lane --lane ci/hardware/moza-r5/<date> --stage passive --json-out ci/hardware/moza-r5/<date>/lane-audit-passive.json
```

When `verify-bundle` fails, its JSON receipt includes `next_commands` with the staged commands to rebuild evidence through the requested gate. For the passive stage those commands remain observe-only: enumeration, descriptor metadata, missing input capture, offline lane analysis, role-status sync, parser validation, fixture promotion, verification, manifest promotion, and lane audit. If a capture already exists but parser-visible role movement is missing, `next_commands` keeps the flow on `analyze-lane` / `sync-role-status` rather than blindly recapturing the same file. Native stages are split from external compatibility: `openracing-control-ready` is the native control foundation, `native-response-ready` proves bounded PIDFF response above the response floor, `native-visible-ready` proves operator-visible motion, and `smoke-ready` adds simulator and Pit House evidence. Pit House, SimHub, simulator telemetry, and simulator FFB must not appear as prerequisites for native control or native response. If a real response receipt exists but visible motion remains unproven and both the follow-up plan plus `pre-output-readiness.json` already exist, generated native-visible and smoke-ready guidance stops at the operator action instead of looping on a timestamp-only readiness refresh. In that blocked state, `blocked_safe_followups` may list no-output Pit House subcase or simulator telemetry commands that can gather evidence without satisfying native-visible or smoke-ready, and without authorizing another actuator output run.

Validate every passive capture before promoting fixtures. The verifier consumes `parser-fixture-validation.json` from `wheelctl moza validate-captures` as the lane summary and requires `fixture-promotion.json` from `wheelctl moza promote-fixtures`; it rejects a single passing idle capture as coverage for steering, pedals, HBP, KS, and ES. Promoted fixture entries may point to lane-relative files or repo-relative files under `crates/hid-moza-protocol/fixtures/...`, matching the documented parser fixture promotion command.

`parser-fixture-validation.json` records each capture's required product IDs, required category, axis variation, exact discriminator values, any-of control groups, minimum full-report length, and `missing_requirements`. If passive validation fails, use those fields as the operator checklist before recapturing; they identify missing evidence such as through-R5 role movement, standalone SR-P/HBP PID mismatch when direct-plug topology is declared, KS direction/button movement, ES button movement, full-length wheelbase reports, or missing `capture-input` metadata.

`verify-bundle --stage passive` also replays every required capture JSONL through the Moza parsers. The R5 captures must use the manifest-selected R5 PID. Standalone captures must use their declared PIDs only when topology requests direct-plug coverage. The steering and isolated through-R5 role captures must show movement, while KS and ES control captures must be full wheelbase reports with the expected control movement described above. Placeholder JSONL cannot satisfy passive evidence.

Promoted fixtures must cover every required passive capture and must not contain HID paths, raw serial numbers, or other per-user device identity fields.
Once fixtures are promoted into `crates/hid-moza-protocol/fixtures/`, the parser crate replay test consumes them as normal cargo-test regression coverage. The checked-in `fixtures/synthetic/parser_replay_smoke.json` file only proves the fixture schema and replay harness; it is not real hardware evidence.

## Phase 4: Zero-Torque Proof

Zero-torque proof is the first output phase. The zero command requires an explicit descriptor-trusted strategy. `--strategy direct-report-0x20` sends only report `0x20` with raw torque `0`, flags `0`, and motor disabled, and remains the required zero proof before later direct low-torque tests. `--strategy pidff-stop-all` sends only standard PIDFF Device Control report `0x0C` with Stop All Effects, which the live R5 V1 descriptor exposes as a 2-byte output report. Both strategies must refuse before HID initialization unless `pre-output-readiness.json` is passing and the same lane descriptor proves the selected report shape. `ready_for_ffb` remains false after this stage.

```powershell
wheelctl moza zero --device <r5> --lane ci/hardware/moza-r5/<date> --strategy pidff-stop-all --confirm-zero-torque --repeat 100 --hz 1000 --json-out ci/hardware/moza-r5/<date>/zero-torque-proof.json
wheelctl moza watchdog-proof --device <r5> --lane ci/hardware/moza-r5/<date> --strategy pidff-stop-all --confirm-watchdog-test --pre-zero-count 3 --watchdog-timeout-ms 100 --json-out ci/hardware/moza-r5/<date>/watchdog-proof.json
wheelctl moza disconnect-proof --device <r5> --lane ci/hardware/moza-r5/<date> --strategy pidff-stop-all --confirm-disconnect-test --max-duration-ms 10000 --json-out ci/hardware/moza-r5/<date>/disconnect-proof.json
wheelctl moza verify-bundle --lane ci/hardware/moza-r5/<date> --stage zero --json-out ci/hardware/moza-r5/<date>/zero-verification.json
wheelctl moza promote-manifest --lane ci/hardware/moza-r5/<date> --stage zero --json-out ci/hardware/moza-r5/<date>/manifest-promotion-zero.json
wheelctl moza audit-lane --lane ci/hardware/moza-r5/<date> --stage zero --json-out ci/hardware/moza-r5/<date>/lane-audit-zero.json
```

The zero-stage verifier requires detailed `watchdog-proof.json` and `disconnect-proof.json` receipts. These must come from `wheelctl moza watchdog-proof` and `wheelctl moza disconnect-proof`, not placeholders. Zero, watchdog, and disconnect receipts must include a `receipt_path` that resolves to the exact dated-lane artifact being verified plus a valid UTC `generated_at_utc`, so copied receipts from another dated lane are rejected.

The disconnect proof is an operator-coordinated bench action. Before starting it, confirm the wheel is clear, the power cutoff is understood, no simulator or vendor FFB source is active, and the selected R5 endpoint is connected. After `wheelctl moza disconnect-proof` starts, unplug the R5 USB during the `--max-duration-ms` window and leave it unplugged until the command exits. Do not reconnect during the proof window; re-enumerate the R5 with observe-only tooling before any later staged init, low-torque, or simulator work.

Done when:

- At least 100 scheduled zero reports are logged.
- A final zero is attempted and sent.
- The command log contains no non-zero payloads.
- Watchdog and disconnect receipts prove zero/final-zero behavior.
- Disconnect proof records a final-zero attempt; the write may fail if the HID handle is already gone after the operator disconnects.
- High torque remains disabled.

## Phase 5: Gated Low Torque

Before low torque, run the staged feature-report handshake in off and standard modes. Do not use direct mode in this phase.

```powershell
wheelctl moza init --device <r5> --lane ci/hardware/moza-r5/<date> --mode off --confirm-init --json-out ci/hardware/moza-r5/<date>/init-off.json
wheelctl moza init --device <r5> --lane ci/hardware/moza-r5/<date> --mode standard --confirm-init --json-out ci/hardware/moza-r5/<date>/init-standard.json
```

The live R5 V1 init receipts must show the descriptor-backed mode feature report only: sequence `0` writes feature report `0x11` (`11FF0000` for off, `11000000` for standard) to select the requested mode. The trusted R5 V1 descriptor does not expose feature report `0x03`; it exposes `0x03` as an output report, so `0x03` must not be sent by this init stage. Other wheelbase lanes may require an ordered `0x03` start-input feature report before `0x11` only when their trusted descriptor and adapter prove that feature-report shape. Each real init feature report must record a successful 4-byte write. They must not include high-torque report `0x02` or direct torque output report `0x20`.
Actual init feature-report writes require `--lane <dir>` and `--confirm-init`, and the command validates passing same-lane `zero-verification.json` plus `lane-audit-zero.json` before the HID API is initialized.

Low torque is allowed only after a passing real zero-torque proof, passing off/standard init receipts, an explicit output strategy gate, and explicit operator confirmation. The wheel must be mounted safely with hands clear and the wheel off the ground or otherwise physically safe.

```powershell
wheelctl moza torque-test --device <r5> --lane ci/hardware/moza-r5/<date> --strategy direct-report-0x20 --zero-proof ci/hardware/moza-r5/<date>/zero-torque-proof.json --descriptor ci/hardware/moza-r5/<date>/descriptor.json --confirm-low-torque --max-percent 2 --duration-ms 250 --json-out ci/hardware/moza-r5/<date>/low-torque-proof.json
```

The smoke-ready verifier must not generate the direct command until the lane proves the direct path, not just zero output. `descriptor.json` must prove trusted direct report `0x20` metadata, and `zero-torque-proof.json` must be a same-lane `direct_report_0x20` proof accepted by the torque-test preflight. If the lane only proves PIDFF Stop All zero output, generated guidance must not add `--explicit-operator-override`.

The PIDFF low-torque strategy is explicit and verifier-distinct. Its software surface validates the same-lane PIDFF Stop All zero proof, off/standard init receipts, exact lane endpoint selector, descriptor-proven PIDFF Device Control report, descriptor-proven PIDFF effect reports, and the R5-shaped Set Effect encoder:

```powershell
wheelctl moza torque-test --device <r5> --lane ci/hardware/moza-r5/<date> --strategy pidff-bounded-effect --zero-proof ci/hardware/moza-r5/<date>/zero-torque-proof.json --init-off ci/hardware/moza-r5/<date>/init-off.json --init-standard ci/hardware/moza-r5/<date>/init-standard.json --confirm-low-torque --max-percent 1 --duration-ms 150 --json-out ci/hardware/moza-r5/<date>/low-torque-proof.json
```

The implemented R5 V1 writer uses descriptor-proven PIDFF output reports only: R5-shaped Set Effect `0x01`, Set Constant Force `0x05`, Effect Operation `0x0A`, and final Device Control Stop All `0x0C`. The live R5 V1 descriptor exposes report `0x01` with a non-generic length, so the generic PIDFF encoder layout is not enough for hardware writes. A PIDFF receipt must declare `low_torque_strategy: "pidff_bounded_effect"`, bind the exact lane endpoint, prove effect setup explicitly, record bounded nonzero PIDFF writes, avoid direct report `0x20`, and end with a successful PIDFF Stop All cleanup. It cannot satisfy the direct-report verifier path. The `2026-05-13` lane contains the first real bounded PIDFF low-torque receipt; new lanes still have no low-torque evidence until the operator runs this command on hardware.

Required behavior:

- Direct strategy: direct torque report `0x20` only.
- PIDFF strategy: no direct torque report `0x20`; bounded PIDFF effect writes using descriptor-proven PIDFF reports and final Stop All cleanup.
- No high-torque feature report.
- Direct report writes require either a descriptor-derived trusted descriptor receipt in the lane for the same R5 PID or `--explicit-operator-override`; the receipt must record which gate was used.
- The verifier recomputes the expected Moza direct-torque payload from the R5 PID and claimed percent and rejects command logs whose raw payload, torque, flags, or motor-enable state do not match.
- Actual low-torque writes require `--lane <dir>` plus passing same-lane `zero-torque-proof.json`, `init-off.json`, `init-standard.json`, and strategy-specific descriptor evidence before the HID API is initialized. `--init-off`, `--init-standard`, `--zero-proof`, and `--descriptor` may be supplied explicitly only when they resolve to those same dated-lane artifacts. Zero/init/low-torque receipts must include same-lane `receipt_path` provenance and valid UTC timestamps. The low-torque receipt embeds timestamp and CRC summaries for the zero/off/standard prerequisites; the verifier re-reads the same dated lane files and rejects stale, off-lane, or newer prerequisite receipts.
- Use `--explicit-operator-override` only as a deliberate manual decision when the operator accepts that direct report `0x20` is not proven by descriptor metadata; generated next-commands must not print that override, and high torque remains disabled.
- Ladder at or below 2 percent of R5 max torque.
- Abort to final zero on any HID write error.
- Receipt logs every command and final zero.

## Phase 6: Native Response And Visible Motion

OpenRacing native movement is independent of Pit House, SimHub, and simulator telemetry. The native path is descriptor, init, Stop All/zero, steering stream, bounded PIDFF output, measured response, then intentionally reviewed visible motion. External compatibility receipts are useful later, but they are not prerequisites for native OpenRacing control.

The 1 percent actuator-profile receipt proves the native PIDFF output rail and cleanup path, but it does not claim visible motion. A bounded 5 percent / 2000 ms R5 PIDFF command can reasonably produce sub-degree motion after firmware filtering, friction, damping, wheel inertia, and centering behavior. The preserved 2026-05-13 response-only receipt and the 2026-05-17 bounded-shaped micro-profile receipt each measured about 0.181 degrees, sent PIDFF reports, and sent final Stop All; classify those receipts as actuator-response evidence, not as failed output paths.

```powershell
wheelctl moza steering-stream-proof --device <r5> --lane ci/hardware/moza-r5/<date> --duration-ms 5000 --jsonl-out ci/hardware/moza-r5/<date>/steering-angle-stream.jsonl --json-out ci/hardware/moza-r5/<date>/steering-angle-stream-proof.json
wheelctl moza verify-bundle --lane ci/hardware/moza-r5/<date> --stage openracing-control-ready --json-out ci/hardware/moza-r5/<date>/openracing-control-verification.json
wheelctl moza promote-manifest --lane ci/hardware/moza-r5/<date> --stage openracing-control-ready --json-out ci/hardware/moza-r5/<date>/manifest-promotion-openracing-control.json
wheelctl moza audit-lane --lane ci/hardware/moza-r5/<date> --stage openracing-control-ready --json-out ci/hardware/moza-r5/<date>/lane-audit-openracing-control.json
wheelctl moza verify-bundle --lane ci/hardware/moza-r5/<date> --stage native-response-ready --json-out ci/hardware/moza-r5/<date>/native-response-verification.json
wheelctl moza promote-manifest --lane ci/hardware/moza-r5/<date> --stage native-response-ready --json-out ci/hardware/moza-r5/<date>/manifest-promotion-native-response.json
wheelctl moza audit-lane --lane ci/hardware/moza-r5/<date> --stage native-response-ready --json-out ci/hardware/moza-r5/<date>/lane-audit-native-response.json
```

`native-response-ready` requires measured steering delta above the response threshold, same-lane steering and actuator-profile prerequisites, bounded PIDFF strategy, successful write accounting, final Stop All cleanup, and the same high-risk exclusions as visible motion. It does not require `success=true` or `movement_observed=true` from a visible-motion receipt, and it does not require Pit House or simulator artifacts.

`native-visible-ready` remains stricter. It requires `success=true`, `movement_observed=true`, and measured steering delta that meets the visible-motion threshold. The current 0.181 degree receipts must still fail this stage. Do not rerun, raise force, extend dwell, or replace any preserved receipt merely because the bench is available. The 2026-05-17 visible-motion authorization was consumed, and the older follow-up plan is back to `planned_next_output.allowed=false`. The 2026-05-18 real 1 degree controlled-angle attempt is preserved as `native-controlled-angle-smoke.json`; it sent five bounded PIDFF writes, sent final Stop All, stayed post-stop stable, and timed out before target with about 0.181 degrees of movement. The reviewed retry is preserved separately as `native-controlled-angle-retry-smoke.json`; it used `bounded-pidff-micro-step-v2`, sent 33 bounded PIDFF writes, sent final Stop All, stayed post-stop stable, and remained in the same 0.181 degree response band. Those are safe undertravel receipts, not permission to run a third attempt; see the [controlled-angle analysis](moza-r5-controlled-angle-analysis.md).

`native-pidff-semantics-diagnosis.json` records the no-output diagnosis for this state. It classifies the evidence as `same_response_band_despite_micro_step_replay`: changing the write shape and write count did not materially change the measured motion. `native-pidff-lifecycle-trace.json` records the decoded lifecycle trace: repeated PIDFF effect setup, constant-force, effect-start, and Stop All cycles with the first and retry attempts still in the same delta band. `native-pidff-effect-lifecycle-plan.json` records the no-output software plan for `bounded-pidff-effect-lifecycle-v1`, a profile now implemented for preflight and exact-command binding that tests a different standard PIDFF effect lifecycle under the same 1 degree / 5 percent / 2000 ms ceiling. No output is authorized by these artifacts. A 5 percent / 3000 ms retry, a 5 percent / 30000 ms dwell, a 30 degree target, and a 90 degree right/left reset are not authorized. Treat 90 degrees as a later feedback-bounded controlled-angle profile: sample the start angle, command only while the wheel is moving toward a staged target, Stop All on target/timeout/wrong-way/velocity guard, then return to the start angle and Stop All again. Stage that through 1, 3, 5, 10, and 30 degree receipts before any 90 degree attempt. Dry-runs remain allowed for software-only preflight, but a dry-run is not motion evidence and does not create authorization.

```powershell
wheelctl moza receipt-template --kind visible-motion-follow-up --json-out ci/hardware/moza-r5/<date>/native-actuator-visible-follow-up-plan.json
wheelctl moza receipt-template --kind controlled-angle-plan --json-out ci/hardware/moza-r5/<date>/native-controlled-angle-plan.json
wheelctl moza verify-bundle --lane ci/hardware/moza-r5/<date> --stage native-visible-ready --json-out ci/hardware/moza-r5/<date>/native-visible-verification.json
```

## Phase 7: Pit House Coexistence

Pit House coexistence is a separate test. Do not infer it from passive capture or low-torque success.

| State | Expected result |
|-------|-----------------|
| Pit House closed | OpenRacing staged handshake works |
| Pit House open, no active tuning | Standard mode works or documents conflict |
| Pit House open, OpenRacing direct | Block or require explicit acknowledgement |
| Pit House changes FFB mode during run | Detect mismatch or fail safe |
| Pit House firmware/update page open | Refuse high-risk tests |

`pit-house-coexistence.json` is not a placeholder success file. It must include:

- `success: true`
- `high_torque: false`
- `shared_control_risk: "detected" | "warned" | "documented_limit"`
- `no_serial_config_commands: true`
- `no_firmware_or_dfu_commands: true`
- `direct_requires_ack: true`
- `firmware_page_blocks_high_risk: true`
- `cases[]` entries for `pit_house_closed`, `pit_house_open_idle_standard`, `pit_house_open_direct`, `pit_house_mode_change_during_run`, and `pit_house_firmware_update_page_open`
- `template: false`
- `evidence_status: "observed_on_real_hardware"`
- non-empty `evidence` and `artifact` fields on every case
- per-case JSON artifact files, referenced by simple lane-relative paths, whose `case`, `result`, `observed`, `high_torque`, `no_serial_config_commands`, and `no_firmware_or_dfu_commands` fields agree with the matrix row
- `pit_house_observation_artifact` on every case artifact, pointing at a separate lane-relative observation JSON file produced by `wheelctl moza pit-house-observation`; the observation must record `case`, `pit_house_observed_state`, timestamp, operator, non-notes evidence kind, and an existing lane-relative `evidence_artifact` such as a screenshot, video, or process/window snapshot
- source links on every case artifact: `source_receipt`, `source_gate`, and `source_log`, with `source_record_kinds` or `source_record_kind` where the evidence is a command log. The verifier reuses the linked gates instead of trusting operator booleans alone.
- case-specific artifact evidence: closed Pit House records `staged_handshake_ready` linked to `init-off.json` / `init_off_handshake` feature reports; open idle records `ffb_mode: "standard"` linked to `init-standard.json` / `init_standard_handshake`; direct mode records `blocked` or `operator_ack_required` linked to `low-torque-proof.json` / `low_torque_bounded`; mode change records `mismatch_detected`, `failed_safe`, output cleared, and final-zero attempted linked to `simulator-ffb-smoke.json` / `simulator_ffb_bounded` with a `clear_zero` record tagged `source_clear_event: "mode_mismatch"` and `source_requires_final_zero: true`; firmware/update page records `high_risk_refused` linked to `support-bundle.json` / `service_status_receipts`

The closed, open-idle, direct-block, and firmware-page artifacts can be collected during this phase. The `pit_house_mode_change_during_run` artifact and the final `pit-house-coexistence.json` parent receipt must be generated after Phase 8 bounded simulator FFB smoke, because the verifier requires the mode-change case to link to `simulator-ffb-smoke.json` and a `clear_zero` output record tagged `mode_mismatch`.

Generate each Pit House state observation receipt with the dedicated no-HID observation command, then reference the resulting JSON from the matching case artifact. Operator notes alone are not verifier-accepted evidence; save a lane-relative screenshot, video, or process/window snapshot first and pass it as `--evidence-artifact`. Before open-state cases, record `pit-house-availability.json`; it is a non-claiming install/process/window snapshot and does not satisfy `pit-house-coexistence.json`. If it records Pit House unavailable, leave smoke-ready blocked rather than fabricating open-state evidence. For process/window evidence on Windows, use `pit-house-evidence`; it writes a no-output JSON snapshot and `pit-house-observation` rejects wheelctl-generated snapshots that contradict the requested case. Verifier-generated evidence commands include `--require-match` so an unavailable or wrong Pit House state fails before writing a stale artifact:

```powershell
wheelctl moza pit-house-availability --operator Steven --evidence "Pit House install/process/window availability snapshot." --json-out ci/hardware/moza-r5/<date>/pit-house-availability.json
wheelctl moza pit-house-evidence --case closed --operator Steven --evidence "Pit House closed before staged handshake." --require-match --json-out ci/hardware/moza-r5/<date>/pit-house-evidence-closed.json
wheelctl moza pit-house-evidence --case open-standard --operator Steven --evidence "Pit House open and idle while standard mode completed." --require-match --json-out ci/hardware/moza-r5/<date>/pit-house-evidence-open-standard.json
wheelctl moza pit-house-evidence --case open-direct --operator Steven --evidence "Pit House open while direct mode was blocked or required acknowledgement." --require-match --json-out ci/hardware/moza-r5/<date>/pit-house-evidence-open-direct.json
wheelctl moza pit-house-evidence --case mode-change --operator Steven --evidence "Pit House mode change observed during bounded run; output cleared." --require-match --json-out ci/hardware/moza-r5/<date>/pit-house-evidence-mode-change.json
wheelctl moza pit-house-evidence --case firmware-page --operator Steven --evidence "Pit House firmware/update page open; high-risk tests refused." --require-match --json-out ci/hardware/moza-r5/<date>/pit-house-evidence-firmware-page.json
wheelctl moza pit-house-observation --case closed --evidence-kind process-window-snapshot --evidence-artifact pit-house-evidence-closed.json --evidence "Pit House closed before staged handshake." --json-out ci/hardware/moza-r5/<date>/pit-house-observation-closed.json
wheelctl moza pit-house-observation --case open-standard --evidence-kind process-window-snapshot --evidence-artifact pit-house-evidence-open-standard.json --evidence "Pit House open and idle while standard mode completed." --json-out ci/hardware/moza-r5/<date>/pit-house-observation-open-standard.json
wheelctl moza pit-house-observation --case open-direct --evidence-kind process-window-snapshot --evidence-artifact pit-house-evidence-open-direct.json --evidence "Pit House open while direct mode was blocked or required acknowledgement." --json-out ci/hardware/moza-r5/<date>/pit-house-observation-open-direct.json
wheelctl moza pit-house-observation --case mode-change --evidence-kind process-window-snapshot --evidence-artifact pit-house-evidence-mode-change.json --evidence "Pit House mode change observed during bounded run; output cleared." --json-out ci/hardware/moza-r5/<date>/pit-house-observation-mode-change.json
wheelctl moza pit-house-observation --case firmware-page --evidence-kind process-window-snapshot --evidence-artifact pit-house-evidence-firmware-page.json --evidence "Pit House firmware/update page open; high-risk tests refused." --json-out ci/hardware/moza-r5/<date>/pit-house-observation-firmware-page.json
```

Build the five case artifacts from those observations and the lane's already verified source receipts:

```powershell
wheelctl moza pit-house-case --lane ci/hardware/moza-r5/<date> --case closed --observation-artifact pit-house-observation-closed.json --evidence "Pit House closed; staged init remained ready." --json-out ci/hardware/moza-r5/<date>/pit-house-closed.json
wheelctl moza pit-house-case --lane ci/hardware/moza-r5/<date> --case open-standard --observation-artifact pit-house-observation-open-standard.json --evidence "Pit House open and idle; standard mode completed without conflict." --json-out ci/hardware/moza-r5/<date>/pit-house-open-standard.json
wheelctl moza pit-house-case --lane ci/hardware/moza-r5/<date> --case open-direct --observation-artifact pit-house-observation-open-direct.json --evidence "Direct mode was blocked until explicit operator acknowledgement." --json-out ci/hardware/moza-r5/<date>/pit-house-direct-blocked.json
wheelctl moza pit-house-case --lane ci/hardware/moza-r5/<date> --case mode-change --observation-artifact pit-house-observation-mode-change.json --evidence "Mode mismatch was detected and output failed safe." --json-out ci/hardware/moza-r5/<date>/pit-house-mode-change.json
wheelctl moza pit-house-case --lane ci/hardware/moza-r5/<date> --case firmware-page --observation-artifact pit-house-observation-firmware-page.json --evidence "Firmware/update page open; high-risk tests refused." --json-out ci/hardware/moza-r5/<date>/pit-house-firmware-page.json
```

After writing the five case artifact files under the lane and after the simulator FFB smoke receipt exists, generate the verifier-accepted parent receipt with:

```powershell
wheelctl moza pit-house-proof --lane ci/hardware/moza-r5/<date> --closed-artifact pit-house-closed.json --open-standard-artifact pit-house-open-standard.json --direct-artifact pit-house-direct-blocked.json --mode-change-artifact pit-house-mode-change.json --firmware-page-artifact pit-house-firmware-page.json --shared-control-risk warned --json-out ci/hardware/moza-r5/<date>/pit-house-coexistence.json
```

The producer copies the case-specific safety booleans from each artifact into the parent receipt and fails if any case does not meet the verifier contract. To avoid hand-typing the field names during preparation, start from a non-claiming template and then replace the pending fields with real observations:

```powershell
wheelctl moza receipt-template --kind pit-house --json-out ci/hardware/moza-r5/<date>/pit-house-coexistence.json
```

The generated template has `success: false` and is intentionally rejected by `verify-bundle` until every case is observed and the safety fields are updated from receipts. For each `pit-house-observation`, capture or copy the named screenshot or video into the dated lane, or create a process/window JSON snapshot with `pit-house-evidence --require-match`, before running the command; the evidence producer refuses to write a verifier-directed artifact when the snapshot does not match the requested case, and the observation producer refuses to write a receipt if `--evidence-artifact` is missing or if a wheelctl-generated process/window snapshot contradicts the requested case.

The bundle verifier requires the direct-mode case to be blocked or require explicit acknowledgement, the mode-change case to detect mismatch or fail safe, and the firmware/update-page case to refuse high-risk tests.

## Phase 8: Simulator Proof

Start with telemetry only, then run bounded simulator-to-Moza smoke.

```text
game -> telemetry adapter -> normalized snapshot -> recorder
game -> OpenRacing engine -> bounded Moza output -> receipt
```

The first simulator row can be Assetto Corsa, ACC, iRacing, or SimHub bridge. The manifest may set `simulator_validated=true` only after both telemetry proof and bounded FFB smoke receipts exist and `wheelctl moza promote-manifest --stage smoke-ready` has passed live bundle verification.

`simulator-telemetry-proof.json` must prove telemetry-only operation with `hardware_output_enabled: false`, `no_ffb_writes: true`, `no_serial_config_commands: true`, `no_firmware_or_dfu_commands: true`, at least one normalized snapshot, a recorder artifact path, recorder provenance, and no faults. The recorder JSON/JSONL artifact must exist, contain exactly the claimed normalized snapshot count, and include normalized fields such as `speed_ms`, `steering_angle`, `throttle`, `brake`, `rpm`, `gear`, and `ffb_scalar` with sequence or timestamp ordering evidence. The artifact must also bind the parent receipt's `duration_ms` to the recording, either through matching per-record `recording_duration_ms`/`duration_ms` fields or through a timestamp span that covers the claimed duration. Every recorder record must also include provenance matching the parent receipt: `recorder_command: "wheelctl telemetry record"`, a stable non-empty `recorder_session_id`, matching `game`, matching `telemetry_source`, `hardware_output_enabled: false`, `no_ffb_writes: true`, `no_serial_config_commands: true`, and `no_firmware_or_dfu_commands: true`. First record normalized snapshots from the game adapter or SimHub bridge, then generate the Moza proof from that recorder artifact. For the first Windows bench path, record live SimHub UDP JSON telemetry:

```powershell
wheelctl telemetry record --game simhub-bridge --telemetry-source simhub_bridge --live-simhub --port 5555 --out ci/hardware/moza-r5/<date>/simulator-telemetry-recording.jsonl --session-id simhub-bridge-<date> --duration-ms 30000
wheelctl moza simulator-telemetry-proof --lane ci/hardware/moza-r5/<date> --game simhub-bridge --telemetry-source simhub_bridge --recorder-artifact simulator-telemetry-recording.jsonl --duration-ms 30000 --json-out ci/hardware/moza-r5/<date>/simulator-telemetry-proof.json
```

Existing normalized JSON/JSONL files can still be stamped with `--input` when live SimHub is not available. Checked-in fixtures remain rehearsal inputs, not real simulator evidence.

`simulator-ffb-smoke.json` must prove bounded output with `hardware: "moza-r5"`, an R5 output-capable device record, `hardware_output_enabled: true`, `no_hid_device_opened: false`, `no_ffb_writes: false`, an explicit `output_strategy`, descriptor trust cross-checked against lane `descriptor.json` for the same R5 PID and strategy, `hardware_prerequisites_validated: true`, passing `prerequisite_gates` for zero torque, watchdog, disconnect, off init, standard init, and low torque, and `prerequisite_artifacts` binding those same receipts by same-lane path, CRC, and `generated_at_utc` before `writer_started_at_utc`. It must also prove `high_torque: false`, `no_high_torque: true`, `no_serial_config_commands: true`, `no_firmware_or_dfu_commands: true`, `watchdog_active: true`, `watchdog_timeout_ms > 0`, non-zero output and zero output counts, an `input_telemetry_artifact`/`input_telemetry_snapshot_count`/`input_telemetry_recorder_session_id` link matching a passing `simulator-telemetry-proof.json`, an output log artifact, `final_zero_attempted: true`, `final_zero_sent: true`, `mode_mismatch_cleared_output: true`, a safe final-zero payload, no faults, and `max_output_percent <= 5`. The output JSON/JSONL artifact must exist, contain exactly the claimed output report count, and each record must include `payload_hex`, `report_id`, `report_len`, `torque_raw`, `motor_enabled`, signed `percent`/`output_percent`, `bytes_written`, contiguous `sequence`, monotonic advancing `elapsed_us`, `telemetry_sequence`, `input_ffb_scalar`, HID write metadata (`transport: "hid"`, `hid_write_target: "output_report"`, and `hid_write_attempted=true`), and input telemetry link fields (`input_telemetry_artifact`, `input_telemetry_snapshot_count`, `input_telemetry_recorder_session_id`, `input_telemetry_game`, and `input_telemetry_source`); every output record must also include writer provenance (`writer_command` beginning with `wheeld --hardware-lane`, `writer_hardware_lane` or `moza_lane` matching the dated lane, `writer_endpoint_selector` matching the exact lane manifest HID endpoint, a stable `writer_session_id`, ordered UTC `writer_started_at_utc`/`writer_completed_at_utc`, R5 writer device path/product identity, `hardware_output_enabled: true`, `no_hid_device_opened: false`, and `no_ffb_writes: false`) that matches the parent receipt. Direct-report smoke requires successful 8-byte report `0x20` records and a final zero payload `2000000000000000`. PIDFF smoke requires `output_strategy: "pidff_bounded_effect"`, no direct report `0x20` records, descriptor-proven PIDFF report metadata, bounded `pidff_set_constant_force` report `0x05` records, `pidff_set_effect`/`pidff_effect_start` setup records, ordered PIDFF Stop All `0x0C` cleanup records tagged with `clear_event: "stop"`, `"pause"`, `"game_exit"`, and `"mode_mismatch"`, and final Stop All payload `0C04`. Non-zero output signs must agree with `input_ffb_scalar`. Stop, pause, game exit, and mode mismatch must each record cleared output in the output log, not only as top-level receipt booleans. Generate the smoke receipt only after the telemetry proof and earlier hardware prerequisite receipts exist:

```powershell
wheeld --hardware-lane ci/hardware/moza-r5/<date>
wheelctl moza simulator-ffb-smoke --lane ci/hardware/moza-r5/<date> --game simhub-bridge --telemetry-source simhub_bridge --output-log-artifact simulator-ffb-output.jsonl --strategy pidff-bounded-effect --descriptor-trusted --watchdog-timeout-ms 100 --stop-cleared-output --pause-cleared-output --game-exit-cleared-output --json-out ci/hardware/moza-r5/<date>/simulator-ffb-smoke.json
```

Starter templates are available for the response-only visible-motion follow-up review and offline preparation of the two simulator receipts:

```powershell
wheelctl moza receipt-template --kind visible-motion-follow-up --json-out ci/hardware/moza-r5/<date>/native-actuator-visible-follow-up-plan.json
wheelctl moza receipt-template --kind controlled-angle-plan --json-out ci/hardware/moza-r5/<date>/native-controlled-angle-plan.json
wheelctl moza receipt-template --kind simulator-telemetry --json-out ci/hardware/moza-r5/<date>/simulator-telemetry-proof.json
wheelctl moza receipt-template --kind simulator-ffb --json-out ci/hardware/moza-r5/<date>/simulator-ffb-smoke.json
```

These templates also default to `success: false`. The visible-motion follow-up template is only a review artifact after a response-only real visible-motion receipt; it does not authorize another output attempt, replace the preserved response/visible-motion receipt, or satisfy the visible-motion gate. Its profile-design section records the current command limits, keeps higher or longer profile ideas in `requires_separate_software_and_safety_plan` state, and exposes profile-review fields until a separate exact authorization exists. Use `wheelctl moza authorize-visible-output` only for that older `actuator-visible-smoke` follow-up path. The controlled-angle path uses `wheelctl moza authorize-controlled-angle-output` and an exact same-lane authorization receipt; it does not need the visible-output authorizer. The controlled-angle preflight receipt may have `success: true` only for no-output preflight success; it sets `hardware_output_enabled=false`, `actual_hardware_writes_supported=false`, and `controlled_angle_motion_proven=false`, so it is not visible-motion evidence. The first failed controlled-angle attempt, reviewed retry attempt, lifecycle trace, effect-lifecycle plan, and attempt-03 dry-run preflight are all preserved; the preflight for `bounded-pidff-effect-lifecycle-v1` opened no HID device, sent no writes, proves no motion, and does not authorize output. A later real controlled-angle receipt can satisfy native visible motion only when it is non-dry-run, same-lane, feedback-bounded, final-Stop-All-cleaned, returned to start, and backed by a new exact controlled-angle authorization receipt. The simulator templates must not be used as evidence until a real telemetry recording or bounded FFB smoke run fills the fields.
The simulator FFB template includes operator-pending `prerequisite_gates`, same-lane `prerequisite_artifacts`, telemetry session, and writer timing placeholders so the required provenance fields are visible before the real run.

After passing native visible-motion, simulator, and Pit House receipts exist, write the stored smoke-ready verification receipt:

```powershell
wheelctl moza verify-bundle --lane ci/hardware/moza-r5/<date> --stage smoke-ready --json-out ci/hardware/moza-r5/<date>/smoke-ready-verification.json
```

After `smoke-ready-verification.json` passes, promote the manifest claim with:

```powershell
wheelctl moza promote-manifest --lane ci/hardware/moza-r5/<date> --stage smoke-ready --json-out ci/hardware/moza-r5/<date>/manifest-promotion-smoke-ready.json
wheelctl moza audit-lane --lane ci/hardware/moza-r5/<date> --stage smoke-ready --json-out ci/hardware/moza-r5/<date>/lane-audit-smoke-ready.json
```

`promote-manifest` reruns the live bundle gates before and after changing `manifest.json`; it keeps `release_ready: false` and `high_torque_validated: false`. `audit-lane` is the post-promotion completeness check: it reruns the requested live bundle verification and checks that the stored `*-verification.json` and `manifest-promotion-*.json` receipts through that stage are present, successful, non-claiming, and still carry matching before/after verification summaries with zero missing artifacts, invalid artifacts, and failed gates. The `lane-audit-*.json` receipts are first-class lane artifacts and should be regenerated after each promotion. `audit-lane` opens no HID device and sends no reports.

## Support Bundle Context

When asking for help on a Moza lane, include the lane verifier summaries in the support bundle:

```powershell
wheelctl support-bundle --device <r5> --moza-lane ci/hardware/moza-r5/<date> --output ci/hardware/moza-r5/<date>/support-bundle.json
```

Use the top-level `wheelctl support-bundle --device <r5>` form for the lane artifact so the receipt records the device filter and the Phase 9 checklist command shape. `wheelctl diag support --moza-lane ...` remains useful for ad hoc triage, but it is not sufficient for the smoke-ready service-status gate.

The smoke-ready verifier requires `moza-status.json`, `device-status.json`, and `support-bundle.json`. These receipts must all identify the same R5 PID, including the support bundle's top-level `devices[]` entry and service-facing `device_statuses[]` snapshot. They must keep `ffb_ready`, direct mode, high torque, and `safe_to_send_torque` false, include descriptor CRC/source where service status is involved, declare no FFB/serial/firmware/DFU commands, and keep support-bundle readiness as diagnostic context with `release_ready: false`. During service-status verification, the Moza support-bundle section is checked against a fresh lane read on a no-overclaim basis: a bundle may conservatively show an earlier stage or a missing artifact from when it was generated, but it cannot claim a passing readiness flag, lane-audit flag, highest stage, or artifact `pass` state that the current lane cannot prove.

The support bundle includes service-facing `device_statuses` snapshots plus a Moza section with an `artifact_index` for every required lane receipt/capture, including stored verification, manifest-promotion, and lane-audit receipts even when they are still missing, and a diagnostic `readiness` summary with `highest_passing_stage`, `next_required_stage`, `first_blocking_stage`, `ready_for_zero_torque`, `ready_for_low_torque`, `ready_for_native_control`, `native_actuator_response_proven`, `native_visible_motion_proven`, `ready_for_external_compatibility`, `ready_for_real_hardware_smoke`, lane-audit booleans, and `release_ready: false`. Each artifact-index entry must record the path, kind, required stage, existence/validity booleans, and a consistent `pass`, `missing`, or `invalid` status. `ready_for_zero_torque` requires the passive verifier, `lane-audit-passive.json`, and at least one implemented descriptor-trusted zero-output strategy; `ready_for_low_torque` requires either the descriptor/direct zero path for `direct_report_0x20` or the descriptor-proven PIDFF bounded-effect path with same-lane PIDFF zero and init receipts; `ready_for_native_control` tracks the OpenRacing-owned movement path and excludes SimHub/Pit House; `native_actuator_response_proven` and `native_visible_motion_proven` distinguish measured PIDFF response from operator-visible motion; `ready_for_external_compatibility` tracks simulator bridge and vendor-app coexistence gates; `ready_for_real_hardware_smoke` requires the smoke-ready verifier plus `lane-audit-smoke-ready.json`. This summary helps triage missing receipts and failed gates, but it is not a readiness promotion by itself.

For a checked-in human artifact map, use the no-output renderer:

```powershell
wheelctl moza artifact-index --lane ci/hardware/moza-r5/<date> --md-out ci/hardware/moza-r5/<date>/index.md
```

`artifact-index` reads stored lane receipts and support/status diagnostics, groups artifacts by evidence area, and writes Markdown/JSON navigation only. It opens no HID device, sends no output or feature reports, creates no authorization receipt, and does not satisfy or promote any readiness gate.

For operator-facing no-output navigation, use:

```powershell
wheelctl moza bench-wizard --lane ci/hardware/moza-r5/<date> --json-out target/moza-bench-wizard.json --md-out target/moza-bench-wizard.md --json
```

`bench-wizard` reads stored lane artifacts, summarizes the current frontier and next operator step, lists safe no-output refresh commands, and records active blockers. It is not an interactive hardware runner: it does not open HID, does not write output or feature reports, does not create a controlled-angle authorization receipt, and does not generate permission to rerun, extend dwell, raise force, run 30/90 degrees, use direct report `0x20`, high torque, serial config, firmware, or DFU.

This command reads lane receipts only; it opens no HID device and sends no reports. The Moza section is diagnostic context for missing artifacts and failed gates, not a manifest promotion or compatibility claim.

## Claim Rules

- No high torque by default.
- No direct mode without descriptor trust or explicit operator override.
- No non-zero torque before zero-torque proof passes.
- No "supported" claim without receipt-backed validation.
- No release-ready claim from this lane.
- No serial config work until the HID input/FFB path is proven.
- No firmware or DFU commands.
