# Moza Native Visible Lane Handoff

Status: blocked
Last verified: 2026-05-29
Lane: `ci/hardware/moza-r5/2026-05-13`
Active goal: `.openracing/goals/active.toml`

This handoff exists because the active goal has no `ready` work item. The next
implementation steps are blocked by real hardware or external evidence, and
agents must not invent more no-output churn to move the lane. No-output work is
only useful when it consumes checked-in evidence, tightens a gate, or preserves a
claim boundary.

## Verified Frontier

The lane is currently `native_response_ready`.

Current verified state:

- `ready_for_native_control=true`
- `native_actuator_response_proven=true`
- `native_visible_motion_proven=false`
- `native_control_blocking_items=[]`
- `frontier=closed_loop_undertravel_recorded`
- `hardware_output_authorized=false`

The first 1 degree controlled-angle attempt, the reviewed retry, attempt 03,
and the closed-loop attempt all failed safely below the visible-motion
threshold. Attempt 03 used `bounded-pidff-effect-lifecycle-v1`, consumed the
exact command-bound authorization, sent four bounded PIDFF writes, timed out
before target, sent final Stop All, stayed post-stop stable, and recorded no
direct report `0x20`, no high torque, no serial config, and no firmware or DFU.
The closed-loop attempt used `closed-loop-pidff-angle-v1`, consumed its exact
command-bound authorization, recomputed bounded PIDFF force from live
steering-angle error, sent 672 bounded reports with zero write errors, timed out
at `angle_delta_degrees=0.13183794918745662`, sent final Stop All/final zero,
and stayed post-stop stable.

The vendor-authority rail has now also run exactly one `estop_set_ffb` attempt
through the exact authorization gate. The consumed attempt sent only
`7E02461C0001F0`, consumed `vendor-authority-authorization.json`, and closed
hardware output authorization again. The separately authorized post-authority
PIDFF response receipt then recorded a lower response than the preserved
baseline: baseline `0.18127718013275285` degrees, post-authority
`0.032959487296864154` degrees, delta change `-0.1483176928358887` degrees.
`vendor-post-authority-pidff-response.json` classifies this as
`post_authority_pidff_response_regressed`. It is diagnostic evidence only, not
native-control or native-visible proof.

`native-controlled-angle-attempt-03-failure-analysis.json` classifies attempt 03
as safe undertravel and keeps native visible motion unclaimed.
`native-pidff-standard-path-diagnosis.json` classifies the standard PIDFF
controlled-angle path as ineffective in the current R5 device mode after three
same-band undertravel attempts. `native-controlled-angle-closed-loop-failure-analysis.json`
classifies the feedback-bounded attempt as safe undertravel and keeps native
visible motion unclaimed. `vendor-post-authority-pidff-response.json` extends
that diagnosis: the tested vendor-authority frame did not improve the comparable
standard PIDFF response under the same 5 percent / 2000 ms envelope.

Six passive sniff scenario plans now exist under
`ci/hardware/sniff/moza-r5/2026-05-13`. The `pit-house-open-idle` and
`pit-house-full-controls` scenarios have checked-in non-claiming sniff
receipts, classified summaries, and bundle manifests. The repeat
`pit-house-setting-change` capture is also recorded as accepted passive
correlation evidence with derived receipt, summary, bundle manifest, and
operator notes only; raw pcapng and ZIP artifacts remain local scratch. The
remaining `simhub-open-idle`, `simhub-output-session`, and
`simulator-session-start-stop` scenarios remain navigation-only until matching
pcap receipts and summaries exist. No passive sniff artifact authorizes hardware
output or satisfies native-visible, smoke-ready, or coexistence gates. No further
hardware output is authorized.

`vendor-protocol-evidence-review.json` now records a no-output review across the
checked-in passive sniff summaries, command registry, consumed vendor-authority
attempt, and post-authority PIDFF comparison. It classifies the current state as
`estop_set_ffb_regressed_and_protocol_enable_path_still_undecoded`, records that
the current summaries do not expose a decoded output candidate, and keeps
`planned_next_output.allowed=false`.

