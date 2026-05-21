# Moza R5 Lane Completion Audit

Status: active
Owner: hardware
Created: 2026-05-18
Linked proposal: docs/proposals/OR-PROP-0001-moza-native-visible-lane.md
Linked specs: docs/specs/OR-SPEC-0001-moza-native-visible-lane.md
Linked ADRs: docs/adr/0009-hardware-validation-evidence-state-machine.md
Linked plan: plans/moza-native-visible-lane/implementation-plan.md
Support/status impact: audit only; no readiness promotion
Policy impact: no new policy exception

This audit maps the Moza lane objective to concrete artifacts. It is not a
generated receipt, does not open HID, does not authorize output, and does not
satisfy native-visible, smoke-ready, or release-ready gates.

## Objective Restatement

The lane is complete when OpenRacing has a boring, reproducible, receipt-backed
hardware bring-up path for Steven's Moza R5 stack:

- R5 wheelbase, KS/ES wheels, SR-P pedals, and HBP handbrake are represented in
  a dated lane.
- Passive enumeration, descriptor capture, input parsing, and parser fixtures
  are proven by artifacts.
- Zero-torque, low-torque, watchdog, disconnect, and final-zero / Stop All
  safety proofs are proven by artifacts.
- Native movement reaches the visible-motion gate without overclaiming or
  bypassing authorization.
- Pit House coexistence is proven as external compatibility.
- One real simulator telemetry path is proven.
- One bounded sim-to-Moza FFB smoke run is proven.
- Smoke-ready is promoted only after the verifier, manifest promotion, and lane
  audit all pass.
- `release_ready`, `high_torque_validated`, firmware, DFU, serial config, and
  direct-report `0x20` output remain unclaimed unless separate evidence exists.

## Current Verdict

The objective is not complete.

The lane is currently `native_response_ready`. The native OpenRacing response
path is proven, but native visible motion is still blocked. Smoke-ready is also
blocked by missing Pit House coexistence, simulator telemetry, and bounded
simulator FFB receipts.

Attempt 03 and the later closed-loop controlled-angle attempt have each run
exactly once and failed safely. The consumed authorizations, output receipts,
native-visible verifier, and no-output failure analyses are preserved. No
further hardware output is authorized; the next work is no-output Moza
vendor-specific enable/control path investigation before any future output
family is proposed. The standard PIDFF path diagnosis records that three
bounded standard-PIDFF-family controlled-angle attempts remained in the same
undertravel band, and the closed-loop attempt records that feedback-bounded
PIDFF control also stayed below the visible-motion threshold.

The exact vendor-authority rail has also run one consumed
`estop_set_ffb` attempt and one separately authorized post-authority PIDFF
response capture. The comparison receipt classifies the result as
`post_authority_pidff_response_regressed`: baseline response was
`0.18127718013275285` degrees, post-authority response was
`0.032959487296864154` degrees, and the delta change was
`-0.1483176928358887` degrees. This is useful protocol evidence, but it is not
native-control proof, native-visible proof, or authorization to retry.

The vendor-control investigation now has six passive sniff scenario plans for
Pit House, SimHub, and simulator sessions. Two Pit House scenarios,
`pit-house-open-idle` and
`pit-house-full-controls`, have checked-in non-claiming receipts, summaries,
and bundle manifests. The other four scenarios remain navigation-only until
matching pcap receipts and summaries exist.

