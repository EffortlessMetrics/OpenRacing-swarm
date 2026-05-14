# OpenRacing Capture Utility Design

## Overview

The `openracing-capture` utility is a standalone tool designed to democratize the reverse engineering of racing wheels. Instead of requiring users to be experts in Wireshark or USB protocols, this tool gamifies the process of mapping device inputs and capturing initialization sequences.

## Architecture

The tool is split into two distinct modes of operation: **Mapping** (Safe, Easy) and **Sniffing** (Advanced, Admin-only).

### 1. Mapper Mode (Input Mapping)
* **Goal**: Identify which bit in the HID report corresponds to which physical button/axis.
* **Tech Stack**: `hidapi` (Cross-platform HID access).
* **Workflow**:
    1.  **Detection**: List all connected HID devices and let user select the wheel.
    2.  **Baseline**: Read 50 frames of "hands off" data to establish a baseline.
    3.  **Prompt & Detect**:
        *   "Please turn the wheel 90 degrees right." -> Detect largest changing `u16` or `i16`.
        *   "Please press the Throttle." -> Detect axis change.
        *   "Please press Button A." -> Detect bit toggle.
    4.  **Verification**: Ask user to press the detected input again to confirm.
    5.  **Output**: Generate `device_map.json`.

### 2. Sniffer Mode (Protocol Capture)
* **Goal**: Capture the "Magic Bytes" sent by the OEM driver to initialize the wheel (enable FFB).
* **Tech Stack**:
    *   **Windows**: Wrapper around `USBPcap` (requires installation).
    *   **Linux**: `usbmon` (kernel module).
* **Workflow**:
    1.  **Setup**: Instruct user to close all wheel software.
    2.  **Start Capture**: Tool starts listening to the specific USB Bus/Device.
    3.  **Trigger**: Instruct user to "Open the OEM Driver Software (e.g., Pit House)".
    4.  **Capture**: Record the first 5 seconds of `OUT` packets (Host -> Device).
    5.  **Filter**: automatically strip standard Windows descriptors requests, isolating the vendor-specific "Magic Bytes".
    6.  **Output**: Append initialization sequence to `device_map.json`.

## Device Definition Schema (`device.json`)

```json
{
  "info": {
    "vendor_id": "0x1234",
    "product_id": "0x5678",
    "name": "Moza R9",
    "manufacturer": "Moza Racing"
  },
  "protocol": {
    "init_sequence": [
      { "report_id": 0x01, "payload": "AA55..." }
    ],
    "ffb": {
      "type": "pidff",
      "quirks": ["shift_byte", "reverse_force"]
    }
  },
  "inputs": {
    "steering": {
      "type": "axis",
      "byte_offset": 0,
      "data_type": "u16_le",
      "min": 0,
      "max": 65535
    },
    "throttle": { "byte_offset": 4, "data_type": "u8" },
    "buttons": [
      { "name": "A", "byte_offset": 8, "bit_mask": 0x01 },
      { "name": "B", "byte_offset": 8, "bit_mask": 0x02 }
    ]
  }
}
```

## Implementation Plan

### Phase 1: Mapper CLI (MVP)
- Implement `openracing-mapper` binary.
- Dependencies: `hidapi`, `crossterm` (for UI), `serde_json`.
- Support: Windows & Linux.
- Deliverable: Tool that produces valid JSON for Button/Axis mapping.

### Phase 1b: Moza HBP capture recipe

- Add a guided Moza section to capture utility:
  - device selection for standalone Moza VID/PID (`0x346E:0x0022`) and Moza wheelbase products,
  - mode selector:
    - **USB mode** (HBP directly connected),
    - **Wheelbase mode** (HBP through wheelbase).
- USB mode handbrake procedure:
  1. Capture 25 frames at rest (baseline `uint16_t` axis).
  2. Prompt operator: “Move handbrake slowly from rest to full and back.”
  3. Record candidate bytes where the high bit transitions.
  4. Offer second confirmation sweep against baseline ± hysteresis window.
- Wheelbase mode handbrake procedure:
  1. Connect HBP to wheelbase RJ45 path.
  2. Verify wheelbase report ID and fields include handbrake bytes.
  3. Record baseline + sweep for at least 2 full travel cycles.
- Button-mode capture:
  1. Put HBP into button mode.
  2. Toggle on/off with a known input pattern.
  3. Confirm as bitfield (`ButtonMode=on`) or axis threshold profile.

### Phase 1c: KS capture and map workflow (capture-driven, mode-aware)