The same review now distinguishes absence of decoded semantic commands from
passive protocol shape. The checked-in Pit House summaries include
host-to-device USB transfers that are not fully mapped to report IDs or payload
candidates in the stored summary. The broad payload export gap has been
narrowed: TShark's `usbcom.data.*_payload` fields now expose candidate
host-to-device frame/report ID `0x7E`, 3,246 extracted host-to-device payload
packets, and 53,988 extracted host-to-device payload bytes across the two
completed Pit House scenarios. Two data-length packets still lack extracted
payload bytes. The review now keeps those residual gaps as bounded locators in
`sniff_evidence.payload_export_gap_summary`: packet ordinal/frame number `5`
for `pit-house-open-idle` and packet ordinal/frame number `5` for
`pit-house-full-controls`, both with `payload_extracted=false`,
`hardware_output_authorized=false`, and `output_sendability_claim=false`.

Those extracted host-to-device payloads now parse into 7,863 length-prefixed
`0x7E` serial-frame candidates with 7,863 valid checksums, zero invalid
checksums, and no frame-shape decode gap. The review preserves 30 tuple IDs and
1,467 commandless frames as protocol-shape evidence only. It also compares the
30 distinct passive tuple IDs to the semantic vendor command registry: one tuple
matches, `0x28/0x13/0x02` (`base_gain_get_overall_strength`), and that match is
read-only status evidence only. The remaining passive tuple evidence is 12
commandless tuple IDs and 17 unknown commanded tuple IDs, with zero known
write-like tuple matches. The same review now ranks per-scenario tuple
frequency so the first decode targets are explicit: `0x5A/0x1B/0x00` appears
1,896 times, `0x5D/0x1B/0x01` appears 1,894 times, and
`0x25/0x19/0x01`, `0x25/0x19/0x02`, and `0x25/0x19/0x03` each appear 624
times. Those high-frequency tuples remain `unknown_commanded` and
`unknown_do_not_send`. The review also preserves 159 bounded passive tuple
sample frames and 30 decode-candidate sample frames for the five
highest-frequency unknown commanded tuples. Artifact-index and bench-wizard now
render the same decode-priority queue plus representative sample frames from the
checked-in review receipt so the normal operator navigation shows concrete
fixture examples without hand-inspecting the large JSON receipt. This does not
decode an approved semantic enable/mode command, make any tuple sendable,
authorize hardware output, or unblock native-visible promotion. The protocol
crate now validates those 30 sample frames as observed wire-shape fixtures while
keeping the tuples unknown to the semantic registry. It also checks recurring
packet-order hints in the same checked-in samples: `0x5A/0x1B/0x00` followed by
`0x5D/0x1B/0x01`, and the ordered `0x25/0x19/0x02` -> `0x25/0x19/0x03` ->
`0x25/0x19/0x01` triad. Those sequence hints remain observed protocol-shape
evidence only, not semantic command or sendability proof. The review now also
records payload-shape morphology for those same top unknown samples: all 30
samples are checksum-valid and unknown-commanded, `0x5A/0x1B/0x00` samples have
empty payloads, and the `0x5D/0x1B/0x01` plus `0x25/0x19/0x01..03` samples use
zero-filled `0000` payloads. This is fixture navigation only; empty or
zero-filled payloads do not make unknown tuples semantic or sendable. The same
review now records packet-group morphology: 11 packet groups, three packet
patterns, and four repeated contiguous motifs. The repeated `0x5A/0x1B/0x00`
-> `0x5D/0x1B/0x01` pair and repeated `0x25/0x19/0x02` ->
`0x25/0x19/0x03` -> `0x25/0x19/0x01` triad are observed six times each across
the two checked-in Pit House scenarios, but this remains fixture evidence only
and does not decode or authorize any tuple. The review now adds low-confidence
semantic hypotheses for those same samples: the `0x5A/0x1B/*` and
`0x5D/0x1B/*` pair is a `session_or_status_keepalive_candidate`, and the
`0x25/0x19/*` triad is a `base_status_or_mode_poll_candidate`. These are
pattern-only decode questions. Every tuple remains `unknown_commanded`,
`unknown_do_not_send`, non-sendable, and ineligible for registry promotion or
hardware output.
The review now converts those hypotheses into a no-output semantic correlation
plan. It groups the five tuple hypotheses into two non-sendable correlation
targets, records that both targets are observed in the completed
`pit-house-open-idle` and `pit-house-full-controls` summaries, and names
`pit-house-setting-change` as the next passive capture priority before the
remaining SimHub and simulator scenarios. The plan pins
`semantic_decode_claim=false`, `registry_promotion_claim=false`,
`hardware_output_authorized=false`, `native_control_evidence=false`,
`output_sendability_claim=false`, and
`protocol_evidence_sufficient_for_output_plan=false`.
The `pit-house-setting-change` plan now also requires the exact Pit House
setting changed, starting value, ending value, and an affirmative restore
status before the capture can become accepted passive correlation evidence.
The 2026-05-27 bounded passive setting-change run is recorded only as
`low-yield-capture-classification.json`: the local pcap was 355 bytes with six
packets, zero `0x346E:0x0004` matches, and restore status `not reported`. It is
not decoded setting-change evidence and does not complete the scenario. A repeat
capture used the corrected USBPcap selector `\\.\USBPcap2 --devices 4` and is
now the accepted setting-change passive evidence: 100,492 Moza `0x346E:0x0004`
packets over 113.446197 seconds, host-to-device vendor candidates `0x7E` and
`0x80`, and operator notes for `KS wheel top-left front LED` default teal -> red
-> default teal. The bounded helper's final receipt-write file-lock error is a
tooling note only because the pcap finalized and the derived summary/bundle
validated.

