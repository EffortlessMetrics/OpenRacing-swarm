# Moza R5 Artifact Checklist

This checklist maps the Moza R5 bring-up objective to the concrete files, commands, and verifier gates that must exist before the lane can be called real-hardware smoke ready. It is an audit aid, not evidence by itself.

## Completion Rule

The lane is complete only when a dated directory under `ci/hardware/moza-r5/<date>/` contains real receipts for Steven's R5 + KS/ES + SR-P + HBP stack, `wheelctl moza verify-bundle --stage smoke-ready` passes, `wheelctl moza promote-manifest --stage smoke-ready` updates `manifest.json`, and `wheelctl moza audit-lane --stage smoke-ready` passes. `release_ready` and `high_torque_validated` must remain `false`.

Passing unit tests, schema validation, placeholder templates, or a complete artifact list is not enough. Treat any missing receipt, stale receipt, failed gate, hardware mismatch, or uncertain source as not complete.

## Current Repo State

The repository contains the lane scaffolding, docs, manifest schema, CLI receipt producers, parser replay harness, service readiness overlay, and verifier gates. It does not contain a dated real-hardware lane. The checked-in `ci/hardware/moza-r5/` directory intentionally contains only the lane README and `manifest.schema.json`; real run artifacts belong under a dated child directory.

## Manifest States

The only valid lane completion states are:

- `not_started`
- `passive_capture_ready`
- `zero_torque_ready`
- `real_hardware_smoke_ready`

## Prompt-To-Artifact Checklist

