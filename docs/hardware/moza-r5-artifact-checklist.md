# Moza R5 Artifact Checklist

This checklist maps the Moza R5 bring-up objective to the concrete files, commands, and verifier gates that must exist before the lane can be called real-hardware smoke ready. It is an audit aid, not evidence by itself.

## Completion Rule

The native OpenRacing control foundation is complete only when a dated directory under `ci/hardware/moza-r5/<date>/` contains real passive, zero, init, service/status, bounded low-torque, steering-angle stream, and native actuator-profile smoke receipts for Steven's R5 + KS/ES + SR-P + HBP stack, `wheelctl moza verify-bundle --stage openracing-control-ready` passes, `wheelctl moza promote-manifest --stage openracing-control-ready` updates `manifest.json`, and `wheelctl moza audit-lane --stage openracing-control-ready` passes. Native actuator response and native visible motion are separate native stages after that foundation. Full smoke-ready remains a separate external-compatibility claim that requires `wheelctl moza verify-bundle --stage smoke-ready`, smoke-ready manifest promotion, and smoke-ready audit. `release_ready` and `high_torque_validated` must remain `false`.

Passing unit tests, schema validation, placeholder templates, or a complete artifact list is not enough. Treat any missing receipt, stale receipt, failed gate, hardware mismatch, or uncertain source as not complete.

## Current Repo State

The repository contains the lane scaffolding, docs, manifest schema, CLI receipt producers, parser replay harness, service readiness overlay, verifier gates, and a dated real-hardware lane at `ci/hardware/moza-r5/2026-05-13/`. That lane has passed passive verification, zero-stage verification/audit, staged init, bounded PIDFF low torque, OpenRacing-control promotion, and native-response promotion for Steven's R5 + KS/ES + SR-P + HBP stack, with descriptor trust, parser fixture validation, fixture promotion, observe-only status receipts, zero-torque proof, watchdog proof, bounded disconnect proof, `init-off.json`, `init-standard.json`, `low-torque-proof.json`, steering stream, native actuator profile, and native actuator response recorded.

The same lane is not smoke-ready complete: the 1 percent native actuator-profile receipt proves the output rail and cleanup path, and both the preserved 5 percent / 2000 ms response-only receipt and the 2026-05-17 bounded-shaped PIDFF micro-profile receipt record a measurable 0.181277 degree steering response with successful writes and Stop All cleanup. Those receipts are actuator-response evidence, not visible-motion evidence, because they remain below the 1 degree visible-motion threshold. Any later visible-motion attempt must first record a non-claiming review artifact, preserve the response receipt, record the intended next profile, and require a fresh bench-clear for the exact next output command. The visible-smoke command supports `constant-low-force` and `bounded-shaped-pidff-micro-profile` through `pidff-bounded-effect`, capped at 5 percent for up to 2000 ms; any higher or longer profile needs a separate software and safety plan. The checked-in follow-up plan records the consumed 2026-05-17 authorization, `planned_next_output.allowed=false`, a blocked 5 percent / 3000 ms retry, a rejected 5 percent / 30000 ms dwell, and a later 90 degree right/left controlled-angle target that requires staged smaller-angle receipts. The non-claiming controlled-angle plan surface remains `native-controlled-angle-plan.json`. The controlled-angle writer is implemented, and the 2026-05-18 real 1 degree attempt is preserved as `native-controlled-angle-smoke.json`: it sent five bounded PIDFF writes, sent final Stop All, stayed post-stop stable, and timed out before target after about 0.181 degrees of movement. Treat that as failed visible-motion evidence and profile-diagnosis input, not permission to rerun; the [controlled-angle analysis](moza-r5-controlled-angle-analysis.md) records the safe-undertravel classification. The reviewed retry used `native-controlled-angle-retry-preflight.json`, `native-controlled-angle-retry-authorization.json`, and `native-controlled-angle-retry-smoke.json`; it also failed safely in the same 0.181 degree response band with 33 successful PIDFF writes, final Stop All, and stable post-stop behavior. The retry analysis is recorded in `native-controlled-angle-retry-failure-analysis.json`; the no-output PIDFF semantics diagnosis is recorded in `native-pidff-semantics-diagnosis.json` and classifies the evidence as `same_response_band_despite_micro_step_replay`. The no-output [PIDFF lifecycle trace](moza-r5-pidff-lifecycle-trace.md) is recorded in `native-pidff-lifecycle-trace.json`; it decodes the preserved command logs, classifies the retry as repeated setup/start/Stop-All cycles, compares both attempts, and still authorizes no third attempt. `native-pidff-effect-lifecycle-plan.json` names `bounded-pidff-effect-lifecycle-v1`, implemented for software preflight and attempt-specific exact authorization binding only; it still authorizes no output. No third attempt, force increase, dwell extension, or larger-angle target is authorized. `wheelctl moza authorize-visible-output` remains the no-output path for the older actuator-visible command, not the controlled-angle rerun path. Pit House coexistence, simulator telemetry, simulator FFB, smoke-ready promotion, and smoke-ready audit are also still missing. Pit House and SimHub are external compatibility work, not prerequisites for native OpenRacing movement control; `pit-house-availability.json` may honestly record that Pit House is absent, but it is not coexistence proof.