## Completion Audit Summary

The broader Moza objective remains incomplete:

| Requirement | Current evidence | Status |
| --- | --- | --- |
| Passive enumeration, descriptor capture, parser fixtures | Lane passive receipts, parser fixture validation, passive verifier | Proven |
| Zero, watchdog, disconnect, low-torque, native response | Zero/openracing-control/native-response receipts and verifiers | Proven |
| Native visible motion | `verify-bundle --stage native-visible-ready` | Blocked: `native_actuator_visible_smoke` |
| Attempt-03 authorization | `native-controlled-angle-attempt-03-authorization.json` | Recorded and consumed |
| Attempt-03 output | `native-controlled-angle-attempt-03-smoke.json` | Recorded safe undertravel |
| Attempt-03 analysis | `native-controlled-angle-attempt-03-failure-analysis.json` | Recorded no-output classification |
| Standard PIDFF path diagnosis | `native-pidff-standard-path-diagnosis.json` | Recorded no-output architecture diagnosis |
| Closed-loop controlled-angle output | `native-controlled-angle-closed-loop-smoke.json` | Recorded safe undertravel |
| Closed-loop failure analysis | `native-controlled-angle-closed-loop-failure-analysis.json` | Recorded no-output classification |
| Consumed vendor-authority attempt | `vendor-authority-attempt.json` | Recorded exact one-frame non-claiming attempt |
| Post-authority PIDFF response | `vendor-post-authority-pidff-smoke.json`; `vendor-post-authority-pidff-response.json`; [post-authority PIDFF response diagnosis](../../docs/hardware/moza-r5-post-authority-pidff-response.md) | Recorded regression versus baseline; no native-visible claim |
| Vendor protocol evidence review | `vendor-protocol-evidence-review.json`; artifact-index/bench-wizard `vendor_protocol_decode_priority` | Recorded no-output review, host-to-device candidate `0x7E`, 7,863 checksum-valid candidate frames, 159 bounded passive tuple sample frames, one read-only registry tuple match, 12 commandless tuple IDs, 17 unknown commanded tuple IDs, frequency-ranked unknown commanded tuples headed by `0x5A/0x1B/0x00` and `0x5D/0x1B/0x01`, 30 decode-candidate sample frames for the top unknown tuples, protocol-crate observed-frame, packet-order, payload-shape, packet-group, low-confidence semantic-hypothesis, semantic-correlation-plan, and mode/enable candidate-question regression coverage for those samples, two residual payload export gap packet locators, and no sufficient semantic protocol evidence for any output plan |
| Read-only vendor status matrix | `vendor-status-mode-matrix.json`; `vendor-status-mode-matrix-demux.json` | Recorded COM4 read-only evidence; seven non-authority status replies decode, but authority-state replies remain missing |
| Authority status endpoint diagnosis | `vendor-status-authority-endpoint-diagnosis.json`; `vendor-status-endpoint-candidates.json`; `vendor-status-endpoint-candidates-from-payload-rerun.json` | Broad serial transport ruled out; current authority-status endpoint returns ACK/no-payload or diagnostic telemetry only, and corrected endpoint candidates remain non-sendable |
| Payload-bearing status-source candidates | `vendor-status-payload-source-candidates.json` | Recorded four nonzero `0x8E` device-to-host setting-change samples as `unknown_do_not_send` status-source questions; no semantic decode, probe readiness, authorization, output, or motion claim |
| Payload-source semantic review | `vendor-status-payload-source-semantic-review.json` | Fixture decoder coverage now recognizes the four `0x8E` samples as payload-bearing status-source questions, but the review records no same-tuple payload variation, no timing correlation, no authority-state source, and no live probe/output/motion eligibility |
| Passive sniff protocol evidence | `pit-house-open-idle`, `pit-house-full-controls`, and `pit-house-setting-change` sniff receipts, summaries, and bundle manifests | Recorded non-claiming evidence; setting-change keeps the earlier low-yield classification as historical failed evidence |
| Remaining passive sniff plans | `simhub-open-idle`, `simhub-output-session`, `simulator-session-start-stop` sniff plans | Plan-only |
| Pit House coexistence | `pit-house-coexistence.json` | Missing |
| Simulator telemetry | `simulator-telemetry-proof.json` | Missing |
| Bounded simulator FFB | `simulator-ffb-smoke.json` | Missing |
| Smoke-ready promotion and audit | smoke-ready verifier, promotion, audit | Not eligible |

