# Moza R5 Vendor-Control Investigation

Status: plan-only no-output protocol research
Lane: `ci/hardware/moza-r5/2026-05-13`
Sniff plan root: `ci/hardware/sniff/moza-r5/2026-05-13`
Claim ceiling: protocol research/support navigation only

## Why This Exists

Three bounded standard-PIDFF-family controlled-angle attempts all stayed in the
same undertravel band, about `0.181277` degrees. The standard PIDFF path is now
classified as ineffective in the current R5 mode by
`native-pidff-standard-path-diagnosis.json`.

The later exact-authority experiment did not unlock the standard PIDFF path.
`vendor-authority-attempt.json` records one consumed `estop_set_ffb` frame, and
`vendor-post-authority-pidff-response.json` classifies the comparable
post-authority response as `post_authority_pidff_response_regressed`: baseline
`0.18127718013275285` degrees versus post-authority
`0.032959487296864154` degrees. That result keeps this investigation on the
no-output protocol rail.

The next native-visible investigation path is no-output Moza vendor-specific
enable/control research. The next step is to review the already captured
passive summaries and fill remaining no-output capture gaps before decoding
report IDs or proposing any vendor-control output path.

## Current Artifacts

The following passive sniff artifacts are committed:

| Scenario | Plan artifact | Current status |
| --- | --- | --- |
| Pit House open idle | `ci/hardware/sniff/moza-r5/2026-05-13/pit-house-open-idle/sniff-plan.json` | Summary recorded, non-claiming |
| Pit House setting change | `ci/hardware/sniff/moza-r5/2026-05-13/pit-house-setting-change/sniff-plan.json` | Plan only |
| SimHub open idle | `ci/hardware/sniff/moza-r5/2026-05-13/simhub-open-idle/sniff-plan.json` | Plan only |
| SimHub output session | `ci/hardware/sniff/moza-r5/2026-05-13/simhub-output-session/sniff-plan.json` | Plan only |
| Simulator session start/stop | `ci/hardware/sniff/moza-r5/2026-05-13/simulator-session-start-stop/sniff-plan.json` | Plan only |
| Pit House full controls | `ci/hardware/sniff/moza-r5/2026-05-13/pit-house-full-controls/sniff-plan.json` | Summary recorded, non-claiming |

`wheelctl moza artifact-index` reports recorded scenarios as non-claiming
receipt/summary evidence and leaves missing scenarios `partial_or_unaccepted`
until matching `sniff-receipt.json` and `sniff-summary.json` artifacts exist.

## Boundaries

These plans do not authorize hardware output. They do not create pcap receipts,
sniff summaries, native-visible evidence, smoke-ready evidence, or vendor-control
implementation permission.

Forbidden while following these plans:

- installing Zadig
- replacing the HID driver
- switching the device to WinUSB
- running OpenRacing output commands
- sending OpenRacing HID output reports
- sending OpenRacing HID feature reports
- touching serial configuration
- opening firmware update flows
- running firmware or DFU tools

Vendor apps may produce external traffic during a capture. That traffic must be
recorded as passive external observation only, not OpenRacing output.

## Next Evidence Needed

Run `wheelctl moza bench-wizard --lane ci/hardware/moza-r5/2026-05-13` to get
the current command-bound no-output handoff. With the current post-authority
state, the wizard records that the PIDFF comparison is present and emits no
output command. Continue with no-output protocol review or the remaining
passive capture scenarios before proposing any further hardware output family.

For each planned scenario:

1. Capture host-side USB URBs with USBPcap/Wireshark or `tshark`.
2. Save the local `.pcapng` outside the committed lane by default.
3. Generate `sniff-receipt.json` from the plan and pcap hash.
4. Generate `sniff-summary.json` from the receipt and pcap.
5. Decode vendor reports, map report IDs, and identify any enable, gain, or
   mode handshakes.

Do not commit raw `.pcapng` files unless a separate review approves the raw
capture, size, sensitivity, and operator consent.

Only after summaries identify a defensible vendor-specific enable/control path
should a reviewed vendor-control plan be designed. That later plan would still
need fresh command-bound bench-clear evidence and a new exact authorization
before any output attempt.