KS has mode-sensitive control semantics and should not be hard-coded. Add a dedicated workflow branch:

#### A) Topology discovery

1. Detect one of:
   - Moza wheelbase VID/PID with KS present via user confirmation,
   - Universal Hub VID/PID with a selected hub-port profile.
2. Log explicit topology decision:
   - `wheelbase-aggregated`,
   - `universal-hub`.
3. Persist topology in `capture_notes.json` with firmware/build + HID signature summary.

#### B) Wheelbase workflow (aggregated)

1. Capture 60s baseline with KS disconnected/unmoved.
2. Walk controls in deterministic script:
   - clutch axes to both extremes in combined mode,
   - left/right clutch axes independently if firmware exposes them,
   - clutch buttons if button mode is enabled,
   - joystick directions, joystick button mode,
   - each rotary as press/release and dial rotation.
3. Capture report bytes and infer `KsReportMap`:
   - buttons bitmap span,
   - clutch encoders / clutch button bits,
   - rotary source bytes,
   - joystick source.

#### C) Universal Hub workflow

1. Validate which USB interface carries wheel input (if multiple interfaces are visible).
2. Capture `start` + `stop` to confirm no duplicate interfaces (and which interface terminates feature-like writes).
3. Run the same scripted action set and compare with wheelbase traces for:
   - identical control semantics in same KS profile,
   - reduced/failing controls in firmware-limited revisions.
4. If wheel payload is absent, mark device signature as `fsr-only` and fail the KS mode path with user-facing diagnostic.

#### D) Artifact set for each topology/mode

- `capture_notes.json`:
  - topology,
  - product/firmware strings,
  - report ID map,
  - ks mode hypothesis log.
- `descriptor.bin` (raw, per interface),
- `device_map.json` (KS offsets/bitmasks + mode hints),
- `ks_input_trace.bin` (timestamped raw input stream),
- `ks_init_bytes.bin` (if any OUT traffic is detected for mode toggles),
- `ks_golden_snapshots.json` (expected `KsReportSnapshot` per scripted actions),
- `ks_pairing_notes.txt` (indicator behavior, pairing retries, known cable caveats).

### Phase 2: Sniffer Integration
- Integrate `pcap` crate.
- Add admin-check logic.
- Deliverable: Tool can capture Init packets.

### Artifacts produced for Moza HBP

- `device_map.json`
- `hbp_usb_baseline.bin`
- `hbp_usb_sweep.bin`
- `hbp_button_mode.bin`
- `hbp_wheelbase_baseline.bin`
- `hbp_wheelbase_sweep.bin`
- `capture_notes.json` (topology + report IDs + axis offsets + button mapping assumptions)

### Phase 3: Community Platform
- GitHub Actions workflow to validate submitted JSONs.
- "Device Library" registry in the main `OpenRacing` repo.

## Moza R5 identity capture (deterministic matrix capture)

The repository now includes a small cross-platform capture utility:

- `crates/openracing-capture-ids`
- crate name: `openracing-capture-ids`

This utility captures the full HID interface matrix for Moza devices and emits JSON suitable for stable identity baselines.

For each interface it emits:

- `vendor_id` / `product_id` (hex and decimal)
- `interface_number` (when present)
- `usage_page` / `usage`
- `path` (`/dev/hidrawX` on Linux, Windows HID path on Windows)
- Linux report descriptor summary (`len`, `crc32`, optional hex via `--descriptor-hex`)

Run examples:

```bash
cargo run -p openracing-capture-ids --release -- --vid 0x346E --descriptor-hex > moza_ids_linux.json
```

```powershell
cargo run -p openracing-capture-ids --release -- --vid 0x346E > moza_ids_windows.json
```

Use the captured output to:

- select the correct output/FFB interface by usage + descriptor fingerprint,
- gate direct torque behind known descriptors,
- track layout drift across firmware versions.

When present, the `crc32` field from this tool maps directly to `OPENRACING_MOZA_DESCRIPTOR_CRC32_ALLOWLIST` in the lower-level runtime. Windows hidapi enumeration may not expose that raw descriptor fingerprint; receipt-backed hardware lanes should instead import exported descriptor bytes through `wheelctl moza descriptor --report-descriptor-hex-file` or `--report-descriptor-bin-file` so the lane verifier can recompute the descriptor CRC and parsed report metadata. See [Moza Protocol: Signature Fingerprinting and Safe Arming Policy](../protocols/MOZA_PROTOCOL.md#signature-fingerprinting-and-safe-arming-policy) for the full capture to descriptor receipt to arm workflow.