Input topology remains partially semantic: steering, throttle, brake, HBP
handbrake, KS rim controls, and ES rim controls are parser-proven. SR-P clutch
is parser-visible through two live R5 V1 extended auxiliary fields, but the
role-specific clutch mapping remains diagnostic with `readiness_claim=false`.

## Required Next Event

The next operator step remains review-only: current evidence has recorded the
post-authority PIDFF regression and reviewed the checked-in protocol evidence
without finding a decoded enable path. It now extracts and frame-shape parses
the Pit House USB CDC payload stream far enough to surface candidate frame/report
ID `0x7E`, 7,863 checksum-valid candidate frames, and 30 tuple IDs, but those
tuples are not decoded into an approved semantic command and still cannot
authorize output. The registry comparison currently finds only one read-only
status tuple, `0x28/0x13/0x02`, and fences 12 commandless plus 17 unknown
commanded tuple IDs as `unknown_do_not_send`. The frequency review makes the
highest-count unknown commanded decode targets `0x5A/0x1B/0x00`,
`0x5D/0x1B/0x01`, `0x25/0x19/0x01`, `0x25/0x19/0x02`, and
`0x25/0x19/0x03`; artifact-index and bench-wizard surface that same queue as
`vendor_protocol_decode_priority`, now with bounded sample frame examples for
the top unknown tuples. The protocol crate accepts those examples as observed
wire-shape fixtures, preserves repeated pair/triad ordering hints, records that
the current sampled payloads are empty or zero-filled, and still rejects them
from the semantic fixture decoder as unknown commands. The accepted
`pit-house-setting-change` capture adds a named Pit House LED setting transition
to those passive correlation inputs without decoding a sendable command. The
review now also
preserves pattern-only hypotheses that make the next decode questions explicit:
`session_or_status_keepalive_candidate` for the `0x5A/0x1B/*` and
`0x5D/0x1B/*` pair, and `base_status_or_mode_poll_candidate` for the
`0x25/0x19/*` triad. It emits no hardware output command and no authorization
command. The review now also records those two groups as no-output
mode/enable candidate questions: the `0x25/0x19/*` triad asks whether the
traffic is a `status_query`, `standard_pidff_mode_enable`, or
`game_control_mode_select`, and the `0x5A/0x1B/*` plus `0x5D/0x1B/*` pair asks
whether the traffic is an `authority_keepalive` or
`volatile_ffb_session_enable`. Those questions are still
`unknown_do_not_send`; they are not semantic decoder proof, registry entries,
sendable tuples, authorization inputs, or output evidence. The correlation plan
now moves the next passive evidence target to SimHub and simulator scenarios,
before any tuple can move toward semantic decoder coverage or registry review.
The `simhub-open-idle` handoff is staged
only: it requires a fresh `wheelctl hardware doctor` immediately before capture,
the current USBPcap Moza selector hint passed through
`sniff-capture --hardware-doctor`, SimHub opened by the operator after capture
starts, idle/stable confirmation, no SimHub output session, no simulator, no
firmware/update/DFU page or prompt, raw pcap local-only, and OpenRacing
no-output confirmation. The native-control implementation path has recorded
fake-transport containment for the mode/enable candidate questions:
representative frames are observable in software fake transport while the
command/send path still rejects them as unknown commands.

The read-only hardware status/mode matrix is now recorded at
`vendor-status-mode-matrix.json` with its fresh precondition doctor at
`vendor-status-mode-matrix-hardware-doctor.json`. COM4 was verified as the R5
`0x346E:0x0004` serial/CDC interface and Pit House was not running. The guarded
`wheelctl moza vendor-status-probe` sent nine registry-approved read-only status
query frames, opened no HID path, and sent no output, PIDFF, feature,
configuration, firmware, DFU, or high-torque command. It failed closed with
zero decoded responses and nine failed responses, so
`real_hardware_status_evidence=false` and
`unknown_safety_or_mode_state_blocks_authority=true` remain the controlling
state.