## Manifest States

The only valid lane completion states are:

- `not_started`
- `passive_capture_ready`
- `zero_torque_ready`
- `openracing_control_ready`
- `native_response_ready`
- `native_visible_ready`
- `real_hardware_smoke_ready`

## Prompt-To-Artifact Checklist

| Objective requirement | Required producer and artifact | Verifier or audit evidence | Current checked-in state |
|-----------------------|--------------------------------|----------------------------|--------------------------|
| Name Steven's target stack exactly | `wheelctl moza init-lane` writes `manifest.json` with R5, KS, ES, SR-P, HBP, Windows, HID-only transport | `manifest.schema.json`, `verify-bundle --stage passive`, `promote-manifest` | Real `2026-05-13` manifest names R5 PID `0x0004`, KS/ES, SR-P, HBP, Windows, HID-only transport, and `native_response_ready` |
| Keep research separate from validation | `docs/hardware/moza-validation-matrix.md` lists researched coverage as source/code verified only | Matrix row changes only after receipts and promotion | Matrix says bounded PIDFF low torque is proven; simulator validation, high torque, and release readiness remain unclaimed |
| Passive device enumeration | `wheelctl device list --hid-observe-only` -> `device-list.json`; `wheelctl moza probe` -> `moza-probe.json`; `hid-capture list --vendor 0x346E` -> `hid-list.json`; `wheelctl hardware doctor` -> `hardware-doctor.json` | Passive verifier requires the manifest-declared topology endpoints; the primary Moza path is R5 hub evidence, with standalone SR-P/HBP required only for direct-plug topology. `hardware-doctor.json` is a redacted observe-only platform/PnP safety receipt, not output evidence. | Real `2026-05-13` observe-only device, probe, HID list, and hardware doctor receipts exist for the R5 hub endpoint |
| Descriptor capture | `wheelctl moza descriptor` -> `descriptor.json`; use `--device <r5> --report-descriptor-hex`, `--report-descriptor-hex-file`, or `--report-descriptor-bin-file` when Windows cannot expose bytes. Acceptable byte sources include USBTreeView raw HID report descriptor hex, USBPcap/Wireshark enumeration capture of the HID Report Descriptor response, and Linux sysfs `report_descriptor` bytes. | Passive verifier checks descriptor source, CRC, serial presence, manufacturer, interface, usage, descriptor-derived input lengths, and the observed output/feature report metadata for the selected PID. Summaries, `wDescriptorLength`, Windows HidP KDR/preparsed blobs, driver replacement, firmware/update flows, output reports, feature reports, serial config, and DFU are not descriptor evidence. | Command and docs exist; live R5 V1 descriptor receipt proves PIDFF Device Control `0x0C` but not direct report `0x20` |
| Passive input capture | `wheelctl moza capture-input` writes `captures/r5-idle.jsonl`, isolated through-R5 role captures, `captures/r5-aggregated-idle-after-controls.jsonl`, `captures/ks-controls.jsonl`, and `captures/es-controls.jsonl`; `wheelctl moza analyze-capture`, `wheelctl moza analyze-lane`, and `wheelctl moza sync-role-status` diagnose raw byte/word movement before assigning role semantics | `validate-captures` checks parser success, expected product IDs, role evidence movement, rim controls, per-line no-output assertions, and topology-declared optional direct-plug captures; `analyze-lane` reports per-role `semantic_status` as diagnostic evidence only, and `sync-role-status` updates manifest status fields without promoting receipts | Steven's `2026-05-13` lane has real R5, KS, ES, SR-P, and HBP passive captures; passive verifier gates pass |
| Parser fixture promotion | `wheelctl moza validate-captures` -> `parser-fixture-validation.json`; `wheelctl moza promote-fixtures` -> `fixture-promotion.json` and sanitized fixtures under `crates/hid-moza-protocol/fixtures/...` | Passive verifier checks fixture coverage, sanitization, PID consistency, and parser replay equality from lane-relative fixtures or repo-relative `crates/hid-moza-protocol/fixtures/...` paths; `cargo test -p racing-wheel-hid-moza-protocol promoted_capture_fixtures_replay_through_moza_parser` also covers promoted fixtures | Real `2026-05-13` parser fixture validation and promotion receipts exist; fixture replay is covered by the passive verifier |
| Passive promotion | `wheelctl moza verify-bundle --stage passive` -> `passive-verification.json`; `wheelctl moza promote-manifest --stage passive` -> `manifest-promotion-passive.json` | `wheelctl moza audit-lane --stage passive`; manifest moves to `passive_capture_ready` without hardware/simulator claims | Real `2026-05-13` passive verification, passive manifest promotion, and passive audit receipts pass |
| Zero torque | `wheelctl moza zero --strategy pidff-stop-all --repeat 100 --hz 1000` -> `zero-torque-proof.json`; use `--strategy direct-report-0x20` only when descriptor metadata proves direct report `0x20` | Zero verifier checks same-lane `receipt_path`, valid timestamp, real HID opened, selected zero-output strategy, no high torque, no feature reports, no non-zero torque, exact write accounting, final zero | Real `2026-05-13` PIDFF Stop All zero-torque receipt exists and passes |
| Watchdog safety | `wheelctl moza watchdog-proof` -> `watchdog-proof.json` | Zero-stage verifier checks same-lane `receipt_path`, valid timestamp, timeout, watchdog fault, zero output, final zero, no non-zero payloads | Real `2026-05-13` watchdog receipt exists and passes |
| Disconnect safety | `wheelctl moza disconnect-proof --confirm-disconnect-test` -> `disconnect-proof.json`; operator starts from a connected R5, unplugs USB during the `--max-duration-ms` window, and leaves it unplugged until the command exits | Zero-stage verifier checks same-lane `receipt_path`, valid timestamp, disconnect observation, write accounting, final-zero attempt, safe failure if handle is gone; later stages must re-enumerate before staged init or torque work | Real `2026-05-13` bounded disconnect receipt exists and passes; final zero was attempted and failed safely after the HID handle was gone |
| Zero-stage promotion | `wheelctl moza verify-bundle --stage zero` -> `zero-verification.json`; `wheelctl moza promote-manifest --stage zero` -> `manifest-promotion-zero.json` | `wheelctl moza audit-lane --stage zero`; manifest moves to `zero_torque_ready` without hardware/simulator claims | Real `2026-05-13` zero verification, zero manifest promotion, and zero audit receipts pass |
| Safe staged init | `wheelctl moza init --mode off` -> `init-off.json`; `wheelctl moza init --mode standard` -> `init-standard.json` | Init gate requires same-lane `receipt_path`, valid timestamp, lane endpoint selector, descriptor-proven feature reports, no `0x02`, no `0x20`, no serial/firmware commands. Live R5 V1 receipts use the descriptor-backed mode-only `0x11` feature report; other lanes may require `0x03` then `0x11` only when their descriptor proves that feature shape. | Real `2026-05-13` off and standard init receipts exist and pass |
| Low torque | `wheelctl moza torque-test --confirm-low-torque --max-percent 2` -> `low-torque-proof.json`; live R5 V1 guidance uses `--strategy pidff-bounded-effect --max-percent 1 --duration-ms 150` while direct `0x20` remains unavailable | Generated guidance requires `--lane`, same-lane zero/init artifacts, and an explicit strategy. Direct `direct_report_0x20` requires trusted direct report `0x20` metadata and a same-lane direct zero proof; PIDFF `pidff_bounded_effect` requires same-lane PIDFF Stop All zero proof, descriptor-proven PIDFF Device Control metadata, descriptor-proven PIDFF effect reports, explicit effect setup proof, an R5-shaped Set Effect encoder, exact endpoint selector, bounded nonzero PIDFF writes, no direct `0x20`, and final Stop All cleanup. | Real `2026-05-13` PIDFF bounded low-torque receipt exists at 1 percent / 150 ms and passes; direct `0x20` remains unavailable and unclaimed |
| Native steering feedback | `wheelctl moza steering-stream-proof --device <r5> --lane ci/hardware/moza-r5/YYYY-MM-DD --duration-ms 5000 --jsonl-out ci/hardware/moza-r5/YYYY-MM-DD/steering-angle-stream.jsonl --json-out ci/hardware/moza-r5/YYYY-MM-DD/steering-angle-stream-proof.json` | OpenRacing-control verifier requires same R5 endpoint, monotonic steering samples, angle units/baseline, and no output/feature/FFB/serial/firmware actions | Real `2026-05-13` steering stream receipt exists and passes without output/feature/FFB/serial/firmware actions |
| Native actuator profile | `wheelctl moza actuator-profile-smoke --confirm-actuator-profile --max-percent 1` -> `native-actuator-profile-smoke.json` | OpenRacing-control verifier requires same R5 endpoint, bounded PIDFF strategy, no direct `0x20`, no high torque, final Stop All cleanup, and no serial/firmware/DFU actions | Real `2026-05-13` 1 percent native PIDFF actuator-profile receipt exists and passes; it does not claim visible motion |
| Native actuator response | `wheelctl moza actuator-visible-smoke --confirm-actuator-visible --max-percent 5` -> `native-actuator-visible-smoke.json`; existing response receipts remain valid evidence when they exceed the response noise floor | Native-response verifier requires same R5 endpoint, valid prior 1 percent actuator-profile and steering proofs, bounded PIDFF strategy, measured steering delta above the response threshold, final Stop All cleanup, no direct `0x20`, no high torque, and no feature/serial/firmware/DFU actions. It does not require Pit House, SimHub, simulator telemetry, simulator FFB, `success=true`, or `movement_observed=true` from the visible-motion receipt | Real `2026-05-13` 5 percent / 2000 ms receipts pass as response evidence: the preserved response-only receipt and current bounded-shaped receipt both record `angle_delta_degrees=0.18127718013275285`, successful writes, Stop All cleanup, and no direct `0x20`, feature, high-torque, serial, firmware, or DFU actions |
| Native visible motion | `wheelctl moza actuator-visible-smoke --confirm-actuator-visible --max-percent 5` -> `native-actuator-visible-smoke.json`, or `wheelctl moza controlled-angle-smoke --target-degrees 1 --profile bounded-pidff-micro-step-v2 --max-percent 5 --timeout-ms <=2000 --confirm-controlled-angle` after `wheelctl moza authorize-controlled-angle-output` records exact authorization -> `native-controlled-angle-smoke.json` or retry output `native-controlled-angle-retry-smoke.json` | Native-visible verifier requires the same safety/provenance as response plus `success=true`, `movement_observed=true`, and measured delta meeting the visible-motion threshold. It does not require Pit House, SimHub, simulator telemetry, or simulator FFB | Real `2026-05-13` 5 percent / 2000 ms response receipts, the first 2026-05-18 real 1 degree controlled-angle attempt, and the retry output `native-controlled-angle-retry-smoke.json` still fail the visible-motion gate: `movement_observed=false`, `angle_delta_degrees=0.18127718013275285` below the 1 degree threshold. The retry wrote PIDFF successfully 33 times, sent final Stop All, stayed post-stop stable, and timed out before target. Both authorizations are consumed and no next output is authorized; 5 percent / 3000 ms, 5 percent / 30000 ms, 30 degree, and 90 degree right/left are not authorized. `native-pidff-semantics-diagnosis.json` records that repeated micro-step writes did not increase movement, `native-pidff-lifecycle-trace.json` records the decoded repeated setup/start/Stop-All lifecycle, and `native-pidff-effect-lifecycle-plan.json` records the no-output profile plan. The next work is no-output attempt-03 dry-run/preflight for `bounded-pidff-effect-lifecycle-v1`; no visible-motion or smoke-ready success is claimed |
| OpenRacing control promotion | `wheelctl moza verify-bundle --stage openracing-control-ready` -> `openracing-control-verification.json`; `wheelctl moza promote-manifest --stage openracing-control-ready` -> `manifest-promotion-openracing-control.json` | `wheelctl moza audit-lane --stage openracing-control-ready`; manifest moves to `openracing_control_ready`, `hardware_validated=true`, `simulator_validated=false`, `release_ready=false` | Real `2026-05-13` OpenRacing-control verification, manifest promotion, and lane audit receipts pass; this stage deliberately excludes visible-motion, SimHub, Pit House, and simulator FFB compatibility claims |
| Native response promotion | `wheelctl moza verify-bundle --stage native-response-ready` -> `native-response-verification.json`; `wheelctl moza promote-manifest --stage native-response-ready` -> `manifest-promotion-native-response.json` | `wheelctl moza audit-lane --stage native-response-ready`; manifest moves to `native_response_ready`, `hardware_validated=true`, `simulator_validated=false`, `release_ready=false` | Real `2026-05-13` native-response verification, manifest promotion, and lane audit receipts pass; this stage deliberately excludes visible-motion, Pit House, SimHub, simulator telemetry, and simulator FFB compatibility claims |
| Native visible promotion | `wheelctl moza verify-bundle --stage native-visible-ready` -> `native-visible-verification.json`; `wheelctl moza promote-manifest --stage native-visible-ready` -> `manifest-promotion-native-visible.json` | `wheelctl moza audit-lane --stage native-visible-ready`; manifest moves to `native_visible_ready`, `hardware_validated=true`, `simulator_validated=false`, `release_ready=false` | Not claimed; the current response receipt remains non-claiming for visible motion |
| Pit House coexistence | `wheelctl moza pit-house-availability` -> optional `pit-house-availability.json`, then `wheelctl moza pit-house-evidence --require-match`, `wheelctl moza pit-house-observation`, `wheelctl moza pit-house-case`, and `wheelctl moza pit-house-proof` -> `pit-house-coexistence.json` plus process/window evidence and five case artifacts | Smoke-ready verifier requires all five cases, non-notes evidence artifacts, source receipt links, direct-mode block or ack, mode-mismatch fail-safe, firmware-page high-risk refusal. Availability is non-claiming and only prevents fabricated open-state guidance when Pit House is absent. Verifier-generated process/window evidence commands refuse to write mismatched snapshots, and `pit-house-observation` rejects wheelctl-generated snapshots that contradict the requested case. | Commands and gates exist; Pit House is not installed/running on the current bench, so no coexistence claim exists |
| Service readiness status | `wheeld --hardware-lane ...`, `wheelctl moza status` -> `moza-status.json`, `wheelctl device status --moza-lane` -> `device-status.json`, `wheelctl support-bundle --device <r5> --moza-lane` -> `support-bundle.json` | Service-status gate requires matching R5 PID, descriptor metadata, torque readiness disabled, no FFB/serial/firmware/DFU, and support-bundle artifact/readiness diagnostics that do not overclaim the current lane | Real `2026-05-13` status and support receipts exist and keep torque readiness disabled |
| Pre-output readiness | `wheelctl moza pre-output-readiness` -> `pre-output-readiness.json` | Read-only ledger separates `ready_for_zero_torque`, `ready_for_native_control`, `native_actuator_response_proven`, `native_visible_motion_proven`, `ready_for_external_compatibility`, and legacy `ready_for_ffb`; it checks passive verification, passive audit, service/status no-output receipts, descriptor-trusted zero-output strategy candidates, zero-stage receipts, OpenRacing control receipts, native response receipts, and external compatibility blockers without opening HID devices or suggesting output while passive is red | Real `2026-05-13` readiness receipt reports `ready_for_zero_torque=true`, `ready_for_native_control=true`, `native_actuator_response_proven=true`, `native_visible_motion_proven=false`, `ready_for_ffb=false`, `ready_for_external_compatibility=false`, PIDFF Device Control `0x0C` stop-all ready, direct `0x20` unavailable, and native visible motion plus simulator telemetry still blocking later FFB/external-compatibility claims |
| Simulator telemetry | `wheelctl telemetry record` creates a recorder artifact; `wheelctl moza simulator-telemetry-proof` -> `simulator-telemetry-proof.json` | Telemetry gate requires telemetry-only operation, normalized snapshots, recorder provenance, no hardware output, no faults | Commands and gate exist; no real simulator telemetry receipt exists |
| Bounded simulator FFB smoke | `wheelctl moza simulator-ffb-smoke` -> `simulator-ffb-smoke.json` plus output log | Smoke gate requires hardware prerequisites bound by same-lane prerequisite artifact CRC/timestamp summaries before writer start, strategy-specific descriptor trust, high torque false, watchdog active, bounded non-zero output, final zero / PIDFF Stop All, stop/pause/game-exit/mode-mismatch clear events, telemetry and lane-bound writer provenance. Live R5 V1 uses `--strategy pidff-bounded-effect`; direct report `0x20` remains verifier-distinct. | Command and gate exist; no real simulator FFB receipt exists |
| Smoke-ready promotion | `wheelctl moza verify-bundle --stage smoke-ready` -> `smoke-ready-verification.json`; `wheelctl moza promote-manifest --stage smoke-ready` -> `manifest-promotion-smoke-ready.json` | `wheelctl moza audit-lane --stage smoke-ready`; manifest moves to `real_hardware_smoke_ready`, `hardware_validated=true`, `simulator_validated=true`, `high_torque_validated=false`, `release_ready=false` | Commands and gates exist; no real smoke-ready promotion exists. The current smoke-ready verifier fails on native visible motion plus missing Pit House, simulator telemetry, and simulator FFB receipts |