| Objective requirement | Required producer and artifact | Verifier or audit evidence | Current checked-in state |
|-----------------------|--------------------------------|----------------------------|--------------------------|
| Name Steven's target stack exactly | `wheelctl moza init-lane` writes `manifest.json` with R5, KS, ES, SR-P, HBP, Windows, HID-only transport | `manifest.schema.json`, `verify-bundle --stage passive`, `promote-manifest` | Schema and starter docs exist; no dated hardware manifest is checked in |
| Keep research separate from validation | `docs/hardware/moza-validation-matrix.md` lists researched coverage as source/code verified only | Matrix row changes only after receipts and promotion | Matrix says `Not started`, hardware No, simulator No |
| Passive device enumeration | `wheelctl device list --hid-observe-only` -> `device-list.json`; `wheelctl moza probe` -> `moza-probe.json`; `hid-capture list --vendor 0x346E` -> `hid-list.json`; `wheelctl hardware doctor` -> `hardware-doctor.json` | Passive verifier requires the manifest-declared topology endpoints; the primary Moza path is R5 hub evidence, with standalone SR-P/HBP required only for direct-plug topology. `hardware-doctor.json` is a redacted observe-only platform/PnP safety receipt, not output evidence. | Commands and gates exist; no real devices observed in repo |
| Descriptor capture | `wheelctl moza descriptor` -> `descriptor.json`; use `--device <r5> --report-descriptor-hex`, `--report-descriptor-hex-file`, or `--report-descriptor-bin-file` when Windows cannot expose bytes | Passive verifier checks descriptor source, CRC, serial presence, manufacturer, interface, usage, input lengths, output report `0x20` with 8-byte report length, feature reports `0x03`/`0x11` | Command and docs exist; no real descriptor receipt exists |
| Passive input capture | `wheelctl moza capture-input` writes `captures/r5-idle.jsonl`, isolated through-R5 role captures, `captures/r5-aggregated-idle-after-controls.jsonl`, `captures/ks-controls.jsonl`, and `captures/es-controls.jsonl`; `wheelctl moza analyze-capture`, `wheelctl moza analyze-lane`, and `wheelctl moza sync-role-status` diagnose raw byte/word movement before assigning role semantics | `validate-captures` checks parser success, expected product IDs, role evidence movement, rim controls, per-line no-output assertions, and topology-declared optional direct-plug captures; `analyze-lane` reports per-role `semantic_status` as diagnostic evidence only, and `sync-role-status` updates manifest status fields without promoting receipts | Capture command and verifier exist; no real captures exist |
| Parser fixture promotion | `wheelctl moza validate-captures` -> `parser-fixture-validation.json`; `wheelctl moza promote-fixtures` -> `fixture-promotion.json` and sanitized fixtures under `crates/hid-moza-protocol/fixtures/...` | Passive verifier checks fixture coverage, sanitization, PID consistency, and parser replay equality from lane-relative fixtures or repo-relative `crates/hid-moza-protocol/fixtures/...` paths; `cargo test -p racing-wheel-hid-moza-protocol promoted_capture_fixtures_replay_through_moza_parser` also covers promoted fixtures | Replay harness exists with synthetic smoke fixture; no real promoted Moza fixture set exists |
| Passive promotion | `wheelctl moza verify-bundle --stage passive` -> `passive-verification.json`; `wheelctl moza promote-manifest --stage passive` -> `manifest-promotion-passive.json` | `wheelctl moza audit-lane --stage passive`; manifest moves to `passive_capture_ready` without hardware/simulator claims | Commands and gates exist; no real passive verification receipt exists |
| Zero torque | `wheelctl moza zero --repeat 100 --hz 1000` -> `zero-torque-proof.json` | Zero verifier checks same-lane `receipt_path`, valid timestamp, real HID opened, only report `0x20` zero payload, no high torque, no feature reports, exact write accounting, final zero | Command and gate exist; no real zero receipt exists |
| Watchdog safety | `wheelctl moza watchdog-proof` -> `watchdog-proof.json` | Zero-stage verifier checks same-lane `receipt_path`, valid timestamp, timeout, watchdog fault, zero output, final zero, no non-zero payloads | Command and gate exist; no real watchdog receipt exists |
| Disconnect safety | `wheelctl moza disconnect-proof --confirm-disconnect-test` -> `disconnect-proof.json` | Zero-stage verifier checks same-lane `receipt_path`, valid timestamp, disconnect observation, write accounting, final-zero attempt, safe failure if handle is gone | Command and gate exist; no real disconnect receipt exists |
| Zero-stage promotion | `wheelctl moza verify-bundle --stage zero` -> `zero-verification.json`; `wheelctl moza promote-manifest --stage zero` -> `manifest-promotion-zero.json` | `wheelctl moza audit-lane --stage zero`; manifest moves to `zero_torque_ready` without hardware/simulator claims | Commands and gates exist; no real zero-stage promotion exists |
| Safe staged init | `wheelctl moza init --mode off` -> `init-off.json`; `wheelctl moza init --mode standard` -> `init-standard.json` | Init gate requires same-lane `receipt_path`, valid timestamp, exactly feature report `0x03` then `0x11`, no `0x02`, no `0x20`, no serial/firmware commands | Command and gate exist; no real init receipts exist |
| Low torque | `wheelctl moza torque-test --confirm-low-torque --max-percent 2` -> `low-torque-proof.json` | Low-torque preflight requires `--lane` and same-lane zero/init/descriptor artifacts before HID initialization; gate requires same-lane zero proof, off/standard init summaries with timestamp/CRC matches, descriptor trust or explicit override, direct mode gate, bounded payload recomputation, abort-to-zero | Command and gate exist; no real low-torque receipt exists |
| Pit House coexistence | `wheelctl moza pit-house-observation`, `wheelctl moza pit-house-case`, and `wheelctl moza pit-house-proof` -> `pit-house-coexistence.json` plus five case artifacts | Smoke-ready verifier requires all five cases, non-notes evidence artifacts, source receipt links, direct-mode block or ack, mode-mismatch fail-safe, firmware-page high-risk refusal | Commands and gates exist; no Pit House evidence exists |
| Service readiness status | `wheeld --hardware-lane ...`, `wheelctl moza status` -> `moza-status.json`, `wheelctl device status --moza-lane` -> `device-status.json`, `wheelctl support-bundle --device <r5> --moza-lane` -> `support-bundle.json` | Service-status gate requires matching R5 PID, descriptor metadata, torque readiness disabled, no FFB/serial/firmware/DFU, and support-bundle artifact/readiness diagnostics that do not overclaim the current lane | Service overlay and gates exist; no real service receipts exist |
| Pre-output readiness | `wheelctl moza pre-output-readiness` -> `pre-output-readiness.json` | Read-only ledger separates `ready_for_zero_torque` from `ready_for_ffb`, checks passive verification, passive audit, service/status no-output receipts, zero-stage receipts, and bounded-FFB prerequisites without opening HID devices or suggesting output while passive is red | Command exists; no real readiness receipt is checked in |
| Simulator telemetry | `wheelctl telemetry record` creates a recorder artifact; `wheelctl moza simulator-telemetry-proof` -> `simulator-telemetry-proof.json` | Telemetry gate requires telemetry-only operation, normalized snapshots, recorder provenance, no hardware output, no faults | Commands and gate exist; no real simulator telemetry receipt exists |
| Bounded simulator FFB smoke | `wheelctl moza simulator-ffb-smoke` -> `simulator-ffb-smoke.json` plus output log | Smoke gate requires hardware prerequisites bound by same-lane prerequisite artifact CRC/timestamp summaries before writer start, direct mode gate, high torque false, watchdog active, bounded non-zero output, final zero, stop/pause/game-exit/mode-mismatch clear events, telemetry and lane-bound writer provenance | Command and gate exist; no real simulator FFB receipt exists |
| Smoke-ready promotion | `wheelctl moza verify-bundle --stage smoke-ready` -> `smoke-ready-verification.json`; `wheelctl moza promote-manifest --stage smoke-ready` -> `manifest-promotion-smoke-ready.json` | `wheelctl moza audit-lane --stage smoke-ready`; manifest moves to `real_hardware_smoke_ready`, `hardware_validated=true`, `simulator_validated=true`, `high_torque_validated=false`, `release_ready=false` | Commands and gates exist; no real smoke-ready promotion exists |

## Required Artifact Names

The support bundle artifact index and manifest schema must stay aligned with this list:

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
- Did low torque require explicit confirmation, direct-mode gate evidence, high torque false, and final zero?
- Do Pit House cases include observation artifacts and source receipt links rather than notes-only evidence?
- Does simulator telemetry prove hardware output disabled before simulator FFB smoke proves bounded output enabled?
- Does simulator FFB smoke end with final zero and include stop, pause, game-exit, and mode-mismatch clear records?
- Do `moza-status.json`, `device-status.json`, and `support-bundle.json` keep torque readiness disabled while reporting diagnostic lane context?
- Did `promote-manifest --stage smoke-ready` keep `release_ready=false` and `high_torque_validated=false`?
