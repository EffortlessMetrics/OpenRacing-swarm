# Passive USB Sniffing

Passive USB sniffing is a hardware evidence workflow for protocol research and
support triage. It observes USB traffic caused by the operating system, vendor
applications, simulators, or bridges. It is not an OpenRacing runtime mode and
it is not OpenRacing hardware output.

Sniffing can inform protocol design. Sniffing cannot satisfy native
OpenRacing readiness gates. Sniff artifacts must not advance
`native-response-ready`, `native-visible-ready`, `smoke-ready`, or
`release-ready`.

## Capture Layer

Software USB capture records USB Request Blocks as seen by the host USB stack.
It does not record the literal physical bus packets that a hardware analyzer
would see. This is the useful layer for external-application protocol
observation because it shows the transfers that Windows, Linux, Pit House,
SimHub, a simulator, or a bridge submits to or receives from the device.

Use the platform capture stack directly:

- Windows: USBPcap with Wireshark or `tshark`.
- Linux: `usbmon` with Wireshark or `tshark`.

USBPcap or Wireshark installation is an intentional bench-machine change. Make
that change only when the operator accepts it for the bench host. Do not use
Zadig, driver replacement, or WinUSB conversion as part of this workflow.

Reference: [Wireshark USB capture setup](https://wiki.wireshark.org/CaptureSetup/USB).

## Non-Claiming Doctrine

Every passive sniff artifact must carry this distinction:

```json
{
  "evidence_status": "passive_external_usb_observation",
  "native_control_evidence": false,
  "satisfies_native_response_ready": false,
  "satisfies_native_visible_ready": false,
  "satisfies_smoke_ready": false,
  "satisfies_release_ready": false,
  "openracing_hardware_output": false,
  "external_app_may_have_sent_output": true
}
```

`external_app_may_have_sent_output` is intentional. If Pit House, SimHub, a
simulator, or another external application sends an output report during a
capture, the sniff receipt should record that possibility without implying that
OpenRacing sent output.

The readiness fields must also appear under `readiness_claims`:

```json
{
  "readiness_claims": {
    "satisfies_native_response_ready": false,
    "satisfies_native_visible_ready": false,
    "satisfies_smoke_ready": false,
    "satisfies_release_ready": false
  }
}
```

## Forbidden Actions

The passive sniffing workflow forbids:

- installing Zadig
- replacing the HID driver
- switching the device to WinUSB
- running OpenRacing output commands
- sending OpenRacing HID output reports
- sending OpenRacing HID feature reports
- touching serial configuration
- opening firmware update flows
- running firmware or DFU tools

If a scenario needs vendor-app output behavior for protocol research, let the
vendor app produce that traffic and record it as external observation only. Do
not run OpenRacing output, torque, controlled-angle, actuator-visible,
simulator-FFB, serial, firmware, or DFU commands as part of sniffing.

## Artifact Types

Passive sniffing uses four non-claiming artifact contracts:

```text
ci/hardware/sniffing/sniff-plan.schema.json
ci/hardware/sniffing/sniff-receipt.schema.json
ci/hardware/sniffing/sniff-summary.schema.json
ci/hardware/sniffing/sniff-bundle-manifest.schema.json
```

Planned command names are:

```text
wheelctl hardware sniff-plan
wheelctl hardware sniff-receipt
wheelctl hardware sniff-summary
wheelctl hardware sniff-bundle
```

These commands are not implemented by this specification PR. The schemas define
the evidence shape that later command work must produce.

## Local And Committed Paths

Use `target/sniff/<scenario>/` for local scratch output:

```text
target/sniff/pit-house-open-idle/
  sniff-plan.json
  sniff-plan.md
  capture.pcapng
  sniff-receipt.json
  sniff-summary.json
  sniff-summary.md
  openracing-sniff-bundle.zip
```

For committed support evidence, use summary artifacts by default:

```text
ci/hardware/sniff/moza-r5/2026-05-17/pit-house-open-idle/
  sniff-plan.json
  sniff-receipt.json
  sniff-summary.json
  operator-notes.md
```

Do not commit raw `.pcapng` files by default. Include a raw capture only after
explicit review confirms all of the following:

- the raw capture is needed
- the operator consents
- the file size is reasonable
- payload sensitivity has been reviewed

## Bundle Policy

The default sniff bundle excludes raw `.pcapng` data and includes only compact
JSON, Markdown, hashes, and operator notes:

```text
openracing-sniff-bundle/
  README.md
  sniff-plan.json
  sniff-receipt.json
  sniff-summary.json
  operator-notes.md
  pcapng-sha256.txt
```

`capture.pcapng` belongs in the bundle only when an explicit reviewed
`--include-pcapng` style option is used by a future implementation.

## Readiness Interaction

`wheelctl moza verify-bundle` and other readiness gates must not accept sniff
artifacts as lane readiness artifacts. Sniffing is optional protocol research
and support evidence. It is not a shortcut for native response, native visible
motion, smoke readiness, simulator FFB proof, or release readiness.
