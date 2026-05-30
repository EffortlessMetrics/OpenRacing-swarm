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
blocked any authority plan. The follow-up no-output diagnosis is recorded at
`ci/hardware/moza-r5/2026-05-13/vendor-status-framing-diagnosis.json`: the
stored readback frames are dominated by repeated tuple `0x0E/0x71/0x05` ASCII
`NRFloss`/`recvGap` diagnostic stream frames, with one desynchronized partial
frame, rather than registry status/mode replies.

The read-only demux follow-up is recorded at
`ci/hardware/moza-r5/2026-05-13/vendor-status-mode-matrix-demux.json`, with its
fresh observe-only precondition doctor at
`ci/hardware/moza-r5/2026-05-13/vendor-status-mode-matrix-demux-hardware-doctor.json`.
The demux skips diagnostic stream frames and accepts the observed response-side
group/device tuple transform, decoding seven registry status replies. The
receipt still failed closed because `estop_get_ffb` and
`main_misc_get_ffb_status` did not decode; unknown safety/mode state still
blocks any authority plan. The next native-path blocker is read-only status
reply correlation for those two commands, not an output attempt. The current
receipt also records the correlation plan explicitly: two non-sendable
targets, completed observations in the three Pit House scenarios, and
remaining no-output correlation gaps in SimHub and simulator scenarios. The
targeted read-only correlation follow-up is recorded at
`ci/hardware/moza-r5/2026-05-13/vendor-status-reply-correlation-targeted.json`
with its fresh observe-only doctor at
`ci/hardware/moza-r5/2026-05-13/vendor-status-reply-correlation-hardware-doctor.json`
and derived offline diagnosis at
`ci/hardware/moza-r5/2026-05-13/vendor-status-reply-correlation-diagnosis.json`.
It selected only `estop_get_ffb` and `main_misc_get_ffb_status`, sent two
registry-approved read-only queries, decoded zero authority-state replies, and
kept all output/readiness claims false. The diagnosis classifies 23 of 24
scanned frames as diagnostic telemetry and records one response-like
group/device command mismatch, `0xA1/0x21/0x4D`, while requesting
`main_misc_get_ffb_status` `0x21/0x12/0x07`. That mismatch is correlation
evidence only; it does not promote semantic decode, registry sendability,
authorization, native control, or native-visible motion. The
extended read-only scan follow-up is recorded at
`ci/hardware/moza-r5/2026-05-13/vendor-status-extended-scan-targeted.json`
with its fresh observe-only doctor at
`ci/hardware/moza-r5/2026-05-13/vendor-status-extended-scan-hardware-doctor.json`
and derived diagnosis at
`ci/hardware/moza-r5/2026-05-13/vendor-status-extended-scan-diagnosis.json`.
It added only a bounded scan-window option, selected the same two read-only
commands, used `--max-response-frames-per-query 64`, decoded zero
authority-state replies, and now classifies `7E00A1214D` as a checksum-valid
zero-length response-like frame for `0xA1/0x21/no_command`. That removes shallow
scan-window depth as the immediate explanation. The latest diagnosis narrows the
frame to ACK-only/no-payload correlation evidence, not decoded status evidence,
so mode and safety remain unknown until a payload-bearing status reply is
decoded or the authority-status endpoint/command IDs are corrected.
Authorization, PIDFF rerun, and motion remain blocked. The ACK-only live rerun
is recorded at
`ci/hardware/moza-r5/2026-05-13/vendor-status-ack-only-correlation-targeted.json`
with its fresh observe-only doctor at
`ci/hardware/moza-r5/2026-05-13/vendor-status-ack-only-correlation-hardware-doctor.json`
and derived diagnosis at
`ci/hardware/moza-r5/2026-05-13/vendor-status-ack-only-correlation-diagnosis.json`.
It selected the same two read-only commands, opened no HID path, sent no
output/configuration/firmware/PIDFF command, decoded zero authority-state
replies, and reproduced the same ACK-only/no-payload candidate. The
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

The endpoint-candidate receipt now also preserves five passive command-id
`0x07` analog tuples from the same protocol evidence review:
`0x40/0x17/0x07`, `0x28/0x13/0x07`, `0x23/0x19/0x07`,
`0x3F/0x17/0x07`, and `0x5B/0x1B/0x07`. The fake transport records
representative zero-payload containment frames for those analogs and verifies
they remain `unknown_do_not_send`, not payload-status matches, not
read-only-probe-allowed, not sendable, and rejected by the command send path.
They narrow the authority-status endpoint search only; they are not semantic
decode proof, registry promotion, live probe inputs, authorization inputs, or
native motion evidence.

`vendor-status-authority-source-gap.json` now records the current native-path
blocker as a source gap, not a motion-controller or force-tuning gap. The
checked-in evidence still lacks a payload-bearing authority-state status
endpoint or equivalent reviewed status source. Current registry
authority-status queries are ACK/debug-only, the passive command-id `0x07`
analogs and mode/enable groups remain `unknown_do_not_send`, and the checked-in
passive review now extracts 18 checksum-valid device-to-host serial frame
samples from stored response-side report `0x7E` payload samples.

`vendor-status-response-source-correlation.json` now records the follow-up
correlation. The stored response samples correlate to the unknown passive
question groups as sample-scoped response-shape evidence:

```text
0x25/0x19/* -> 0xA5/0x91/*
0x5A/0x1B/0x00 -> 0xDA/0xB1/0x00
0x5D/0x1B/0x01 -> 0xDD/0xB1/0x01
```

The receipt still does not identify a payload-bearing authority-state status
source. It records that the expected registry authority-status response tuples
`0xA1/0x21/0x07` and `0xC6/0xC1/0x01` are absent from passive response samples,
and that the command-id `0x07` analogs have no response-side sample match. The
correlated response semantic fixture review is now recorded at
`vendor-status-response-semantic-fixtures.json`: it decodes the correlated
passive response fixture shapes for `0xA5/0x91/*`, `0xDA/0xB1/0x00`, and
`0xDD/0xB1/0x01`, but all 11 correlated samples have zero-filled/static
payloads. That keeps `payload_variation_observed=false`,
`payload_bearing_authority_state_source_found=false`,
`corrected_read_only_probe_ready=false`, `live_read_only_probe_allowed=false`,
and `motion_attempt_allowed=false`.

`vendor-status-payload-source-candidates.json` records the separate
payload-bearing device-to-host samples in the checked-in setting-change
evidence. It preserves four nonzero `0x8E` samples:

```text
0x8E/0x21/0x00
0x8E/0x31/0x00
0x8E/0x71/0x00
0x8E/0x91/0x00
```

Those samples are useful status-source questions, but they remain
`unknown_do_not_send`. They are not same-tuple payload variation, not timing
correlation, not semantic decode, not registry promotion, not a reviewed
authority-state source, and not read-only probe or output eligibility.

`vendor-status-payload-source-semantic-review.json` now adds fixture-backed
decoder coverage for those same four `0x8E` samples under the passive review
group `passive_payload_bearing_status_source_0x8e`. The review preserves their
nonzero payloads but remains negative for authority planning:

```text
same_tuple_payload_variation_observed=false
only_setting_change_scenario_observed=true
payload_bearing_authority_state_source_found=false
live_read_only_probe_allowed=false
authorization_plan_allowed=false
motion_attempt_allowed=false
wheel_moved_under_openracing=false
visible_motion_verified=false
output_was_sent=false
authority_state=blocked
```

The next step is timing-correlated `0x8E` evidence or another reviewed
payload-bearing authority-state source, not a live probe, authorization, PIDFF
rerun, force escalation, or motion attempt.

`vendor-status-timing-correlation-plan.json` now stages that next passive
timing-correlation run. It consumes the semantic review and requires:

```text
fresh observe-only hardware doctor selector
sniff-capture with --hardware-doctor
Pit House opened and settled as a witness app
KS top-left front LED default teal -> red -> default teal
explicit operator event markers
raw pcap local by default
future no-output timing-correlation review
```

`vendor-status-timing-correlation-review.json` now reviews the existing accepted
Pit House setting-change derived summary and notes with that future review
shape. The reprocessed derived summary contains timestamped samples and records
same-tuple payload variation for all four target `0x8E` tuples, but the notes do
not contain the explicit event-marker fields required by the timing-correlation
plan. The verdict is
`insufficient_missing_event_markers_or_packet_timestamps`; live read-only probe,
authorization planning, PIDFF rerun, force escalation, and motion remain
blocked.

`vendor-status-movement-blocker-audit.json` now consolidates the read-only
zero-response path into one current blocker statement. The original
`vendor-status-mode-matrix.json` decoded zero responses, but
`vendor-status-mode-matrix-demux.json` decoded seven non-authority status
replies on the same COM4 serial lane. The audit records that broad serial
ownership, line-setting, framing, and scan-window depth are not the current
primary blockers. The active blockers are authority endpoint/command mismatch
and missing timing-correlated payload-bearing status-source evidence. It keeps
`wheel_moved_under_openracing=false`, `visible_motion_verified=false`,
`output_was_sent=false`, and `authority_state=blocked`, and it routes the next
concrete action to the staged passive Pit House 0x8E event-marker capture and
no-output timing review, not a read-only rerun or motion attempt.

The plan is not capture evidence. It records `capture_recorded=false`,
`timing_correlation_proven=false`, `live_read_only_probe_allowed=false`,
`authorization_plan_allowed=false`, `motion_attempt_allowed=false`,
`wheel_moved_under_openracing=false`, `visible_motion_verified=false`,
`output_was_sent=false`, and `authority_state=blocked`.

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
present and emits no output command. The read-only demux follow-up decoded
seven registry status replies while leaving `estop_get_ffb` and
`main_misc_get_ffb_status` failed closed. The extended scan-window follow-up
also decoded zero authority-state replies with a 64-frame cap, and the stored
passive response-source correlation still found no payload-bearing
authority-state source. The correlated response semantic fixture review now
shows the available correlated passive payloads are zero-filled/static, and the
payload-source semantic review records the four nonzero `0x8E` setting-change
samples as fixture-covered but still insufficient for authority planning. The
timing-correlation plan stages the next passive Pit House LED event-marker run
but records no capture or timing proof. The native path remains blocked on a
reviewed payload-varying authority-state source or equivalent timing-correlated
status source before proposing any further hardware output family.

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