The no-output framing diagnosis is now recorded at
`vendor-status-framing-diagnosis.json`. It reads only the stored matrix receipt
and opens no HID or serial device. The diagnosis classifies the nine captured
response frames as repeated tuple `0x0E/0x71/0x05` ASCII `NRFloss`/`recvGap`
diagnostic stream frames, including one desynchronized partial frame, rather
than registry status/mode replies. The native-path blocker is therefore
transport framing, serial stream demultiplexing, or endpoint/command
correlation.

The read-only authority-status correlation follow-ups have now narrowed that
blocker. `vendor-status-reply-correlation-targeted.json` selected only
`estop_get_ffb` and `main_misc_get_ffb_status`, decoded zero authority-state
replies, and the diagnosis preserved one response-like command mismatch
`0xA1/0x21/0x4D` while requesting `main_misc_get_ffb_status`
`0x21/0x12/0x07`. `vendor-status-extended-scan-targeted.json` repeats that same
read-only probe with `--max-response-frames-per-query 64`; it scanned 19 frames,
decoded zero authority-state replies, and the regenerated diagnosis now
classifies `7E00A1214D` as a checksum-valid zero-length response-like frame:
`0xA1/0x21/no_command`. Shallow scan-window depth is no longer the immediate
explanation. The latest diagnosis narrows the frame to ACK-only/no-payload
correlation evidence, not decoded status evidence, so mode and safety remain
unknown until a payload-bearing status reply is decoded or the endpoint/command
IDs are corrected. The later endpoint-candidate receipt now turns that into a
no-output fixture-backed decoder coverage step before any live probe,
authorization, PIDFF, force escalation, or motion.

The ACK-only correlation live rerun is now recorded at
`vendor-status-ack-only-correlation-targeted.json`, with its fresh observe-only
doctor at `vendor-status-ack-only-correlation-hardware-doctor.json` and derived
diagnosis at `vendor-status-ack-only-correlation-diagnosis.json`. It selected
only `estop_get_ffb` and `main_misc_get_ffb_status`, sent two
registry-approved read-only queries, opened no HID path, sent no output,
configuration, firmware, DFU, or PIDFF commands, and decoded zero
authority-state replies. The diagnosis reproduced the same
`0xA1/0x21/no_command` ACK-only/no-payload candidate. The exact blocker remains
status-payload correlation or corrected authority-status endpoint/command IDs.

The authority endpoint diagnosis is now recorded at
`vendor-status-authority-endpoint-diagnosis.json`. It compares the ACK-only
targeted probe with `vendor-status-mode-matrix-demux.json`, which decoded seven
payload-bearing non-authority status replies on the same serial lane. That
rules out broad serial framing, ownership, timeout, or line settings as the
primary blocker. The native path is blocked on authority-status endpoint/command
correction: `estop_get_ffb` and `main_misc_get_ffb_status` still decode zero
authority-state replies, and `0xA1/0x21/no_command` remains ACK/no-payload
correlation evidence only.

The endpoint candidate plan is now recorded at
`vendor-status-endpoint-candidates.json`. It is a no-output stored-receipt
review that records the expected `main_misc_get_ffb_status` payload response
shape as `0xA1/0x21/0x07`, preserves the observed authority response as
ACK-only `0xA1/0x21/no_command`, and carries forward the passive
`0x25/0x19/*`, `0x5A/0x1B/*`, and `0x5D/0x1B/*` groups as
`unknown_do_not_send`. It does not open HID or serial, send read-only queries,
authorize output, promote registry sendability, or claim semantic decode. The
next native-path step is no-output fixture-backed decoder coverage for a
corrected authority-status endpoint candidate, not a live probe, not output,
not a mode-enable write, not PIDFF, and not motion.

The authority-status payload fixture follow-up is now recorded in tests and in
`vendor-status-authority-payload-rerun-targeted.json` /
`vendor-status-authority-payload-rerun-diagnosis.json`. Fake decoder coverage
now pins the payload-bearing response shapes that would satisfy the two current
registry-approved authority-status queries:

```text
main_misc_get_ffb_status -> 0xA1/0x21/0x07 payload response
estop_get_ffb           -> 0xC6/0xC1/0x01 payload response
```

