# Moza R5 Hardware Lane Artifacts

This directory holds receipt bundles for the first OpenRacing Moza validation lane:

```text
moza-r5-windows-usb
```

The lane targets Steven's Moza R5 wheelbase with KS/ES wheels, SR-P pedals, and HBP handbrake on Windows over USB HID. It is a validation lane, not a release claim.

## Directory Shape

Create one dated directory per real hardware run:

```text
ci/hardware/moza-r5/YYYY-MM-DD/
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

The manifest must validate against `manifest.schema.json`.

The operator-facing prompt-to-artifact checklist lives at `docs/hardware/moza-r5-artifact-checklist.md`. Use it before updating any validation row or manifest state; it maps each bring-up claim to the required receipt and verifier gate.

## Manifest Starter

Use this state before any hardware evidence exists:

```json
{
  "schema_version": 1,
  "lane": "moza-r5-windows-usb",
  "completion_state": "not_started",
  "generated_at_utc": "2026-05-06T00:00:00Z",
  "operator": "Steven",
  "platform": {
    "os": "Windows",
    "transport": {
      "hid": true,
      "serial_config": false
    }
  },
  "hardware": {
    "wheelbase": "Moza R5",
    "wheelbase_pid": "0x0014",
    "rims": ["KS", "ES"],
    "pedals": ["SR-P"],
    "handbrake": "HBP"
  },
  "topology": {
    "primary_input_path": "wheelbase_hub",
    "endpoints": [
      {
        "id": "moza-r5-if2",
        "kind": "wheelbase_hub",
        "vendor_id": "0x346E",
        "product_id": "0x0014",
        "interface_number": 2,
        "usage_page": "0x0001",
        "usage": "0x0004",
        "output_capable": true
      }
    ],
    "logical_controls": {
      "steering": {
        "role": "steering",
        "source_endpoint": "moza-r5-if2",
        "connection": "wheelbase_hub",
        "required": true,
        "evidence_capture": "captures/r5-steering-sweep.jsonl",
        "semantic_status": "deferred"
      },
      "ks_rim_controls": {
        "role": "rim_controls",
        "rim": "KS",
        "source_endpoint": "moza-r5-if2",
        "connection": "wheelbase_hub",
        "required": true,
        "evidence_capture": "captures/ks-controls.jsonl",
        "semantic_status": "deferred"
      },
      "es_rim_controls": {
        "role": "rim_controls",
        "rim": "ES",
        "source_endpoint": "moza-r5-if2",
        "connection": "wheelbase_hub",
        "required": true,
        "evidence_capture": "captures/es-controls.jsonl",
        "semantic_status": "deferred"
      },
      "throttle": {
        "role": "throttle",
        "source_endpoint": "moza-r5-if2",
        "connection": "wheelbase_hub",
        "required": true,
        "evidence_capture": "captures/r5-throttle-only-sweep.jsonl",
        "semantic_status": "deferred"
      },
      "brake": {
        "role": "brake",
        "source_endpoint": "moza-r5-if2",
        "connection": "wheelbase_hub",
        "required": true,
        "evidence_capture": "captures/r5-brake-only-sweep.jsonl",
        "semantic_status": "deferred"
      },
      "clutch": {
        "role": "clutch",
        "source_endpoint": "moza-r5-if2",
        "connection": "wheelbase_hub",
        "required": true,
        "evidence_capture": "captures/r5-clutch-only-sweep.jsonl",
        "semantic_status": "deferred"
      },
      "handbrake": {
        "role": "handbrake",
        "source_endpoint": "moza-r5-if2",
        "connection": "wheelbase_hub",
        "required": true,
        "evidence_capture": "captures/r5-handbrake-only-sweep.jsonl",
        "semantic_status": "deferred"
      }
    }
  },
  "claims": {
    "ffb": "staged",
    "high_torque": false,
    "pit_house_coexistence": "tested_separately"
  },
  "hardware_validated": false,
  "simulator_validated": false,
  "high_torque_validated": false,
  "release_ready": false,
  "artifacts": {
    "manifest": "manifest.json",
    "device_list": "device-list.json",
    "hid_list": "hid-list.json",
    "moza_probe": "moza-probe.json",
    "hardware_doctor": "hardware-doctor.json",
    "descriptor": "descriptor.json",
    "captures_dir": "captures",
    "capture_r5_idle": "captures/r5-idle.jsonl",
    "capture_r5_steering_sweep": "captures/r5-steering-sweep.jsonl",
    "capture_r5_throttle_only_sweep": "captures/r5-throttle-only-sweep.jsonl",
    "capture_r5_brake_only_sweep": "captures/r5-brake-only-sweep.jsonl",
    "capture_r5_clutch_only_sweep": "captures/r5-clutch-only-sweep.jsonl",
    "capture_r5_handbrake_only_sweep": "captures/r5-handbrake-only-sweep.jsonl",
    "capture_r5_aggregated_idle_after_controls": "captures/r5-aggregated-idle-after-controls.jsonl",
    "capture_ks_controls": "captures/ks-controls.jsonl",
    "capture_es_controls": "captures/es-controls.jsonl",
    "parser_fixture_validation": "parser-fixture-validation.json",
    "fixture_promotion": "fixture-promotion.json",
    "passive_verification": "passive-verification.json",
    "passive_manifest_promotion": "manifest-promotion-passive.json",
    "passive_lane_audit": "lane-audit-passive.json",
    "init_off": "init-off.json",
    "init_standard": "init-standard.json",
    "moza_status": "moza-status.json",
    "device_status": "device-status.json",
    "support_bundle": "support-bundle.json",
    "zero_torque_proof": "zero-torque-proof.json",
    "watchdog_proof": "watchdog-proof.json",
    "disconnect_proof": "disconnect-proof.json",
    "zero_verification": "zero-verification.json",
    "zero_manifest_promotion": "manifest-promotion-zero.json",
    "zero_lane_audit": "lane-audit-zero.json",
    "low_torque_proof": "low-torque-proof.json",
    "steering_angle_stream_proof": "steering-angle-stream-proof.json",
    "native_actuator_profile_smoke": "native-actuator-profile-smoke.json",
    "native_actuator_visible_smoke": "native-actuator-visible-smoke.json",
    "openracing_control_verification": "openracing-control-verification.json",
    "openracing_control_manifest_promotion": "manifest-promotion-openracing-control.json",
    "openracing_control_lane_audit": "lane-audit-openracing-control.json",
    "pit_house_coexistence": "pit-house-coexistence.json",
    "simulator_telemetry_proof": "simulator-telemetry-proof.json",
    "simulator_ffb_smoke": "simulator-ffb-smoke.json",
    "smoke_ready_verification": "smoke-ready-verification.json",
    "smoke_ready_manifest_promotion": "manifest-promotion-smoke-ready.json",
    "smoke_ready_lane_audit": "lane-audit-smoke-ready.json"
  },
  "notes": [
    "No compatibility claim is made until receipts exist and the verifier passes.",
    "No serial configuration, firmware update, or DFU command is in scope."
  ]
}
```

Each logical control declares a `semantic_status` so the manifest can distinguish
planned evidence from parser-backed proof. New lanes start at `deferred`.
`proven` means parser-visible role-specific evidence exists, `generic_aux`
means movement is visible only through generic R5 V1 extended fields, `missing`
means the selected capture parsed but did not satisfy the role, and
`unavailable` means the role has no selected capture or endpoint evidence.
The `hardware` section is declared inventory for the bench profile: only the R5
wheelbase identity is mandatory, while rims, pedals, and handbrake are optional
inventory hints. Required evidence comes from `topology.logical_controls`, not
from a fixed Moza kit checklist.

## Verification Commands

Initialize a dated lane directory:

```powershell
wheelctl moza init-lane --lane ci/hardware/moza-r5/YYYY-MM-DD --wheelbase-pid 0x0014 --operator Steven
```

Passive evidence:

```powershell
wheelctl device list --hid-observe-only --json-out ci/hardware/moza-r5/YYYY-MM-DD/device-list.json
wheelctl moza probe --json-out ci/hardware/moza-r5/YYYY-MM-DD/moza-probe.json
hid-capture list --vendor 0x346E --json-out ci/hardware/moza-r5/YYYY-MM-DD/hid-list.json
wheelctl hardware doctor --json-out ci/hardware/moza-r5/YYYY-MM-DD/hardware-doctor.json
wheelctl moza descriptor --json-out ci/hardware/moza-r5/YYYY-MM-DD/descriptor.json
wheelctl moza capture-input --device <r5> --duration-ms 5000 --json-out ci/hardware/moza-r5/YYYY-MM-DD/captures/r5-idle.jsonl
wheelctl moza capture-input --device <r5> --duration-ms 10000 --json-out ci/hardware/moza-r5/YYYY-MM-DD/captures/r5-steering-sweep.jsonl
wheelctl moza capture-input --device <r5> --duration-ms 10000 --json-out ci/hardware/moza-r5/YYYY-MM-DD/captures/r5-throttle-only-sweep.jsonl
wheelctl moza capture-input --device <r5> --duration-ms 10000 --json-out ci/hardware/moza-r5/YYYY-MM-DD/captures/r5-brake-only-sweep.jsonl
wheelctl moza capture-input --device <r5> --duration-ms 10000 --json-out ci/hardware/moza-r5/YYYY-MM-DD/captures/r5-clutch-only-sweep.jsonl
wheelctl moza capture-input --device <r5> --duration-ms 10000 --json-out ci/hardware/moza-r5/YYYY-MM-DD/captures/r5-handbrake-only-sweep.jsonl
wheelctl moza capture-input --device <r5> --duration-ms 5000 --json-out ci/hardware/moza-r5/YYYY-MM-DD/captures/r5-aggregated-idle-after-controls.jsonl
wheelctl moza capture-input --device <r5> --duration-ms 10000 --json-out ci/hardware/moza-r5/YYYY-MM-DD/captures/ks-controls.jsonl
wheelctl moza capture-input --device <r5> --duration-ms 10000 --json-out ci/hardware/moza-r5/YYYY-MM-DD/captures/es-controls.jsonl
wheelctl moza sync-role-status --lane ci/hardware/moza-r5/YYYY-MM-DD --json-out target/moza-role-status-sync.json
wheelctl moza validate-captures --lane ci/hardware/moza-r5/YYYY-MM-DD --json-out ci/hardware/moza-r5/YYYY-MM-DD/parser-fixture-validation.json
wheelctl moza promote-fixtures --lane ci/hardware/moza-r5/YYYY-MM-DD --fixture-dir crates/hid-moza-protocol/fixtures/moza-r5-YYYY-MM-DD --json-out ci/hardware/moza-r5/YYYY-MM-DD/fixture-promotion.json
wheelctl moza verify-bundle --lane ci/hardware/moza-r5/YYYY-MM-DD --stage passive --json-out ci/hardware/moza-r5/YYYY-MM-DD/passive-verification.json
wheelctl moza promote-manifest --lane ci/hardware/moza-r5/YYYY-MM-DD --stage passive --json-out ci/hardware/moza-r5/YYYY-MM-DD/manifest-promotion-passive.json
wheelctl moza audit-lane --lane ci/hardware/moza-r5/YYYY-MM-DD --stage passive --json-out ci/hardware/moza-r5/YYYY-MM-DD/lane-audit-passive.json
```

If a `verify-bundle` receipt fails, inspect `next_commands`. The verifier fills that field with the staged command sequence needed for the requested gate; at `--stage passive` it contains only no-FFB observation and offline parser commands. Existing parseable captures are not blindly recaptured from that list; failed role movement is diagnosed with lane analysis and role-status sync first.

The passive verifier requires the R5 VID/PID in `device-list.json`, `moza-probe.json`, `hid-list.json`, and `descriptor.json`, then validates the manifest topology instead of requiring one fixed kit shape. `hardware-doctor.json` is a first-class passive safety receipt for redacted platform diagnostics, including Windows PnP topology when available; it must remain observe-only and must not open HID handles, send output or feature reports, touch serial config, or run firmware/DFU commands. The primary Moza path is the R5 wheelbase hub: steering, KS/ES rim controls, SR-P throttle/brake/clutch, and HBP handbrake may all be proven from the R5 aggregated HID endpoint. Standalone SR-P (`0x0003`) and HBP (`0x0022`) records are optional direct-plug coverage only when the manifest declares `connection: "standalone_usb"` for that logical role. The manifest's `hardware.wheelbase_pid` pins the exact R5 row being validated, so every R5 enumeration record, wheelbase-hub capture, promoted wheelbase parser fixture, output-capable receipt, service receipt, and simulator writer receipt must use that same PID (`0x0004` or `0x0014`). Run `wheelctl moza descriptor` vendor-wide for the lane receipt so `descriptor.json` contains the observed Moza records. The R5 descriptor record must include a descriptor source (`linux_sysfs` or `operator_supplied_hex`), CRC, manufacturer, interface/usage metadata, descriptor-derived R5 input lengths for the observed PID, and observed descriptor-derived output/feature report metadata. The live R5 V1 lane uses the observed 42-byte aggregated input report shape; older R5 descriptor receipts may still expose the legacy 7/31-byte input report shape. If Windows cannot expose raw R5 descriptor bytes, rerun `wheelctl moza descriptor --device <r5> --report-descriptor-hex "<hex bytes>" ...`, `wheelctl moza descriptor --device <r5> --report-descriptor-hex-file <file> ...`, or `wheelctl moza descriptor --device <r5> --report-descriptor-bin-file <file> ...` with bytes from USBTreeView raw HID report descriptor hex, USBPcap/Wireshark enumeration capture of the HID Report Descriptor response, a raw binary Linux sysfs `report_descriptor` dump, or an equivalent descriptor tool; that command preserves the vendor-wide Moza records and applies the supplied descriptor bytes only to the one selected R5 record. USB device/interface descriptor fields, `wDescriptorLength` summaries, Windows HidP KDR/preparsed blobs, driver replacement, WinUSB switching, output reports, feature reports, serial config, firmware, and DFU are not descriptor evidence. `hid-capture descriptor --vendor 0x346E` is still an accepted lower-level producer for the same receipt shape, but the runbook uses the wheelctl command so all Moza receipts share one command surface. Descriptor commands parse supplied or sysfs descriptor bytes into report lengths and IDs; they set `report_metadata_source="report_descriptor_parsed"` only when that metadata came from descriptor bytes. Protocol-expected report metadata is passive evidence only; direct zero-output and direct-mode descriptor trust require descriptor-derived report metadata plus stored `report_descriptor_hex` whose CRC, parsed report IDs, and parsed `0x20` output report length match lane `descriptor.json`, or an explicit operator override for later direct-mode stages. Passive receipts must come from the expected observation commands, have `success=true`, and declare `no_ffb_writes=true`, `no_serial_config_commands=true`, and `no_firmware_or_dfu_commands=true`; pure observation receipts must also declare `no_hid_device_opened=true`. The verifier requires `parser-fixture-validation.json` from `wheelctl moza validate-captures` and `fixture-promotion.json` from `wheelctl moza promote-fixtures`, both covering every required capture; promoted fixtures may be lane-relative or repo-relative under `crates/hid-moza-protocol/fixtures/...`. Raw capture JSONL must come from `wheelctl moza capture-input` with per-line `command`, no-output assertions, timestamp, path, interface, usage, report ID, product, and VID/PID metadata on every line. Required R5 hub captures must use the manifest-selected R5 PID and prove one logical control at a time: steering, throttle, brake, clutch, handbrake, KS controls, and ES controls. KS control captures must show KS control variation. ES control captures must show button movement; ES does not have a hat/funky control, so passive verification must not require hat/funky variation for that rim. Placeholder standalone files are not accepted for through-base topologies.

`parser-fixture-validation.json` includes per-capture `missing_requirements` plus the expected product IDs, category, axes, exact discriminator values, any-of control groups, and minimum report length. Treat those fields as the recapture checklist when passive validation fails.

Optional observe-only status preflight:

```powershell
wheeld --hardware-lane moza-r5
wheeld --hardware-lane ci/hardware/moza-r5/YYYY-MM-DD
wheelctl moza status --device <r5> --lane ci/hardware/moza-r5/YYYY-MM-DD --json-out ci/hardware/moza-r5/YYYY-MM-DD/moza-status.json
wheelctl device status <r5> --moza-lane ci/hardware/moza-r5/YYYY-MM-DD --json-out ci/hardware/moza-r5/YYYY-MM-DD/device-status.json --json
```

`wheeld --hardware-lane moza-r5` labels service-side readiness; when the value is a lane directory or `descriptor.json`, the service also reports descriptor CRC/source/trust from the receipt. When a lane directory contains stored verifier receipts, the service reports the highest stored lane stage in `safety_state`/`safety_reason` as diagnostic context only; when `zero-verification.json`, `init-off.json`, and `init-standard.json` all pass, status may say the low-torque gate receipts are observed while torque readiness stays disabled. `wheelctl device status --moza-lane --json-out` writes the service status receipt with the same descriptor and stored-stage overlay for a Moza VID/PID. These commands must not initialize Moza protocol or send HID output. Service and CLI status remain observe-only until the lane contains passing init, zero, and torque receipts.

Zero-torque evidence:

```powershell
wheelctl moza zero --device <r5> --repeat 100 --hz 1000 --json-out ci/hardware/moza-r5/YYYY-MM-DD/zero-torque-proof.json
wheelctl moza watchdog-proof --device <r5> --pre-zero-count 3 --watchdog-timeout-ms 100 --json-out ci/hardware/moza-r5/YYYY-MM-DD/watchdog-proof.json
wheelctl moza disconnect-proof --device <r5> --confirm-disconnect-test --max-duration-ms 10000 --json-out ci/hardware/moza-r5/YYYY-MM-DD/disconnect-proof.json
wheelctl moza verify-bundle --lane ci/hardware/moza-r5/YYYY-MM-DD --stage zero --json-out ci/hardware/moza-r5/YYYY-MM-DD/zero-verification.json
wheelctl moza promote-manifest --lane ci/hardware/moza-r5/YYYY-MM-DD --stage zero --json-out ci/hardware/moza-r5/YYYY-MM-DD/manifest-promotion-zero.json
wheelctl moza audit-lane --lane ci/hardware/moza-r5/YYYY-MM-DD --stage zero --json-out ci/hardware/moza-r5/YYYY-MM-DD/lane-audit-zero.json
```

Zero proof requires an explicit descriptor-trusted zero-output strategy. `wheelctl moza zero --strategy direct-report-0x20` uses only direct torque report `0x20` with the zero payload and must refuse unless the lane descriptor proves report `0x20` with the expected 8-byte shape; the observed live R5 V1 descriptor currently does not prove that direct report. `wheelctl moza zero --strategy pidff-stop-all` uses only standard PIDFF Device Control report `0x0C` with Stop All Effects when the lane descriptor proves that 2-byte output report. `pre-output-readiness.json` inventories descriptor-observed zero-output strategy candidates and may make `ready_for_zero_torque=true` when at least one implemented strategy is trusted, while `ready_for_native_control`, `ready_for_external_compatibility`, and `ready_for_ffb` stay separate. Watchdog and disconnect proofs intentionally exercise fault paths but still require no non-zero payloads and final-zero evidence with the same selected strategy.

After staged init, service/status receipts, bounded low torque, native steering-angle stream, and native actuator-profile smoke pass, promote the native OpenRacing control foundation before collecting optional external compatibility receipts:

```powershell
wheelctl moza steering-stream-proof --device <r5> --lane ci/hardware/moza-r5/YYYY-MM-DD --duration-ms 5000 --jsonl-out ci/hardware/moza-r5/YYYY-MM-DD/steering-angle-stream.jsonl --json-out ci/hardware/moza-r5/YYYY-MM-DD/steering-angle-stream-proof.json
wheelctl moza verify-bundle --lane ci/hardware/moza-r5/YYYY-MM-DD --stage openracing-control-ready --json-out ci/hardware/moza-r5/YYYY-MM-DD/openracing-control-verification.json
wheelctl moza promote-manifest --lane ci/hardware/moza-r5/YYYY-MM-DD --stage openracing-control-ready --json-out ci/hardware/moza-r5/YYYY-MM-DD/manifest-promotion-openracing-control.json
wheelctl moza audit-lane --lane ci/hardware/moza-r5/YYYY-MM-DD --stage openracing-control-ready --json-out ci/hardware/moza-r5/YYYY-MM-DD/lane-audit-openracing-control.json
```

This stage sets `hardware_validated=true` and `simulator_validated=false`. It does not require SimHub, Pit House, simulator telemetry, or simulator FFB, but it does require native steering feedback and native bounded PIDFF actuator evidence.

Full real-hardware smoke evidence:

```powershell
wheelctl moza init --device <r5> --lane ci/hardware/moza-r5/YYYY-MM-DD --mode off --confirm-init --json-out ci/hardware/moza-r5/YYYY-MM-DD/init-off.json
wheelctl moza init --device <r5> --lane ci/hardware/moza-r5/YYYY-MM-DD --mode standard --confirm-init --json-out ci/hardware/moza-r5/YYYY-MM-DD/init-standard.json
wheelctl moza torque-test --device <r5> --lane ci/hardware/moza-r5/YYYY-MM-DD --strategy pidff-bounded-effect --zero-proof ci/hardware/moza-r5/YYYY-MM-DD/zero-torque-proof.json --init-off ci/hardware/moza-r5/YYYY-MM-DD/init-off.json --init-standard ci/hardware/moza-r5/YYYY-MM-DD/init-standard.json --confirm-low-torque --max-percent 1 --duration-ms 150 --json-out ci/hardware/moza-r5/YYYY-MM-DD/low-torque-proof.json
wheelctl telemetry record --game simhub-bridge --telemetry-source simhub_bridge --live-simhub --port 5555 --out ci/hardware/moza-r5/YYYY-MM-DD/simulator-telemetry-recording.jsonl --session-id simhub-bridge-YYYY-MM-DD --duration-ms 30000
wheelctl moza simulator-telemetry-proof --lane ci/hardware/moza-r5/YYYY-MM-DD --game simhub-bridge --telemetry-source simhub_bridge --recorder-artifact simulator-telemetry-recording.jsonl --duration-ms 30000 --json-out ci/hardware/moza-r5/YYYY-MM-DD/simulator-telemetry-proof.json
wheeld --hardware-lane ci/hardware/moza-r5/YYYY-MM-DD
wheelctl moza simulator-ffb-smoke --lane ci/hardware/moza-r5/YYYY-MM-DD --game simhub-bridge --telemetry-source simhub_bridge --output-log-artifact simulator-ffb-output.jsonl --strategy pidff-bounded-effect --descriptor-trusted --watchdog-timeout-ms 100 --stop-cleared-output --pause-cleared-output --game-exit-cleared-output --json-out ci/hardware/moza-r5/YYYY-MM-DD/simulator-ffb-smoke.json
wheelctl moza pit-house-observation --case closed --evidence-kind process-window-snapshot --evidence-artifact pit-house-evidence-closed.json --evidence "Pit House closed before staged handshake." --json-out ci/hardware/moza-r5/YYYY-MM-DD/pit-house-observation-closed.json
wheelctl moza pit-house-observation --case open-standard --evidence-kind process-window-snapshot --evidence-artifact pit-house-evidence-open-standard.json --evidence "Pit House open and idle while standard mode completed." --json-out ci/hardware/moza-r5/YYYY-MM-DD/pit-house-observation-open-standard.json
wheelctl moza pit-house-observation --case open-direct --evidence-kind process-window-snapshot --evidence-artifact pit-house-evidence-open-direct.json --evidence "Pit House open while direct mode was blocked or required acknowledgement." --json-out ci/hardware/moza-r5/YYYY-MM-DD/pit-house-observation-open-direct.json
wheelctl moza pit-house-observation --case mode-change --evidence-kind process-window-snapshot --evidence-artifact pit-house-evidence-mode-change.json --evidence "Pit House mode change observed during bounded run; output cleared." --json-out ci/hardware/moza-r5/YYYY-MM-DD/pit-house-observation-mode-change.json
wheelctl moza pit-house-observation --case firmware-page --evidence-kind process-window-snapshot --evidence-artifact pit-house-evidence-firmware-page.json --evidence "Pit House firmware/update page open; high-risk tests refused." --json-out ci/hardware/moza-r5/YYYY-MM-DD/pit-house-observation-firmware-page.json
wheelctl moza pit-house-case --lane ci/hardware/moza-r5/YYYY-MM-DD --case closed --observation-artifact pit-house-observation-closed.json --evidence "Pit House closed; staged init remained ready." --json-out ci/hardware/moza-r5/YYYY-MM-DD/pit-house-closed.json
wheelctl moza pit-house-case --lane ci/hardware/moza-r5/YYYY-MM-DD --case open-standard --observation-artifact pit-house-observation-open-standard.json --evidence "Pit House open and idle; standard mode completed without conflict." --json-out ci/hardware/moza-r5/YYYY-MM-DD/pit-house-open-standard.json
wheelctl moza pit-house-case --lane ci/hardware/moza-r5/YYYY-MM-DD --case open-direct --observation-artifact pit-house-observation-open-direct.json --evidence "Direct mode was blocked until explicit operator acknowledgement." --json-out ci/hardware/moza-r5/YYYY-MM-DD/pit-house-direct-blocked.json
wheelctl moza pit-house-case --lane ci/hardware/moza-r5/YYYY-MM-DD --case mode-change --observation-artifact pit-house-observation-mode-change.json --evidence "Mode mismatch was detected and output failed safe." --json-out ci/hardware/moza-r5/YYYY-MM-DD/pit-house-mode-change.json
wheelctl moza pit-house-case --lane ci/hardware/moza-r5/YYYY-MM-DD --case firmware-page --observation-artifact pit-house-observation-firmware-page.json --evidence "Firmware/update page open; high-risk tests refused." --json-out ci/hardware/moza-r5/YYYY-MM-DD/pit-house-firmware-page.json
wheelctl moza pit-house-proof --lane ci/hardware/moza-r5/YYYY-MM-DD --closed-artifact pit-house-closed.json --open-standard-artifact pit-house-open-standard.json --direct-artifact pit-house-direct-blocked.json --mode-change-artifact pit-house-mode-change.json --firmware-page-artifact pit-house-firmware-page.json --shared-control-risk warned --json-out ci/hardware/moza-r5/YYYY-MM-DD/pit-house-coexistence.json
wheelctl moza verify-bundle --lane ci/hardware/moza-r5/YYYY-MM-DD --stage smoke-ready --json-out ci/hardware/moza-r5/YYYY-MM-DD/smoke-ready-verification.json
wheelctl moza promote-manifest --lane ci/hardware/moza-r5/YYYY-MM-DD --stage smoke-ready --json-out ci/hardware/moza-r5/YYYY-MM-DD/manifest-promotion-smoke-ready.json
wheelctl moza audit-lane --lane ci/hardware/moza-r5/YYYY-MM-DD --stage smoke-ready --json-out ci/hardware/moza-r5/YYYY-MM-DD/lane-audit-smoke-ready.json
```

The 1% actuator-profile receipt proves the native PIDFF output rail and cleanup path, but it does not claim visible motion. Before smoke-ready promotion, collect a separate 5% visible-motion receipt that proves native steering delta from the R5 input stream while keeping final PIDFF Stop All cleanup and all high-risk surfaces disabled:

```powershell
wheelctl moza actuator-visible-smoke --device <r5> --lane ci/hardware/moza-r5/YYYY-MM-DD --prior-actuator-proof ci/hardware/moza-r5/YYYY-MM-DD/native-actuator-profile-smoke.json --steering-proof ci/hardware/moza-r5/YYYY-MM-DD/steering-angle-stream-proof.json --profile constant-low-force --strategy pidff-bounded-effect --max-percent 5 --duration-ms 2000 --confirm-actuator-visible --json-out ci/hardware/moza-r5/YYYY-MM-DD/native-actuator-visible-smoke.json
```

For the live R5 V1 lane, the trusted descriptor exposes feature report `0x11` but not feature report `0x03`; `0x03` is an output report in this descriptor and must not be sent by the init stage. The init verifier therefore accepts the R5 V1 mode-only feature write: `0x11` (`11FF0000` for off or `11000000` for standard). Other wheelbase lanes may still require the ordered `0x03` start-input feature write followed by `0x11` when their trusted descriptor and adapter prove that shape. Any high-torque feature report or direct torque output report fails the stage.

Do not run the low-torque command merely because zero-stage verification passed. Generated smoke-ready guidance may suggest the direct `wheelctl moza torque-test --strategy direct-report-0x20` path only when `descriptor.json` proves trusted direct report `0x20` metadata and `zero-torque-proof.json` is a same-lane `direct_report_0x20` proof accepted by the torque-test preflight. If the lane only proves PIDFF Stop All zero output, as the live R5 V1 lane currently does, the verifier must not generate `--explicit-operator-override`.

The PIDFF low-torque path is a separate explicit strategy, not a shortcut around the direct `0x20` gate:

```powershell
wheelctl moza torque-test --device <r5> --lane ci/hardware/moza-r5/YYYY-MM-DD --strategy pidff-bounded-effect --zero-proof ci/hardware/moza-r5/YYYY-MM-DD/zero-torque-proof.json --init-off ci/hardware/moza-r5/YYYY-MM-DD/init-off.json --init-standard ci/hardware/moza-r5/YYYY-MM-DD/init-standard.json --confirm-low-torque --max-percent 1 --duration-ms 150 --json-out ci/hardware/moza-r5/YYYY-MM-DD/low-torque-proof.json
```

Today this PIDFF command validates same-lane PIDFF Stop All zero proof, off/standard init receipts, the exact lane endpoint, descriptor-proven PIDFF Device Control metadata, descriptor metadata for the PIDFF effect reports, and the R5-shaped Set Effect encoder. The live R5 V1 descriptor exposes report `0x01` with a non-generic length, so generic PIDFF packet assumptions are not enough. The implemented R5 V1 writer uses descriptor-proven PIDFF output reports only: R5-shaped Set Effect `0x01`, Set Constant Force `0x05`, Effect Operation `0x0A`, then final Device Control Stop All `0x0C`. A PIDFF receipt must declare `low_torque_strategy: "pidff_bounded_effect"`, prove effect setup explicitly, record bounded nonzero PIDFF writes, avoid direct report `0x20`, and end with PIDFF Stop All cleanup. It cannot satisfy the direct-report verifier path. The `2026-05-13` lane contains the first real bounded PIDFF low-torque receipt; new dated lanes still have no low-torque evidence until the operator runs this command on hardware.

The Pit House and simulator proof commands are the verifier-accepted producers. Their artifact arguments are simple lane-relative paths and must already exist under the lane. For Pit House, create one case artifact for each matrix row before running `pit-house-proof`; the mode-change case must be generated after `simulator-ffb-smoke`, because its source link is the simulator output log record tagged `mode_mismatch`. For simulator telemetry, use `wheelctl telemetry record` to stamp normalized snapshots with recorder provenance before `simulator-telemetry-proof`. For simulator FFB, run it only after telemetry proof and the earlier zero/watchdog/disconnect/init/low-torque receipts exist; the output log must be derived from that telemetry, use the explicit PIDFF strategy on the live R5 V1 path, avoid direct report `0x20`, and end with final Stop All cleanup. Pit House coexistence remains a later smoke-ready promotion gate, not evidence for the first OpenRacing-controlled simulator smoke.

For offline preparation, generate non-claiming starter files first if needed:

```powershell
wheelctl moza receipt-template --kind pit-house --json-out ci/hardware/moza-r5/YYYY-MM-DD/pit-house-coexistence.json
wheelctl moza receipt-template --kind simulator-telemetry --json-out ci/hardware/moza-r5/YYYY-MM-DD/simulator-telemetry-proof.json
wheelctl moza receipt-template --kind simulator-ffb --json-out ci/hardware/moza-r5/YYYY-MM-DD/simulator-ffb-smoke.json
```

These templates have `success=false` and are intentionally rejected by `verify-bundle` until real observations replace the pending fields.
The simulator FFB template exposes the pending prerequisite gate summaries, same-lane prerequisite artifact CRC/timestamp summaries, telemetry session link, and writer timing fields that the smoke verifier later requires. Pit House observation receipts require the named screenshot, video, or process/window snapshot to already exist next to the observation receipt output.

The smoke-ready verifier does not accept placeholder success receipts for Pit House or simulator proof. `pit-house-coexistence.json` must include all five coexistence matrix cases, `template=false`, `evidence_status="observed_on_real_hardware"`, and non-empty evidence plus artifact fields on every case; each referenced case artifact must be JSON, use a simple lane-relative path, match the parent case/result, declare high torque false plus no serial/firmware commands, and include the case-specific safety evidence (`staged_handshake_ready`, standard-mode idle state, direct-mode block/ack requirement, mode-change fail-safe/final-zero evidence, or firmware-page high-risk refusal). Each case artifact must also include `pit_house_observation_artifact`, a separate lane-relative JSON observation file produced by `wheelctl moza pit-house-observation`; that observation must record the case, observed Pit House state, timestamp, operator, a non-notes evidence kind, no HID/FFB writes, and an existing lane-relative `evidence_artifact` such as a screenshot, video, or process/window snapshot. Each case artifact must also link to verifier-checked source evidence with `source_receipt`, `source_gate`, and `source_log`: closed Pit House links to `init-off.json` / `init_off_handshake`, open idle links to `init-standard.json` / `init_standard_handshake`, direct mode links to `low-torque-proof.json` / `low_torque_bounded`, mode-change links to `simulator-ffb-smoke.json` / `simulator_ffb_bounded` plus a `clear_zero` output record tagged `mode_mismatch` and final zero, and firmware/update page links to `support-bundle.json` / `service_status_receipts`. `simulator-telemetry-proof.json` must show telemetry-only operation with `hardware_output_enabled=false`, `no_ffb_writes=true`, normalized snapshots, a recorder artifact, recorder provenance, and no faults; the recorder JSON/JSONL artifact must use a lane-relative path, exist under the lane, contain exactly the claimed snapshot count, include normalized fields (`speed_ms`, `steering_angle`, `throttle`, `brake`, `rpm`, `gear`, `ffb_scalar`) with sequence or timestamp ordering evidence, bind the parent receipt's `duration_ms` through per-record duration fields or timestamp span, and include per-record provenance matching the parent receipt (`recorder_command="wheelctl telemetry record"`, stable recorder session, matching game/source, hardware output disabled, no FFB writes, no serial config, and no firmware/DFU commands). `simulator-ffb-smoke.json` must show an R5 output-capable device record, `hardware_output_enabled=true`, `no_hid_device_opened=false`, `no_ffb_writes=false`, an explicit `output_strategy`, descriptor trust cross-checked against lane `descriptor.json` for the same R5 PID and strategy, `hardware_prerequisites_validated=true` with passing prerequisite gates for zero/watchdog/disconnect/init/low-torque, bounded non-zero output plus zero output counts, an input telemetry artifact/count/session link matching a passing `simulator-telemetry-proof.json`, an output log artifact, high torque false, watchdog active, final zero attempted and sent with the strategy-safe zero payload, `mode_mismatch_cleared_output=true`, stop/pause/game-exit/mode-mismatch output clearing, and `max_output_percent <= 5`; the output JSON/JSONL artifact must use a lane-relative path, exist under the lane, contain exactly the claimed output report count, and contain strategy-specific successful records with `payload_hex`, `report_id`, `report_len`, signed percent/output percent, `bytes_written`, contiguous sequence, monotonic advancing `elapsed_us`, `telemetry_sequence`, `input_ffb_scalar` matching the referenced telemetry snapshot's `ffb_scalar`, HID write metadata (`transport="hid"`, `hid_write_target="output_report"`, `hid_write_attempted=true`), input telemetry link fields matching the telemetry proof, and writer provenance matching the parent receipt (`writer_command` beginning with `wheeld --hardware-lane`, `writer_hardware_lane` or `moza_lane` matching the dated lane, stable writer session, ordered UTC writer start/completion timestamps, R5 device path/product ID, hardware output enabled, HID opened, and FFB writes present). Direct-report smoke requires direct torque `0x20` records and final zero payload `2000000000000000`; PIDFF smoke requires no direct report `0x20`, PIDFF Set Effect `0x01`, Constant Force `0x05`, Effect Operation `0x0A`, ordered Stop All `0x0C` cleanup records tagged with `clear_event: "stop"`, `"pause"`, `"game_exit"`, and `"mode_mismatch"`, and final Stop All payload `0C04`. Pit House and simulator receipts must also declare `no_serial_config_commands=true` and `no_firmware_or_dfu_commands=true`.

Simulator FFB writer provenance must also include `writer_endpoint_selector`, and it must match the exact HID endpoint selector declared by the lane manifest.

`wheelctl moza audit-lane` should run after every `promote-manifest` step and its `lane-audit-*.json` receipts are part of the lane manifest contract. It reruns the requested live bundle verification and checks that the stored verification and manifest-promotion receipts through that stage are present, successful, non-claiming, and backed by matching embedded before/after verification summaries with zero missing artifacts, invalid artifacts, and failed gates. It opens no HID device and sends no reports.

Support bundles can include Moza lane verifier context without touching hardware:

```powershell
wheelctl support-bundle --device <r5> --moza-lane ci/hardware/moza-r5/YYYY-MM-DD --output ci/hardware/moza-r5/YYYY-MM-DD/support-bundle.json
```

Use the top-level `support-bundle --device <r5>` command for the lane artifact so the receipt records the device filter and the Phase 9 checklist command shape. `wheelctl diag support --moza-lane ...` is still useful for ad hoc triage, but it is not sufficient for the smoke-ready service-status gate. The smoke-ready verifier requires `moza-status.json`, `device-status.json`, and `support-bundle.json`; all three must identify the same R5 PID, including the support bundle's top-level `devices[]` entry and service-facing `device_statuses[]` snapshot, keep torque readiness disabled, and declare no FFB, serial configuration, firmware, or DFU commands. The verifier rereads the lane and rejects support-bundle Moza sections that overclaim readiness or artifacts: a bundle may conservatively show an earlier stage or a missing artifact from when it was generated, but it cannot claim a passing readiness flag, lane-audit flag, highest stage, or artifact `pass` state that the current lane cannot prove. The support bundle includes service-facing device status snapshots plus a Moza section with an `artifact_index` for every required lane receipt/capture, including stored verification, manifest-promotion, and lane-audit receipts even when they are still missing, and a diagnostic `readiness` summary with the highest passing stage, the next required stage, the first blocking verifier summary, lane-audit booleans, and `release_ready=false`. Each artifact-index entry must record a consistent path, kind, required stage, existence/validity booleans, and `pass`, `missing`, or `invalid` status. `ready_for_zero_torque` requires the passive verifier, `lane-audit-passive.json`, and at least one implemented descriptor-trusted zero-output strategy; `ready_for_low_torque` requires either the descriptor/direct zero path for `direct_report_0x20` or the descriptor-proven PIDFF bounded-effect path with same-lane PIDFF zero and init receipts; `ready_for_real_hardware_smoke` requires the smoke-ready verifier plus `lane-audit-smoke-ready.json`. Treat it as troubleshooting context only; readiness claims still require the corresponding `verify-bundle`, `promote-manifest`, and `audit-lane` receipts in the lane.

The Moza section summarizes missing artifacts and failed gates across passive, zero, OpenRacing-control, and smoke-ready stages. It is diagnostic context only, not a readiness promotion.

## Hard Rules

- Passive commands do not send FFB output, feature reports, or serial config.
- Verifier-accepted receipts declare no serial config, firmware, or DFU commands.
- Zero proof must pass before any non-zero torque test.
- Zero, watchdog, disconnect, off/standard init, and low-torque receipts must carry `receipt_path` values resolving to the exact dated-lane artifact being verified, plus valid UTC `generated_at_utc` values.
- Low torque requires `--confirm-low-torque`, `--lane`, same-lane real zero/off/standard init receipts before HID initialization, and an explicit strategy. The direct strategy requires trusted descriptor-derived direct report `0x20` metadata plus a same-lane direct-report zero proof accepted by `torque-test`; the PIDFF strategy requires same-lane PIDFF Stop All zero proof, descriptor-proven PIDFF Device Control metadata, descriptor-proven PIDFF effect reports, and the R5-shaped Set Effect encoder before real hardware writes. Explicit `--zero-proof`, `--init-off`, `--init-standard`, and `--descriptor` paths are accepted only when they resolve to the expected dated-lane artifacts. The verifier re-reads the same dated lane's zero/off/standard prerequisite receipts, checks their embedded timestamp/CRC summaries, keeps direct and PIDFF receipt paths separate, and recomputes raw direct-torque payloads from the R5 PID and claimed percent before accepting direct command logs.
- Simulator FFB smoke must carry `prerequisite_artifacts` for zero, watchdog, disconnect, off/standard init, and low torque. Each summary must match the current dated-lane artifact by path, CRC, and timestamp, and every prerequisite timestamp must predate `writer_started_at_utc`.
- If direct report `0x20` remains unproven and the PIDFF bounded-effect frontier is blocked, generated next-commands must stop at read-only readiness guidance. When the PIDFF frontier is clear, generated next-commands may suggest only the explicit `--strategy pidff-bounded-effect` low-torque command. `--explicit-operator-override` is a separate deliberate operator decision for a later manual path, never generated guidance, and high torque remains disabled.
- High torque stays false for this lane.
- Pit House coexistence is tested separately.
- Release readiness stays false.