`vendor-protocol-evidence-review.json` reviews the current checked-in passive
summaries, command registry, consumed vendor-authority attempt, and
post-authority PIDFF comparison without opening HID or serial devices. It
classifies the current state as
`estop_set_ffb_regressed_and_protocol_enable_path_still_undecoded`, records no
decoded output command from the current summaries, and keeps
`planned_next_output.allowed=false`. The review now extracts and frame-shape
parses the Pit House USB CDC payload stream far enough to surface candidate
host-to-device frame/report ID `0x7E`, 3,246 extracted host-to-device payload
packets, 53,988 extracted host-to-device payload bytes, and 7,863
length-prefixed `0x7E` serial-frame candidates. All parsed candidate frames
have valid checksums, zero checksum-invalid frames, and no frame-shape decode
gap. It also compares 30 distinct passive tuple IDs to the semantic command
registry: only `0x28/0x13/0x02` (`base_gain_get_overall_strength`) matches, and
that match is read-only status evidence. The remaining passive tuple evidence is
12 commandless tuple IDs and 17 unknown commanded tuple IDs. The review now
preserves per-scenario tuple frequency and ranks the highest-count unknown
commanded tuples as `0x5A/0x1B/0x00` (1,896 frames),
`0x5D/0x1B/0x01` (1,894 frames), and `0x25/0x19/0x01`,
`0x25/0x19/0x02`, and `0x25/0x19/0x03` (624 frames each). Artifact-index and
bench-wizard now surface that decode-priority queue from the checked-in review
receipt, together with 30 bounded decode-candidate sample frames for those top
unknown tuples. The protocol crate now validates those sample frames as observed
wire-shape fixtures and still rejects them as semantic fixture commands because
their tuples are unknown. It also groups those samples into 11 packet-local
groups with three full-packet patterns and four repeated contiguous motifs:
the `0x5A/0x1B/0x00` then `0x5D/0x1B/0x01` pair, the ordered
`0x25/0x19/0x02`, `0x25/0x19/0x03`, `0x25/0x19/0x01` triad, and one combined
five-frame packet containing both patterns. Payload-shape hints still show
empty `0x5A` samples and zero-filled `0000` payloads for the `0x5D` and `0x25`
samples. Two data-length packets still lack extracted payload bytes. This is
protocol-shape, registry-coverage, frequency-prioritization, packet-group,
payload-shape, low-confidence semantic-hypothesis, semantic-correlation-plan,
and sample fixture navigation only, not native-control or native-visible proof.
The correlation plan groups the five low-confidence tuple hypotheses into two
non-sendable targets, records that both appear in the completed Pit House
summaries, and names `pit-house-setting-change` as the next passive capture
priority before SimHub and simulator correlation.
Those two residual payload export gaps are now preserved as packet/frame
locator examples in `sniff_evidence.payload_export_gap_summary`, with
`payload_extracted=false`, `hardware_output_authorized=false`, and
`output_sendability_claim=false`.

## Prompt-To-Artifact Checklist