## Lane Artifact Names

The support bundle artifact index should expose these lane artifacts. The manifest artifact map remains backward-compatible with existing dated lanes; new stored native-stage verification, promotion, and audit receipts are lane artifacts even when older manifests do not list dedicated artifact-map keys for them.

```text
manifest.json
device-list.json
moza-probe.json
hid-list.json
hardware-doctor.json
descriptor.json
captures
captures/r5-idle.jsonl
captures/r5-steering-sweep.jsonl
captures/r5-throttle-only-sweep.jsonl
captures/r5-brake-only-sweep.jsonl
captures/r5-clutch-only-sweep.jsonl
captures/r5-handbrake-only-sweep.jsonl
captures/r5-aggregated-idle-after-controls.jsonl
captures/ks-controls.jsonl
captures/es-controls.jsonl
parser-fixture-validation.json
fixture-promotion.json
passive-verification.json
manifest-promotion-passive.json
lane-audit-passive.json
zero-torque-proof.json
watchdog-proof.json
disconnect-proof.json
zero-verification.json
manifest-promotion-zero.json
lane-audit-zero.json
init-off.json
init-standard.json
moza-status.json
device-status.json
support-bundle.json
pre-output-readiness.json
low-torque-proof.json
steering-angle-stream-proof.json
native-actuator-profile-smoke.json
native-actuator-visible-smoke.json
openracing-control-verification.json
manifest-promotion-openracing-control.json
lane-audit-openracing-control.json
native-response-verification.json
manifest-promotion-native-response.json
lane-audit-native-response.json
native-visible-verification.json
manifest-promotion-native-visible.json
lane-audit-native-visible.json
pit-house-coexistence.json
simulator-telemetry-proof.json
simulator-ffb-smoke.json
smoke-ready-verification.json
manifest-promotion-smoke-ready.json
lane-audit-smoke-ready.json
```