The targeted live rerun stayed read-only: it opened COM4, sent only those two
registry-approved read-only queries, opened no HID path, and sent no output,
configuration, firmware, DFU, PIDFF, authority, or mode-enable command. It still
decoded zero authority-state replies. The regenerated diagnosis classifies the
readback as
`authority_status_endpoint_specific_debug_telemetry_without_payload`: all 24
scanned frames were checksum-valid ASCII diagnostic stream frames, dominated by
`0x0E/0x71/0x05`, with no payload-bearing authority-status response observed.
Because the demux baseline decoded seven payload-bearing non-authority status
replies on the same serial lane, broad serial framing, ownership, timeout, and
line settings are not the primary blocker. The current native-path blocker is
authority-status endpoint/command mismatch, not force or a motion controller
issue.
The payload-rerun endpoint candidate plan is now recorded at
`vendor-status-endpoint-candidates-from-payload-rerun.json`. It consumes the
latest debug-telemetry-only diagnosis, records observed diagnostic tuple
`0x0E/0x71/0x05`, keeps the expected payload-bearing status shape as
`0xA1/0x21/0x07`, and keeps `corrected_read_only_probe_ready=false`. It does
not open HID or serial, send read-only queries, authorize output, promote
registry sendability, or claim semantic decode.

Wheel movement remains unproven:
`wheel_moved_under_openracing=false`, `visible_motion_verified=false`,
`output_was_sent=false`, and `authority_state=blocked`. The next native-path
step is no-output fixture-backed decoder coverage for a corrected
authority-status endpoint candidate before any live probe, authorization, PIDFF
rerun, force escalation, or motion ladder attempt.

That fake-only endpoint-candidate containment is now in place. The fake
transport consumes `vendor-status-endpoint-candidates-from-payload-rerun.json`
with the passive evidence review, observes the two passive endpoint-candidate
groups and five representative frames, and verifies every frame remains
`unknown_do_not_send`, not a payload-status match, not corrected-probe-ready,
and rejected by the command send path. Wheel movement remains unproven:
`wheel_moved_under_openracing=false`, `visible_motion_verified=false`,
`output_was_sent=false`, and `authority_state=blocked`. The next native-path
step is still no-output protocol work to identify a payload-bearing
authority-state status endpoint or equivalent reviewed status source before any
live probe, authorization, PIDFF rerun, force escalation, or motion ladder
attempt.

The endpoint-candidate receipt now also records a no-output passive scan for
command-id `0x07` analog tuples because the current authority-status query is
`0x21/0x12/0x07`. The scan derives five unknown-commanded passive analogs:
`0x40/0x17/0x07`, `0x28/0x13/0x07`, `0x23/0x19/0x07`,
`0x3F/0x17/0x07`, and `0x5B/0x1B/0x07`. The most frequent analog,
`0x40/0x17/0x07`, appears in all three completed Pit House passive scenarios,
but it remains `unknown_do_not_send`, not a payload-status match, not
read-only-probe-allowed, and not sendable. Wheel movement remains unproven:
`wheel_moved_under_openracing=false`, `visible_motion_verified=false`,
`output_was_sent=false`, and `authority_state=blocked`. The next native-path
step remains no-output fixture-backed decoder coverage for these command-id
`0x07` analogs or another reviewed payload-bearing authority-state source
before any live probe, authorization, PIDFF rerun, force escalation, or motion
ladder attempt.

That fake-only command-id `0x07` analog containment is now in place. The fake
transport consumes the five analog tuples from
`vendor-status-endpoint-candidates-from-payload-rerun.json`, observes
representative zero-payload fixture frames for them, and verifies every frame
remains `unknown_do_not_send`, not a payload-status match,
not corrected-probe-ready, not read-only-probe-allowed, not sendable, and
rejected by the command send path. These are endpoint-search fixtures only, not
semantic decode proof and not live probe inputs. Wheel movement remains
unproven: `wheel_moved_under_openracing=false`,
`visible_motion_verified=false`, `output_was_sent=false`, and
`authority_state=blocked`. The next native-path step remains no-output protocol
work to identify a payload-bearing authority-state status endpoint or
equivalent reviewed status source before any live probe, authorization, PIDFF
rerun, force escalation, or motion ladder attempt.

The authority-status source-gap review is now recorded at
`vendor-status-authority-source-gap.json`. It reads only checked-in evidence and
confirms there is still no reviewed payload-bearing authority-state status
endpoint or equivalent status source. Current registry authority-status queries
remain ACK/debug-only, the passive command-id `0x07` analogs and mode/enable
groups remain `unknown_do_not_send`, and the protocol evidence review extracts
18 checksum-valid device-to-host serial frame samples from stored passive
summaries under the sample scope `observed_report_payload_hex_samples_only`.

