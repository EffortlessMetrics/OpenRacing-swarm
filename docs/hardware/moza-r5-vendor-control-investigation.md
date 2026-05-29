# Moza R5 Vendor-Control Investigation

Status: read-only protocol research; no output authorization
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
enable/control research. The current checked-in summaries now expose the Pit
House USB CDC stream as length-prefixed `0x7E` serial-frame candidates, but they
do not decode an approved semantic enable/mode command. The current registry
comparison finds only one read-only status tuple match and no write-like
candidate. The current frequency review ranks the highest-count unknown
commanded tuples as `0x5A/0x1B/0x00` (1,896 frames),
`0x5D/0x1B/0x01` (1,894 frames), and `0x25/0x19/0x01`,
`0x25/0x19/0x02`, and `0x25/0x19/0x03` (624 frames each). Artifact-index and
bench-wizard surface the same queue as no-output `vendor_protocol_decode_priority`
navigation with bounded sample frame examples for the top unknown tuples. The
protocol crate now validates those sample frames as observed wire-shape fixtures
while keeping them unknown to the semantic registry. The review also groups the
same samples by scenario packet ordinal, preserving 11 packet groups, three
full-packet patterns, and four repeated contiguous motifs. The recurring
patterns are the `0x5A/0x1B/0x00` then `0x5D/0x1B/0x01` pair, the ordered
`0x25/0x19/0x02`, `0x25/0x19/0x03`, `0x25/0x19/0x01` triad, and one combined
five-frame packet that contains both. The review also records payload-shape
morphology for those top unknown samples: all are checksum-valid
unknown-commanded samples, with empty `0x5A/0x1B/0x00` payloads and zero-filled
`0000` payloads for the `0x5D` and `0x25` samples. The review now preserves
pattern-only semantic hypotheses for that same queue:
`0x5A/0x1B/*` and `0x5D/0x1B/*` are
`session_or_status_keepalive_candidate`, while `0x25/0x19/*` is
`base_status_or_mode_poll_candidate`. Those labels are low-confidence decode
questions, not semantic commands, registry entries, or sendable output
candidates. The current review now turns those two hypothesis groups into
explicit mode/enable candidate questions while keeping them non-sendable:
`0x25/0x19/*` asks about `status_query`, `standard_pidff_mode_enable`, and
`game_control_mode_select`, while `0x5A/0x1B/*` plus `0x5D/0x1B/*` asks about
`authority_keepalive` and `volatile_ffb_session_enable`. Every candidate remains
`unknown_do_not_send`, with no semantic decode, registry promotion, output
sendability, authorization, hardware output, native-control, native-visible, or
smoke-ready claim. The fake transport now records containment for those
questions: representative frames are observed as `unknown_do_not_send`
candidate evidence, and the command/send path still rejects the same frames as
unknown commands. The live read-only hardware status/mode matrix has now been
recorded at
`ci/hardware/moza-r5/2026-05-13/vendor-status-mode-matrix.json`, with the fresh
observe-only precondition doctor at
`ci/hardware/moza-r5/2026-05-13/vendor-status-mode-matrix-hardware-doctor.json`.
The probe verified COM4 as the R5 `0x346E:0x0004` serial/CDC interface and sent
only registry-approved read-only status queries. It failed closed with zero
decoded responses, nine failed responses, and
`real_hardware_status_evidence=false`, so unknown mode/safety status still
blocks any authority plan. The next native-path step is no-output diagnosis of
that read-only serial response stream, not an output attempt. The current
receipt also records the correlation plan explicitly: two non-sendable
targets, completed observations in the three Pit House scenarios, and
remaining no-output correlation gaps in SimHub and simulator scenarios. The
first bounded setting-change capture attempt remains
classified as low-yield/incomplete: 355 bytes, six packets, zero Moza
`0x346E:0x0004` matches, and restore status `not reported`. The repeat capture
used the corrected selector `\\.\USBPcap2 --devices 4` and is now recorded as
accepted passive correlation evidence only: 100,492 Moza-matched packets over
113.446197 seconds, host-to-device vendor candidates `0x7E` and `0x80`, and
operator notes for the KS wheel top-left front LED changing default teal -> red
-> default teal. The raw pcap remains local, and the bounded helper's final
receipt-write file-lock failure is tracked as tooling follow-up only.

## Current Artifacts

The following passive sniff artifacts are committed:

| Scenario | Plan artifact | Current status |
| --- | --- | --- |
| Pit House open idle | `ci/hardware/sniff/moza-r5/2026-05-13/pit-house-open-idle/sniff-plan.json` | Summary recorded, non-claiming |
| Pit House setting change | `ci/hardware/sniff/moza-r5/2026-05-13/pit-house-setting-change/sniff-plan.json`; `ci/hardware/sniff/moza-r5/2026-05-13/pit-house-setting-change/low-yield-capture-classification.json`; `ci/hardware/sniff/moza-r5/2026-05-13/pit-house-setting-change/sniff-receipt.json`; `ci/hardware/sniff/moza-r5/2026-05-13/pit-house-setting-change/sniff-summary.json`; `ci/hardware/sniff/moza-r5/2026-05-13/pit-house-setting-change/sniff-bundle-manifest.json`; `ci/hardware/sniff/moza-r5/2026-05-13/pit-house-setting-change/operator-notes.md` | Summary recorded, non-claiming; prior low-yield attempt retained |
| SimHub open idle | `ci/hardware/sniff/moza-r5/2026-05-13/simhub-open-idle/sniff-plan.json` | Plan only |
| SimHub output session | `ci/hardware/sniff/moza-r5/2026-05-13/simhub-output-session/sniff-plan.json` | Plan only |
| Simulator session start/stop | `ci/hardware/sniff/moza-r5/2026-05-13/simulator-session-start-stop/sniff-plan.json` | Plan only |
| Pit House full controls | `ci/hardware/sniff/moza-r5/2026-05-13/pit-house-full-controls/sniff-plan.json` | Summary recorded, non-claiming |

`wheelctl moza artifact-index` reports recorded scenarios as non-claiming
receipt/summary evidence and leaves missing scenarios `partial_or_unaccepted`
or `low_yield_incomplete` until matching successful `sniff-receipt.json` and
`sniff-summary.json` artifacts exist.

The committed Pit House summaries preserve 3,246 extracted host-to-device USB
CDC payload packets and parse them into 7,863 candidate `0x7E` serial frames.
All parsed candidate frames have valid checksums, zero checksum-invalid frames,
and no frame-shape decode gap. `vendor-protocol-evidence-review.json` compares
30 distinct passive tuple IDs to `fixtures/moza/r5/vendor-command-registry.json`.
Only `0x28/0x13/0x02` (`base_gain_get_overall_strength`) matches the registry,
and it is read-only status evidence. The review records 12 commandless tuple IDs,
17 unknown commanded tuple IDs, zero known write-like tuple matches, and
`unknown_tuple_risk_class=unknown_do_not_send`. It also records per-scenario
tuple frequency and ranks the top unknown commanded tuples as
`0x5A/0x1B/0x00`, `0x5D/0x1B/0x01`, `0x25/0x19/0x01`,
`0x25/0x19/0x02`, and `0x25/0x19/0x03`; artifact-index and bench-wizard render
those top tuples and representative sample frames in the Vendor Authority
Handoff section. The tuple IDs and sample frames remain protocol-shape,
registry-coverage, frequency-prioritization, and fixture evidence only. The
protocol crate observed-frame decoder accepts the checksum-valid shape of those
samples, while the semantic fixture decoder still rejects them as unknown
commands. The same sample regression preserves packet-local pair, triad, and
combined five-frame group morphology plus empty/zero-filled payload-shape hints
as observed evidence only. It also records low-confidence pattern-only semantic
hypotheses for those samples while keeping every tuple `unknown_commanded`,
non-sendable, and ineligible for registry promotion. The review also preserves
the two remaining payload export gaps as packet/frame locators, one in
`pit-house-open-idle` and one in `pit-house-full-controls`, with
`payload_extracted=false` and no sendability or output claim. No tuple,
hypothesis, or residual packet is sendable without a future semantic decode,
reviewed plan, fresh bench clear, and exact authorization.
The same review now records `decode_candidate_semantic_correlation_plan` as
capture navigation only. It now treats `pit-house-setting-change` as a completed
passive correlation scenario and requires non-claiming `sniff-receipt.json` and
`sniff-summary.json` artifacts for the remaining SimHub and simulator
correlation scenarios. It keeps `planned_next_output.allowed=false` and does not
satisfy native-visible, smoke-ready, coexistence, simulator, or release-ready
gates.

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
and read-only matrix state, the wizard records that the PIDFF comparison is
present and emits no output command. Continue with no-output diagnosis of the
read-only serial response framing or the remaining passive capture scenarios
before proposing any further hardware output family.

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
