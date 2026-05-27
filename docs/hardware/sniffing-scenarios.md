# Passive USB Sniffing Scenarios

This document names the scenario taxonomy for passive USB sniff evidence.
Scenarios describe what external behavior was observed; they do not create
OpenRacing readiness claims.

Future CLI implementations should encode these names as value-enum choices:

```text
enumeration
vendor-app-closed-idle
pit-house-open-idle
pit-house-setting-change
pit-house-firmware-page-observed
simhub-open-idle
simhub-device-detect
simhub-output-session
simulator-session-start-stop
custom
```

## Scenario Table

| Scenario | What it learns | Special warning |
|----------|----------------|-----------------|
| `enumeration` | Descriptor, interface, and endpoint layout from host-side USB traffic | Not native output evidence |
| `vendor-app-closed-idle` | Baseline traffic when the vendor app is not active | Does not prove coexistence |
| `pit-house-open-idle` | Pit House discovery, polling, and idle traffic | Do not open firmware or update pages |
| `pit-house-setting-change` | Feature or output behavior around one explicit setting change | Record the exact setting changed and restore it |
| `pit-house-firmware-page-observed` | Unsafe page-state detection for support triage | Observation only; do not update firmware |
| `simhub-open-idle` | SimHub discovery and idle polling | No OpenRacing dependency |
| `simhub-device-detect` | SimHub detection and device classification traffic | Not native OpenRacing evidence |
| `simhub-output-session` | External output pattern from SimHub | External output only |
| `simulator-session-start-stop` | Simulator lifecycle traffic through an external app or bridge | Not OpenRacing FFB proof |
| `custom` | Operator-described trace outside the named cases | Must include evidence text |

## Scenario Rules

All scenarios share the same safety boundary:

- sniffing observes host-side URBs only
- OpenRacing does not send hardware output
- OpenRacing does not open HID output or feature-report paths
- external apps may send output and that remains external evidence only
- serial configuration, firmware, and DFU stay out of scope
- raw `.pcapng` is not committed by default

For `pit-house-setting-change`, the operator evidence must name the exact
setting, the starting value, the ending value, and an affirmative restore
status. A tiny or zero-match capture, or notes with restore status such as
`not reported`, is low-yield/incomplete evidence and must not complete the
scenario.

The accepted 2026-05-27 repeat setting-change capture used
`\\.\USBPcap2 --devices 4`, not the stale `--devices 3` selector from the
low-yield attempt. Its derived artifacts record the KS wheel top-left front LED
change from default teal to red and back to default teal, with no
firmware/update/DFU page or prompt observed. This is accepted passive
correlation evidence only; it is not semantic decode, sendability,
hardware-output, native-visible, smoke-ready, or release-ready evidence.

For `pit-house-firmware-page-observed`, the operator may observe that the page
exists or that the app attempted enumeration. The operator must not start an
update, accept a firmware prompt, enter DFU, or change firmware state.

For `simhub-output-session` and `simulator-session-start-stop`, any observed
output reports are external-app or simulator traffic. They can inform protocol
research, but they must not be described as OpenRacing FFB or native control.

For `custom`, the operator evidence must explain why a named scenario did not
fit and must preserve the same non-claiming readiness fields required by the
schemas.