## Required Commands

The runbook must keep these command families documented:

```text
wheelctl moza init-lane
wheelctl device list
wheelctl moza probe
hid-capture list --vendor 0x346E
wheelctl hardware doctor
wheelctl moza descriptor
wheelctl moza capture-input
wheelctl moza analyze-capture
wheelctl moza validate-captures
wheelctl moza promote-fixtures
wheelctl moza verify-bundle
wheelctl moza promote-manifest
wheelctl moza audit-lane
wheelctl moza init
wheelctl moza zero
wheelctl moza watchdog-proof
wheelctl moza disconnect-proof
wheelctl moza torque-test
wheelctl moza pit-house-observation
wheelctl moza pit-house-case
wheelctl moza pit-house-proof
wheelctl telemetry record
wheelctl moza simulator-telemetry-proof
wheelctl moza simulator-ffb-smoke
wheelctl moza status
wheelctl device status --moza-lane
wheelctl support-bundle --device <r5> --moza-lane
wheelctl moza pre-output-readiness
```

## Final Audit Questions

Before marking the lane complete, answer each question from actual files in `ci/hardware/moza-r5/<date>/`:

- Does every required artifact exist under one dated lane directory?
- Does every passive receipt identify the same R5 PID as `manifest.json`, plus any standalone SR-P/HBP endpoints only when the topology declares direct-plug coverage?
- Did every passive command declare no HID output, no serial config, and no firmware or DFU command?
- Do the real captures prove steering, pedals, clutch behavior, HBP, KS, and ES controls rather than only parser success?
- Did fixture promotion sanitize local HID paths and raw serial information?
- Did zero, watchdog, disconnect, and low-torque receipts come from real non-dry-run commands?
- Did zero proof pass before any non-zero torque receipt?
- Did low torque require explicit confirmation, strategy-specific output gate evidence, high torque false, and final zero or final PIDFF Stop All?
- Do Pit House cases include observation artifacts and source receipt links rather than notes-only evidence?
- Does simulator telemetry prove hardware output disabled before simulator FFB smoke proves bounded output enabled?
- Does simulator FFB smoke end with final zero and include stop, pause, game-exit, and mode-mismatch clear records?
- Do `moza-status.json`, `device-status.json`, and `support-bundle.json` keep torque readiness disabled while reporting diagnostic lane context?
- Did `promote-manifest --stage smoke-ready` keep `release_ready=false` and `high_torque_validated=false`?