The response-source correlation receipt is now recorded at
`vendor-status-response-source-correlation.json`. It closes the generic
"correlate response samples" step by recording sample-scoped response-shape
correlation for the unknown passive question groups:

```text
0x25/0x19/* -> 0xA5/0x91/*
0x5A/0x1B/0x00 -> 0xDA/0xB1/0x00
0x5D/0x1B/0x01 -> 0xDD/0xB1/0x01
```

That is not packet-timing proof and not an authority-state source. The expected
registry authority-status response tuples `0xA1/0x21/0x07` and
`0xC6/0xC1/0x01` are absent from the stored passive response samples, and the
passive command-id `0x07` analogs have no matching response-side sample tuples.
Wheel movement remains unproven: `wheel_moved_under_openracing=false`,
`visible_motion_verified=false`, `output_was_sent=false`, and
`authority_state=blocked`.

The correlated response semantic fixture review is now recorded at
`vendor-status-response-semantic-fixtures.json`. It decodes the correlated
passive response fixture shapes for `0xA5/0x91/*`, `0xDA/0xB1/0x00`, and
`0xDD/0xB1/0x01`, but all 11 checked-in correlated response samples have
zero-filled/static payloads. The review keeps every candidate
`unknown_do_not_send`, with `payload_variation_observed=false`,
`payload_bearing_authority_state_source_found=false`,
`corrected_read_only_probe_ready=false`, `live_read_only_probe_allowed=false`,
and `motion_attempt_allowed=false`. Wheel movement remains unproven:
`wheel_moved_under_openracing=false`, `visible_motion_verified=false`,
`output_was_sent=false`, and `authority_state=blocked`. The next native-path
step is to add or capture a reviewed payload-varying authority-state status
source before any live probe, authorization plan, PIDFF rerun, force escalation,
or motion attempt.

The payload-bearing status-source candidate review is now recorded at
`vendor-status-payload-source-candidates.json`. It preserves four nonzero
device-to-host `0x8E` samples from the accepted Pit House setting-change
scenario:

```text
0x8E/0x21/0x00
0x8E/0x31/0x00
0x8E/0x71/0x00
0x8E/0x91/0x00
```

Those samples are useful protocol navigation, but remain
`unknown_do_not_send`. They are not same-tuple payload variation, not
packet-timing proof, not semantic decode, not registry promotion, not a reviewed
authority-state source, and not read-only probe or output eligibility. The
receipt keeps `payload_bearing_authority_state_source_found=false`,
`corrected_read_only_probe_ready=false`, `live_read_only_probe_allowed=false`,
`authorization_plan_allowed=false`, `motion_attempt_allowed=false`,
`wheel_moved_under_openracing=false`, `visible_motion_verified=false`,
`output_was_sent=false`, and `authority_state=blocked`. The next native-path
step is fixture-backed semantic review or timing-correlated capture for the
payload-bearing `0x8E` candidates before any live probe, authorization, PIDFF
rerun, force escalation, or motion attempt.

The fixture-backed semantic review is now recorded at
`vendor-status-payload-source-semantic-review.json`. It confirms the `0x8E`
samples decode through the passive fixture review group
`passive_payload_bearing_status_source_0x8e` and preserves the payload values:

```text
0x8E/0x21/0x00 -> 019100002624
0x8E/0x31/0x00 -> 019100002624
0x8E/0x71/0x00 -> 0FB300000001
0x8E/0x91/0x00 -> 013E000000E6
```

The review is still negative for authority planning:
`same_tuple_payload_variation_observed=false`,
`only_setting_change_scenario_observed=true`,
`payload_bearing_authority_state_source_found=false`,
`live_read_only_probe_allowed=false`, `authorization_plan_allowed=false`,
`motion_attempt_allowed=false`, `wheel_moved_under_openracing=false`,
`visible_motion_verified=false`, `output_was_sent=false`, and
`authority_state=blocked`. The next native-path step is timing-correlated
`0x8E` evidence or another reviewed payload-bearing authority-state status
source before any live probe, authorization, PIDFF rerun, force escalation, or
motion attempt.