| Objective requirement | Evidence inspected | Status | Notes |
| --- | --- | --- | --- |
| Dated real hardware lane exists for Steven's Moza stack | `ci/hardware/moza-r5/2026-05-13/manifest.json`; `device-list.json`; `moza-probe.json`; `hid-list.json`; `descriptor.json`; verifier endpoint and role evidence | Pass | Manifest completion state is `native_response_ready`; release, simulator, and high-torque validation are false. |
| R5 wheelbase observed | `verify-bundle --stage native-visible-ready`; gates `moza_r5_observed`, `manifest_r5_pid_consistency` | Pass | Current verifier found matching R5 `0x346E:0x0004` evidence across lane receipts and captures. |
| KS/ES wheels, SR-P pedals, and HBP handbrake represented | Verifier `role_evidence`; captures `ks-controls.jsonl`, `es-controls.jsonl`, `r5-throttle-only-sweep.jsonl`, `r5-brake-only-sweep.jsonl`, `r5-clutch-only-sweep.jsonl`, `r5-handbrake-only-sweep.jsonl` | Pass with one semantic caveat | KS, ES, steering, throttle, brake, and HBP handbrake are parser-proven. SR-P clutch is parser-visible through generic R5 V1 extended fields; role-specific clutch semantic naming remains unproven and diagnostic only. |
| Input role semantic mapping complete | `lane-capture-analysis.json`; `role-status-sync.json`; `artifact-index` Input Role Semantics section | Partial | Two candidate-only R5 V1 extended slots are surfaced for clutch. No candidate is ambiguous. Each candidate has `readiness_claim=false`; these help navigation but do not prove role-specific clutch semantics. |
| Passive enumeration proven | `passive-verification.json`; `manifest-promotion-passive.json`; `lane-audit-passive.json`; verifier passive gates | Pass | Passive verifier and audit pass. |
| Descriptor capture proven | `descriptor.json`; verifier gate `descriptor_metadata` | Pass | Descriptor metadata is complete for the R5 record. |
| Input parsing proven | Passive capture files; verifier gate `passive_captures_parse` | Pass | Current verifier replayed 128215 passive capture reports through Moza parsers. |
| Parser fixtures proven | `parser-fixture-validation.json`; `fixture-promotion.json`; verifier gates `parser_fixture_validation`, `fixture_promotion` | Pass | Current verifier validated 9 required parser captures and fixture promotion. |
| Zero-torque proof exists | `zero-torque-proof.json`; `zero-verification.json`; `manifest-promotion-zero.json`; `lane-audit-zero.json` | Pass | Real zero proof and zero-stage promotion/audit are present. |
| Watchdog proof exists | `watchdog-proof.json`; verifier gate `watchdog_zero_output` | Pass | Watchdog proof injected timeout and sent final zero. |
| Disconnect proof exists | `disconnect-proof.json`; verifier gate `disconnect_final_zero` | Pass | Disconnect proof observed HID write failure and attempted final zero with zero-only payloads. |
| Low-torque proof exists | `low-torque-proof.json`; verifier gate `low_torque_bounded`; `openracing-control-verification.json` | Pass | Bounded PIDFF low-torque proof and OpenRacing native control foundation are present. |
| Final-zero / Stop All cleanup paths proven for current native attempts | `native-actuator-profile-smoke.json`; `native-controlled-angle-smoke.json`; `native-controlled-angle-retry-smoke.json`; `native-controlled-angle-attempt-03-smoke.json`; `native-controlled-angle-closed-loop-smoke.json`; verifier gate details | Partial pass | Native response, all three pre-closed-loop controlled-angle undertravel attempts, and the closed-loop attempt sent cleanup successfully. Smoke-level final-zero / bounded FFB cleanup remains unproven until simulator FFB smoke exists. |
| Native actuator response proven | `native-actuator-visible-smoke.json`; `native-response-verification.json`; pre-output readiness | Pass | Response gate records about 0.181 degrees above the 0.100 degree response threshold. |
| Native visible motion proven | `native-visible-verification.json`; current `verify-bundle --stage native-visible-ready` | Missing | Current verifier fails `native_actuator_visible_smoke`; all controlled-angle attempts, including the closed-loop attempt, remain below the 1 degree visible threshold. |
| Attempt-03 profile planned without output | `native-pidff-effect-lifecycle-plan.json`; `native-controlled-angle-attempt-03-preflight.json` | Pass as no-output preparation | Profile `bounded-pidff-effect-lifecycle-v1` is preflighted with `dry_run=true`, zero writes, and `hardware_output_enabled=false`. It authorizes no output. |
| Attempt-03 authorization exists | `native-controlled-angle-attempt-03-authorization.json` | Pass, consumed | Exact command-bound authorization was recorded and consumed by the single attempt-03 output receipt; it authorizes no further output. |
| Attempt-03 output receipt exists | `native-controlled-angle-attempt-03-smoke.json` | Pass as safe failed evidence | Attempt 03 sent four bounded PIDFF effect-lifecycle writes, reached 0.181277 degrees, timed out before target, sent final Stop All, stayed post-stop stable, and recorded zero write errors. It is not visible-motion proof. |
| Attempt-03 failure analysis exists | `native-controlled-angle-attempt-03-failure-analysis.json` | Pass as no-output classification | Analysis classifies safe undertravel in the same response band and keeps rerun, force escalation, native-visible, smoke-ready, and release-ready claims false. |
| Standard PIDFF path diagnosis exists | `native-pidff-standard-path-diagnosis.json`; [Moza R5 Standard PIDFF Path Diagnosis](moza-r5-standard-pidff-path-diagnosis.md) | Pass as no-output architecture diagnosis | Diagnosis classifies `standard_pidff_controlled_angle_path_ineffective_in_current_r5_mode`, preserves all three controlled-angle receipts, keeps `planned_next_output.allowed=false`, and identifies no-output vendor-specific protocol investigation as next. |
| Closed-loop controlled-angle path exists | `native-controlled-angle-closed-loop-preflight.json`; `native-controlled-angle-closed-loop-authorization.json`; `native-controlled-angle-closed-loop-smoke.json`; `native-controlled-angle-closed-loop-failure-analysis.json` | Pass as safe failed evidence | The closed-loop attempt used `closed-loop-pidff-angle-v1`, wrote 672 bounded PIDFF reports with zero write errors, sent final Stop All/final zero, stayed post-stop stable, and timed out before target at `angle_delta_degrees=0.13183794918745662`. It is not visible-motion proof and authorizes no further output. |
| Vendor-authority attempt exists | `vendor-authority-authorization.json`; `vendor-authority-smoke-dry-run.json`; `vendor-authority-attempt.json` | Pass as exact one-frame non-claiming evidence | The attempt consumed a fresh precondition-bound authorization, sent only the hash-bound `estop_set_ffb` frame once, closed `hardware_output_authorized=false`, and kept native-control/native-visible/smoke-ready/release-ready claims false. |
| Post-authority PIDFF comparison exists | `vendor-post-authority-pidff-smoke.json`; `vendor-post-authority-pidff-response.json`; [Moza R5 Post-Authority PIDFF Response](moza-r5-post-authority-pidff-response.md) | Pass as no-output comparison diagnosis | The comparison classifies `post_authority_pidff_response_regressed`: baseline `0.18127718013275285` degrees versus post-authority `0.032959487296864154` degrees. It authorizes no retry and does not claim native-visible readiness. |
| Vendor protocol evidence review exists | `vendor-protocol-evidence-review.json`; `schemas/moza-vendor-protocol-evidence-review.schema.json`; artifact-index/bench-wizard `vendor_protocol_decode_priority` | Pass as no-output protocol review | The review confirms two of six passive sniff scenarios are complete, current summaries expose 7,863 checksum-valid `0x7E` USB CDC serial-frame candidates, 30 tuple IDs, and 159 bounded passive tuple sample frames, but no decoded semantic enable/output command. Tuple-registry coverage finds one read-only status match, 12 commandless tuple IDs, 17 unknown commanded tuple IDs, zero known write-like tuple matches, and `unknown_tuple_risk_class=unknown_do_not_send`. Tuple-frequency coverage ranks the top unknown commanded tuples as `0x5A/0x1B/0x00`, `0x5D/0x1B/0x01`, `0x25/0x19/0x01`, `0x25/0x19/0x02`, and `0x25/0x19/0x03`; artifact-index and bench-wizard surface this as decode priority with 30 sample fixture frames, 11 packet groups, three full-packet patterns, four repeated motifs, payload-shape navigation, low-confidence semantic hypotheses, and a semantic correlation plan only, not sendability. The correlation plan reports two targets, keeps all claim flags false, and prioritizes `pit-house-setting-change` as the next passive capture. The completed Pit House summaries retain only a two-packet residual payload export gap, now preserved as packet/frame locator examples with output/sendability claims false. No future output is allowed without decoded protocol evidence plus a reviewed plan and fresh exact authorization. |
| Passive tuple observed decoder coverage exists | `crates/hid-moza-protocol/src/serial/frame.rs`; `crates/hid-moza-protocol/tests/vendor_passive_tuple_samples.rs`; `vendor-protocol-evidence-review.json` | Pass as no-output fixture decode regression | The protocol crate validates all 30 decode-candidate passive sample frames as checksum-valid observed `0x7E` frame shapes while keeping `0x5A/0x1B/0x00`, `0x5D/0x1B/0x01`, and `0x25/0x19/0x01..03` unknown to the semantic registry. The same regression preserves packet-local pair, triad, and combined five-frame group morphology plus payload-shape hints: empty `0x5A` samples and zero-filled `0000` samples for `0x5D`/`0x25`. It also pins pattern-only hypotheses for those groups as low-confidence decode questions and pins the correlation plan as non-sendable passive capture navigation. The semantic fixture decoder still rejects those tuples as unknown commands. No tuple is sendable and no hardware/output/readiness claim changes. |
| Pit House compatibility navigation is current | `pit-house-availability.json`, `artifact-index`, and `bench-wizard` `pit_house_compatibility`; `ci/hardware/moza-r5/2026-05-13/index.md` Pit House Compatibility section | Pass as non-claiming navigation | The lane records `pit_house_available=true`, `availability_status=running_install_location_unknown`, `recorded_case_count=2/5`, `pit_house_coexistence_claimed=false`, `readiness_claim=false`, `blocks_native_control=false`, and `blocks_native_visible=false`. Missing direct/mode-change/firmware-page cases remain external smoke blockers. |
| Pit House coexistence proven | `pit-house-coexistence.json`; smoke-ready verifier | Missing | `pit-house-availability.json` is non-claiming availability evidence only. Coexistence matrix is not proven. |
| Simulator compatibility navigation is current | `pre-output-readiness.json`, `artifact-index`, and `bench-wizard` `simulator_compatibility`; `ci/hardware/moza-r5/2026-05-13/index.md` Simulator Compatibility section | Pass as non-claiming navigation | The lane records `recorded_artifact_count=0/2`, `simulator_telemetry_claimed=false`, `bounded_simulator_ffb_claimed=false`, `readiness_claim=false`, `blocks_native_control=false`, and `blocks_native_visible=false`. Missing telemetry and bounded FFB artifacts remain external smoke blockers. |
| Passive USB sniff navigation is current | `artifact-index` and `bench-wizard` `passive_sniff_navigation`; `ci/hardware/moza-r5/2026-05-13/index.md` Passive USB Sniffing section | Pass as non-claiming navigation | The lane records 2/6 passive sniff scenarios with non-claiming summaries: `pit-house-open-idle` and `pit-house-full-controls`. The remaining four scenarios are plan-only and still `partial_or_unaccepted` until pcap receipts and summaries exist. `readiness_claim=false`, `blocks_native_control=false`, `blocks_native_visible=false`, and `blocks_smoke_ready=false`. |
| Simulator telemetry proof exists | `simulator-telemetry-proof.json`; smoke-ready verifier | Missing | No real simulator telemetry receipt exists. |
| Bounded sim-to-Moza FFB smoke exists | `simulator-ffb-smoke.json`; smoke-ready verifier | Missing | No bounded simulator FFB receipt or output log exists. |
| Smoke-ready verification passes | `smoke-ready-verification.json`; current smoke-ready verifier state | Missing | Current smoke-ready verification fails native visible motion, Pit House coexistence, simulator telemetry, and simulator FFB. |
| Smoke-ready manifest promotion and audit exist | `manifest-promotion-smoke-ready.json`; `lane-audit-smoke-ready.json` | Missing | These cannot be created until smoke-ready verification passes. |
| Release-ready remains unclaimed | `manifest.json`; support/readiness receipts | Pass | `release_ready=false`; this lane does not claim release readiness. |
| High torque remains unclaimed | `manifest.json`; controlled-angle receipts; verifier details | Pass | `high_torque_validated=false`; controlled-angle receipts record no high torque. |
| Direct report `0x20`, serial config, firmware, and DFU remain out of scope | Controlled-angle receipts; verifier details; bench wizard | Pass | Current artifacts forbid direct `0x20`, high torque, serial config, firmware, and DFU. |

## Current Gate Evidence

The current no-output verifier result for `native-visible-ready` is expected to
fail:

```text
success=false
failed_gates=1
failed gate: native_actuator_visible_smoke
next_commands=[]
no_hid_device_opened=true
no_ffb_writes=true
```

The current `bench-wizard` result is diagnostic only:

```text
frontier=closed_loop_undertravel_recorded
highest_passing_stage=native_response_ready
next_required_stage=native_visible_ready
hardware_output_authorized=false
authorization_receipt_created=false
attempt_03.authorization=native-controlled-angle-attempt-03-authorization.json consumed
attempt_03.output=native-controlled-angle-attempt-03-smoke.json safe_undertravel
attempt_03.analysis=native-controlled-angle-attempt-03-failure-analysis.json complete_no_output
closed_loop.authorization=native-controlled-angle-closed-loop-authorization.json consumed
closed_loop.output=native-controlled-angle-closed-loop-smoke.json safe_undertravel
closed_loop.analysis=native-controlled-angle-closed-loop-failure-analysis.json complete_no_output
vendor_authority.attempt=vendor-authority-attempt.json consumed_non_claiming
post_authority_pidff.classification=post_authority_pidff_response_regressed
post_authority_pidff.baseline_delta_degrees=0.18127718013275285
post_authority_pidff.post_delta_degrees=0.032959487296864154
post_authority_pidff.delta_change_degrees=-0.1483176928358887
next_operator_step=post_authority_pidff_response_comparison_recorded
```