The read-only demux follow-up is now recorded at
`vendor-status-mode-matrix-demux.json` with its fresh precondition doctor at
`vendor-status-mode-matrix-demux-hardware-doctor.json`. It kept the same
read-only boundary, skipped diagnostic stream frames, accepted the observed
response-side group/device tuple transform, and decoded seven registry status
responses. The receipt still failed closed because `estop_get_ffb` and
`main_misc_get_ffb_status` did not decode, so
`unknown_safety_or_mode_state_blocks_authority=true` remains the authority
blocker. Wheel movement remains unproven:
`wheel_moved_under_openracing=false`, `visible_motion_verified=false`,
`output_was_sent=false`, and `authority_state=blocked`. Later
authority-endpoint, endpoint-candidate, and fake-containment receipts narrow
this from broad demux repair to corrected endpoint/command review. The current
next native-path step is no-output protocol evidence for a payload-varying
authority-state status source before any live probe, output, mode-enable
write, PIDFF rerun, or motion ladder plan. The next witness-lane operator work
remains the remaining passive SimHub/simulator captures.

The targeted read-only status reply correlation follow-up is now recorded at
`vendor-status-reply-correlation-targeted.json`, with its fresh observe-only
doctor at `vendor-status-reply-correlation-hardware-doctor.json` and derived
offline diagnosis at `vendor-status-reply-correlation-diagnosis.json`. It
selected only `estop_get_ffb` and `main_misc_get_ffb_status`, used a 1000 ms
read-only timeout, sent two registry-approved read-only query commands, opened
no HID path, and sent no output/configuration/firmware/PIDFF commands. It still
decoded zero authority-state replies. The offline diagnosis classifies 23 of 24
scanned frames as diagnostic telemetry and records one response-like
group/device tuple with a command mismatch:
`0xA1/0x21/0x4D` while requesting `main_misc_get_ffb_status`
`0x21/0x12/0x07`. That is correlation evidence only. It is not semantic decode,
registry promotion, tuple sendability, authorization input, native control, or
native-visible motion. The current blocker is authority-status command/endpoint
correlation, with `unknown_safety_or_mode_state_blocks_authority=true`.

Passive USB sniff captures may produce non-claiming `sniff-receipt.json`,
`sniff-summary.json`, and bundle manifest artifacts, but those are
protocol/coexistence evidence, not native readiness evidence. Preserve all four
controlled-angle undertravel receipts, the consumed vendor-authority attempt,
the post-authority PIDFF response receipts, the protocol evidence review, and
their analysis/diagnosis artifacts. Do not create another authorization or
output receipt from verifier guidance. Any future output family requires decoded
protocol evidence, a
reviewed vendor-control plan, fresh command-bound bench clear, and a new exact
authorization.

## Do Not Do

- Do not create another authorization receipt from this handoff.
- Do not run hardware output.
- Do not rerun attempt 03, the closed-loop attempt, or either previous 1 degree
  attempt.
- Do not retry `estop_set_ffb` or reuse the consumed vendor-authority attempt.
- Do not rerun the post-authority PIDFF response capture.
- Do not keep iterating standard PIDFF profile variants without new protocol
  evidence.
- Do not raise force, extend dwell, or jump to 3, 5, 30, or 90 degrees.
- Do not use direct report `0x20`.
- Do not use high torque.
- Do not run serial config, firmware, or DFU flows.
- Do not treat Pit House, SimHub, simulator, or passive sniff evidence as native
  OpenRacing motion proof.
- Do not commit raw `.pcapng` captures unless a separate review approves the
  raw capture, size, sensitivity, and operator consent.

## Verification Commands

Use these no-output commands to refresh the handoff state:

```powershell
cargo run --locked -p wheelctl --bin wheelctl -- moza pre-output-readiness `
  --lane ci/hardware/moza-r5/2026-05-13 `
  --json-out target/moza-pre-output-current.json `
  --json

cargo run --locked -p wheelctl --bin wheelctl -- moza verify-bundle `
  --lane ci/hardware/moza-r5/2026-05-13 `
  --stage native-visible-ready `
  --json-out target/moza-native-visible-current.json `
  --json

cargo run --locked -p wheelctl --bin wheelctl -- moza artifact-index `
  --lane ci/hardware/moza-r5/2026-05-13 `
  --json-out target/moza-artifact-index-current.json `
  --md-out ci/hardware/moza-r5/2026-05-13/index.md `
  --json

cargo run --locked -p wheelctl --bin wheelctl -- moza bench-wizard `
  --lane ci/hardware/moza-r5/2026-05-13 `
  --json-out target/moza-bench-wizard-current.json `
  --md-out target/moza-bench-wizard-current.md `
  --json
```

`verify-bundle --stage native-visible-ready` is expected to exit with code `4`
until a passing visible-motion receipt exists.