The current input-role semantic evidence remains diagnostic:

```text
input_semantic_mapping_complete=false
semantic_status=partial_generic_aux_mapping
semantic_mapping_complete=false
semantic_candidate_count=2
ambiguous_semantic_candidate_count=0
unproven_required_role_count=1
readiness_claim=false
```

The current external-compatibility navigation is also diagnostic only:

```text
pit_house_compatibility.pit_house_available=true
pit_house_compatibility.availability_status=running_install_location_unknown
pit_house_compatibility.recorded_case_count=2
pit_house_compatibility.required_case_count=5
pit_house_compatibility.pit_house_coexistence_claimed=false
pit_house_compatibility.blocks_native_control=false
pit_house_compatibility.blocks_native_visible=false
simulator_compatibility.recorded_artifact_count=0
simulator_compatibility.required_artifact_count=2
simulator_compatibility.simulator_telemetry_claimed=false
simulator_compatibility.bounded_simulator_ffb_claimed=false
simulator_compatibility.blocks_native_control=false
simulator_compatibility.blocks_native_visible=false
passive_sniff_navigation.recorded_scenario_count=2
passive_sniff_navigation.required_scenario_count=6
passive_sniff_navigation.readiness_claim=false
passive_sniff_navigation.blocks_native_control=false
passive_sniff_navigation.blocks_native_visible=false
passive_sniff_navigation.blocks_smoke_ready=false
```

The current smoke-ready state is incomplete:

```text
failed gates:
- native_actuator_visible_smoke
- pit_house_coexistence
- simulator_telemetry
- simulator_ffb_bounded
```

## Missing Work

1. Resolve the SR-P clutch semantic mapping if product-quality input topology
   requires role-specific clutch naming before smoke-ready.
2. Capture the remaining passive USB sniff scenarios, generate non-claiming
   receipts and summaries, decode vendor reports, map report IDs, identify
   enable/gain/mode handshakes, and design a reviewed plan.
3. Post-authority PIDFF response review and no-output protocol analysis before
   any future output family.
4. A reviewed future-output plan only if new protocol evidence justifies a new
   output family.
5. If a future receipt passes, native-visible verifier, manifest promotion, and
   lane audit.
6. Pit House coexistence matrix.
7. Real simulator telemetry proof.
8. Bounded simulator FFB smoke receipt.
9. Smoke-ready verification, manifest promotion, and lane audit.

## Claim Boundary

This audit does not move the lane. It records that:

- `native_response_ready` is proven.
- `native_visible_ready` is not proven.
- `smoke_ready` is not proven.
- `release_ready` is false.
- No new authorization or hardware-output permission exists.
