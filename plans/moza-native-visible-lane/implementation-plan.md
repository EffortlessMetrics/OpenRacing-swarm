# Moza native visible implementation plan

Status: active
Owner: hardware
Linked proposal: docs/proposals/OR-PROP-0001-moza-native-visible-lane.md
Linked specs:
- docs/specs/OR-SPEC-0001-moza-native-visible-lane.md
Linked ADRs: docs/adr/0009-hardware-validation-evidence-state-machine.md
Active goal: .openracing/goals/active.toml
Blocked handoff: plans/moza-native-visible-lane/handoff.md

## Current state

The Moza R5 lane at `ci/hardware/moza-r5/2026-05-13` is `native_response_ready`.
The artifact index records frontier
`closed_loop_undertravel_recorded`, highest passing stage
`native_response_ready`, next required stage `native_visible_ready`,
`native_actuator_response_proven=true`, `native_visible_motion_proven=false`,
and `release_ready=false`.

Four real controlled-angle output receipts are preserved. The first 1 degree
attempt sent five bounded PIDFF writes, the reviewed retry sent 33 bounded PIDFF
writes, attempt 03 sent four bounded PIDFF effect-lifecycle writes, and the
closed-loop attempt sent 672 bounded PIDFF writes recomputed from live
steering-angle error. All had zero write errors, final cleanup, and post-stop
stability. The first three stayed around 0.181277 degrees of steering delta; the
closed-loop attempt ended at `angle_delta_degrees=0.13183794918745662`. They
are useful safe undertravel evidence, not visible-motion proof.

`native-pidff-lifecycle-trace.json`,
`native-pidff-effect-lifecycle-plan.json`, and
`native-pidff-standard-path-diagnosis.json` record the no-output PIDFF
diagnosis. `native-controlled-angle-attempt-03-preflight.json` records the
software-only dry-run for `bounded-pidff-effect-lifecycle-v1`. The matching
`native-controlled-angle-attempt-03-authorization.json` is recorded and consumed,
`native-controlled-angle-attempt-03-smoke.json` records safe undertravel, and
`native-controlled-angle-attempt-03-failure-analysis.json` records the no-output
classification. No further hardware output is authorized.

`native-controlled-angle-closed-loop-preflight.json` records the no-output
software preflight for `closed-loop-pidff-angle-v1`. The matching
`native-controlled-angle-closed-loop-authorization.json` is recorded and
consumed, `native-controlled-angle-closed-loop-smoke.json` records safe
undertravel after 672 successful bounded PIDFF writes, and
`native-controlled-angle-closed-loop-failure-analysis.json` records the
no-output classification. No further hardware output is authorized.

The vendor-authority rail is recorded through one consumed `estop_set_ffb`
attempt and one post-authority PIDFF response comparison. The attempt sent only
the exact authorized frame `7E02461C0001F0`, consumed its authorization, and
closed `hardware_output_authorized=false`. The follow-up comparison at
`vendor-post-authority-pidff-response.json` classifies
`post_authority_pidff_response_regressed`: baseline `0.18127718013275285`
degrees, post-authority `0.032959487296864154` degrees, delta change
`-0.1483176928358887` degrees. This does not unlock native-visible motion or
authorize another output attempt.

`docs/hardware/moza-r5-completion-audit.md` maps the broader Moza lane objective
to concrete receipts and confirms that the objective is still incomplete:
native-visible, Pit House coexistence, simulator telemetry, bounded simulator
FFB, and smoke-ready promotion remain missing.

`ci/hardware/sniff/moza-r5/2026-05-13` now contains passive USB sniff artifacts
for Pit House, SimHub, and simulator protocol research. The Pit House
`open-idle` and `full-controls` scenarios have checked-in non-claiming plans,
receipts, classified summaries, and bundle manifests; raw pcapng and bundle ZIP
files remain local scratch artifacts. Remaining scenarios stay navigation-only
until matching pcap receipts and summaries exist.

The Pit House `open-idle` and `full-controls` summaries now extract and review
USB CDC payload frames from TShark `usbcom.data.*_payload` fields. The checked-in
protocol review records candidate host-to-device frame/report ID `0x7E`, 3,246
extracted host-to-device payload packets, 53,988 extracted host-to-device
payload bytes, and two remaining data-length packets without extracted payload
bytes. The extracted stream parses into 7,863 length-prefixed `0x7E` serial-frame
candidates with 7,863 valid checksums, zero checksum-invalid frames, and no
frame-shape decode gap. The review now compares the 30 distinct passive tuple
IDs against `fixtures/moza/r5/vendor-command-registry.json`: one tuple matches
the registry, `0x28/0x13/0x02` (`base_gain_get_overall_strength`), and it is
read-only status evidence only. The other 29 passive tuples are 12 commandless
tuple IDs and 17 unknown commanded tuple IDs. The same review now preserves
per-scenario tuple counts so the highest-frequency unknown commanded tuples are
visible before any semantic decode work: `0x5A/0x1B/0x00` appears 1,896 times,
`0x5D/0x1B/0x01` appears 1,894 times, and `0x25/0x19/0x01`,
`0x25/0x19/0x02`, and `0x25/0x19/0x03` each appear 624 times. This is
protocol-shape, registry-coverage, and frequency-prioritization navigation
only. The review also preserves 159 bounded passive tuple sample frames and 30
decode-candidate sample frames for the five highest-frequency unknown commanded
tuples. Artifact-index and bench-wizard now surface that frequency-ranked decode
priority and representative sample frames from the checked-in review receipt,
but this still does not decode an approved semantic enable command, make any
tuple sendable, authorize output, or promote native-visible readiness.
The protocol crate validates those samples as observed wire-shape fixtures and
now regression-checks their repeated packet-order hints: `0x5A/0x1B/0x00`
precedes `0x5D/0x1B/0x01`, and `0x25/0x19/0x02`,
`0x25/0x19/0x03`, then `0x25/0x19/0x01` repeat as an ordered triad. That is
sequence-shape evidence only, not a semantic decode or sendability claim. The
review now also records payload-shape morphology for those same 30
decode-candidate samples: the five top unknown commanded tuples have checksum
valid samples, remain `unknown_commanded`, and their sampled payloads are either
empty or `0000`. That payload-shape summary is fixture evidence only; empty or
zero-filled observed payloads do not make an unknown tuple semantic or sendable.
The review now also records packet-group morphology for the same samples: 11
packet groups, three full-packet patterns, and four repeated contiguous motifs,
including the repeated `0x5A/0x1B/0x00` -> `0x5D/0x1B/0x01` pair and repeated
`0x25/0x19/0x02` -> `0x25/0x19/0x03` -> `0x25/0x19/0x01` triad. That grouping
summary is still fixture evidence only and creates no semantic command decode,
registry promotion, output sendability, or hardware authorization.
The review now preserves low-confidence semantic hypotheses for those same
five highest-frequency unknown commanded tuples. The `0x5A/0x1B/*` and
`0x5D/0x1B/*` pair is classified as a
`session_or_status_keepalive_candidate`, and the `0x25/0x19/*` triad is
classified as a `base_status_or_mode_poll_candidate`. These are pattern-only
decode questions, not semantic command definitions: each tuple remains
`unknown_commanded`, non-sendable, and ineligible for registry promotion or
hardware output.
The review now also records a no-output semantic correlation plan for those
hypotheses. It groups the five tuple hypotheses into two correlation targets,
records that both are observed in the completed `pit-house-open-idle` and
`pit-house-full-controls` summaries, and names `pit-house-setting-change` as the
next passive capture priority before SimHub and simulator correlation gaps. The
plan is capture navigation only: `semantic_decode_claim=false`,
`registry_promotion_claim=false`, `output_sendability_claim=false`, and
`protocol_evidence_sufficient_for_output_plan=false`.
The `pit-house-setting-change` sniff plan now pins the scenario-specific
operator evidence required for that next capture: exact Pit House setting,
starting value, ending value, and whether the setting value was restored. That
hardens the passive capture handoff only and does not create a semantic decode,
registry promotion, output sendability, or readiness claim.

The latest pre-output, lane analysis, role-status, and artifact-index receipts
report six proven input roles and one remaining generic auxiliary role.
Steering, throttle, brake, HBP handbrake, KS rim controls, and ES rim controls
are parser-proven. The SR-P clutch capture is parser-visible through two live
R5 V1 extended auxiliary slots, but the role-specific clutch semantic mapping
remains unproven:
`input_semantic_mapping_complete=false`,
`semantic_candidate_count=2`,
`ambiguous_semantic_candidate_count=0`, and
`unproven_required_role_count=1`. The clutch candidates are diagnostic
navigation only and keep `readiness_claim=false`.

The artifact-index and bench-wizard regression coverage now explicitly checks
that valid failed native-visible and smoke-ready verifier receipts remain useful
diagnostic artifacts without becoming readiness claims. This protects the
current lane shape, where failed verifier receipts are intentionally preserved
while native-visible and smoke-ready remain blocked.

The bench wizard now surfaces the pinned attempt-03 bench-clear phrase, required
profile, authorization receipt, and planned output receipt in its read-only
operator packet. This is navigation only: the wizard still creates no
authorization receipt, emits no output command, and makes no readiness claim.

The bench wizard also has a distinct post-authorization, pre-output handoff
state. If `native-controlled-angle-attempt-03-authorization.json` exists and the
attempt-03 output receipt is still missing, the wizard reports that the separate
authorization is recorded and names `native-controlled-angle-attempt-03-smoke.json`
as the planned output receipt, while still emitting no output command and
creating no authorization receipt itself.

The artifact index now treats the attempt-03 authorization and output receipts
as first-class frontier artifacts. After attempt 03, the required table marks
`native-controlled-angle-attempt-03-authorization.json` and
`native-controlled-angle-attempt-03-smoke.json` as present artifacts while
`native_visible_not_claimed` remains preserved.

The stored input analysis artifacts now include the same candidate-only R5 V1
extended-slot details that the role-status and artifact-index renderers surface.
`lane-capture-analysis.json` and `role-status-sync.json` identify brake and HBP
handbrake as proven semantic axes, and identify only the clutch auxiliary slots
as diagnostic candidates with `readiness_claim=false`; they still leave the
full input semantic mapping incomplete.

The current blocked-state handoff is
`plans/moza-native-visible-lane/handoff.md`. Use it when no active goal work
item is ready; do not invent new no-output work just to keep the lane moving.
After the post-authority PIDFF response comparison, the handoff should point at
review-only protocol analysis and the remaining no-output evidence gaps, not at
another hardware-output attempt.

## Work item: activate-source-of-truth

Status: completed
Linked proposal: docs/proposals/OR-PROP-0001-moza-native-visible-lane.md
Linked spec: docs/specs/OR-SPEC-0001-moza-native-visible-lane.md
Linked ADR: docs/adr/0009-hardware-validation-evidence-state-machine.md
Blocks: source-of-truth guided lane work
Blocked by: n/a

### Goal

Create the proposal, spec, implementation plan, active goal, and sprint status
update that identify the current Moza native-visible frontier without relying
on chat history.

### Production delta

Add source-of-truth docs and metadata. Refresh `docs/NOW_NEXT_LATER.md` so it
no longer names passive or zero-torque Moza work as the current hardware step.

### Non-goals

No hardware output, no authorization receipt, no verifier promotion, no hardware
artifact replacement, and no output code change.

### Acceptance

- `.openracing/goals/active.toml` points to this plan and spec.
- `docs/NOW_NEXT_LATER.md` names the current native-visible frontier.
- The claim boundary says attempt-03 preflight is non-authorizing.

### Proof commands

```powershell
python scripts/policy_file.py
cargo run --locked -p openracing-tools --bin package-surface -- --check
git diff --check
```

### Rollback

Remove the added source-of-truth files and restore `docs/NOW_NEXT_LATER.md` to
the previous text.

## Work item: record-completion-audit

Status: completed
Linked proposal: docs/proposals/OR-PROP-0001-moza-native-visible-lane.md
Linked spec: docs/specs/OR-SPEC-0001-moza-native-visible-lane.md
Linked ADR: docs/adr/0009-hardware-validation-evidence-state-machine.md
Blocks: clear no-output handoff while attempt-03 authorization is blocked
Blocked by: n/a

### Goal

Record a prompt-to-artifact audit that maps the broad Moza lane objective to
real checked-in receipts, verifier gates, and missing artifacts.

### Production delta

Add `docs/hardware/moza-r5-completion-audit.md` and link it from the active goal
status docs.

### Non-goals

No hardware output, no authorization receipt, no readiness promotion, no
hardware artifact replacement, and no output code change.

### Acceptance

- The audit restates the lane objective as concrete deliverables.
- Every explicit objective area is mapped to artifact evidence.
- Missing native-visible, Pit House, simulator, bounded simulator FFB, and
  smoke-ready gates are called out as incomplete.
- The audit does not rely on proxy status alone or claim completion.

### Proof commands

```powershell
python scripts/policy_file.py
cargo run --locked -p openracing-tools --bin package-surface -- --check
cargo run --locked -p wheelctl --bin wheelctl -- moza bench-wizard --lane ci/hardware/moza-r5/2026-05-13 --json-out target/moza-bench-wizard-current.json --md-out target/moza-bench-wizard-current.md --json
cargo run --locked -p wheelctl --bin wheelctl -- moza verify-bundle --lane ci/hardware/moza-r5/2026-05-13 --stage native-visible-ready --json-out target/moza-native-visible-current.json --json
git diff --check
```

### Rollback

Remove the audit doc and status-doc pointer. Do not touch lane receipts.

## Work item: document-input-role-candidates

Status: completed
Linked proposal: docs/proposals/OR-PROP-0001-moza-native-visible-lane.md
Linked spec: docs/specs/OR-SPEC-0001-moza-native-visible-lane.md
Linked ADR: docs/adr/0009-hardware-validation-evidence-state-machine.md
Blocks: clear input-topology handoff while native-visible output is blocked
Blocked by: n/a

### Goal

Record that brake, clutch, and handbrake are parser-visible through generic R5
V1 extended fields while their role-specific semantic mapping is still
diagnostic and incomplete.

### Production delta

Surface candidate-only slot metadata in pre-output readiness and artifact-index
navigation, then update the completion audit and artifact checklist so the
source-of-truth docs distinguish represented input topology from proven
role-specific semantics.

### Non-goals

No hardware output, no authorization receipt, no native-visible promotion, no
smoke-ready promotion, no role-specific semantic claim, and no parser remapping.

### Acceptance

- `pre-output-readiness.json` reports `input_semantic_mapping_complete=false`.
- Brake, clutch, and handbrake generic-aux roles include candidate-only R5 V1
  extended slots.
- Candidate metadata has `readiness_claim=false`.
- The completion audit calls the semantic mapping incomplete rather than
  treating candidate slots as proof.

### Proof commands

```powershell
python scripts/cargo_fmt_workspace.py
cargo test --locked -p wheelctl --bin wheelctl input_role -- --nocapture
cargo test --locked -p wheelctl --bin wheelctl moza -- --nocapture
cargo clippy --locked -p wheelctl --bin wheelctl -- -D warnings
cargo run --locked -p openracing-tools --bin package-surface -- --check
python scripts/policy_file.py
git diff --check
```

### Rollback

Remove the candidate-only navigation fields and revert the docs. Do not alter
the underlying passive input captures.

## Work item: target-only-r5-control-parser-regression

Status: completed
Linked proposal: docs/proposals/OR-PROP-0001-moza-native-visible-lane.md
Linked spec: docs/specs/OR-SPEC-0001-moza-native-visible-lane.md
Linked ADR: docs/adr/0009-hardware-validation-evidence-state-machine.md
Blocks: safer parser expansion from target-only wheel control captures
Blocked by: n/a

### Goal

Preserve the target-only wheel button, thumb control, and central control
samples as parser regression coverage without turning them into readiness or
button-name claims.

### Production delta

Added a `racing-wheel-moza-wheelbase-report` regression test using target-only
live R5 V1 extended reports from the Pit House solo wheel-controls capture. The
test verifies the parser selects the live R5 V1 extended layout, preserves the
full packed button byte window, and keeps clutch/rotary/funky semantics
unpromoted for those samples.

### Non-goals

No hardware output, no HID open, no new capture, no raw pcap commit, no
button-name mapping, no clutch role-specific semantic promotion, no rotary
semantic promotion, no native-visible promotion, and no smoke-ready promotion.

### Acceptance

- Target-only live R5 V1 wheel-control sample reports parse as 42-byte extended
  reports.
- Packed button bytes from offsets `17..32` are preserved exactly.
- `clutch`, `funky`, and `rotary` remain unpromoted for these samples.
- The work creates no readiness claim and does not change lane promotion status.

### Proof commands

```powershell
python scripts/cargo_fmt_workspace.py
cargo test --locked -p racing-wheel-moza-wheelbase-report parse_wheelbase_input_preserves_target_only_live_r5_v1_button_samples -- --nocapture
cargo clippy --locked -p racing-wheel-moza-wheelbase-report --all-features -- -D warnings
cargo run --locked -p openracing-tools --bin package-surface -- --check
python scripts/policy_file.py
git diff --check
```

### Rollback

Remove only the parser regression test and this plan entry. Do not remove local
target-only capture artifacts, lane input receipts, or candidate-only input
role documentation.

## Work item: harden-artifact-claim-boundaries

Status: completed
Linked proposal: docs/proposals/OR-PROP-0001-moza-native-visible-lane.md
Linked spec: docs/specs/OR-SPEC-0001-moza-native-visible-lane.md
Linked ADR: docs/adr/0009-hardware-validation-evidence-state-machine.md
Blocks: reliable operator navigation while failed verifier receipts are
preserved
Blocked by: n/a

### Goal

Ensure artifact navigation treats valid failed verifier receipts as diagnostic
evidence only, not native-visible or smoke-ready success.

### Production delta

Extend the Moza artifact-index claim-status regression to cover bench-wizard
readiness handling for the same failed native-visible and smoke-ready verifier
fixtures.

### Non-goals

No hardware output, no authorization receipt, no hardware artifact replacement,
no readiness promotion, and no change to verifier gate semantics.

### Acceptance

- `artifact-index` keeps valid failed native-visible and smoke-ready verifier
  receipts at artifact status `pass` with claim status `stage_failed`.
- `bench-wizard` keeps `native_visible_claimed=false`,
  `smoke_ready_claimed=false`, `native_visible_motion_proven` not true, and
  `ready_for_real_hardware_smoke` not true for those fixtures.
- Native-visible remains an active blocker when the stored verifier failed.

### Proof commands

```powershell
python scripts/cargo_fmt_workspace.py
cargo test --locked -p wheelctl --bin wheelctl artifact_index -- --nocapture
cargo test --locked -p wheelctl --bin wheelctl bench_wizard -- --nocapture
cargo test --locked -p wheelctl --bin wheelctl moza -- --nocapture
cargo clippy --locked -p wheelctl --bin wheelctl -- -D warnings
cargo run --locked -p openracing-tools --bin package-surface -- --check
git diff --check
```

### Rollback

Remove only the added regression assertions. Do not remove or rewrite preserved
failed verifier receipts.

## Work item: surface-attempt-03-clearance-phrase

Status: completed
Linked proposal: docs/proposals/OR-PROP-0001-moza-native-visible-lane.md
Linked spec: docs/specs/OR-SPEC-0001-moza-native-visible-lane.md
Linked ADR: docs/adr/0009-hardware-validation-evidence-state-machine.md
Blocks: reliable operator handoff for attempt-03 authorization
Blocked by: n/a

### Goal

Make the no-output bench wizard show the exact command-bound clearance phrase
that the attempt-03 authorizer requires.

### Production delta

Add the pinned attempt-03 bench-clear phrase, required profile, authorization
receipt, and planned output receipt to the `bench-wizard` next-operator-step
payload and Markdown output.

### Non-goals

No authorization receipt, no hardware output, no HID open, no output command,
no readiness promotion, and no change to the attempt-03 command shape.

### Acceptance

- `bench-wizard` reports `required_bench_clear_evidence` for the
  `awaiting_separate_attempt_03_authorization` step.
- Markdown includes the exact phrase in a text block.
- The wizard still reports `hardware_output_allowed_now=false`,
  `authorization_created_by_wizard=false`, and emits only safe no-output
  commands.

### Proof commands

```powershell
python scripts/cargo_fmt_workspace.py
cargo test --locked -p wheelctl --bin wheelctl bench_wizard -- --nocapture
cargo test --locked -p wheelctl --bin wheelctl moza -- --nocapture
cargo clippy --locked -p wheelctl --bin wheelctl -- -D warnings
cargo run --locked -p openracing-tools --bin package-surface -- --check
git diff --check
```

### Rollback

Remove only the operator-packet fields and assertions. Do not alter attempt-03
receipts or authorization semantics.

## Work item: harden-attempt-03-authorization-handoff

Status: completed
Linked proposal: docs/proposals/OR-PROP-0001-moza-native-visible-lane.md
Linked spec: docs/specs/OR-SPEC-0001-moza-native-visible-lane.md
Linked ADR: docs/adr/0009-hardware-validation-evidence-state-machine.md
Blocks: clear operator handoff after attempt-03 authorization is recorded
Blocked by: n/a

### Goal

Teach the read-only bench wizard to distinguish the state where the exact
attempt-03 authorization receipt exists but the planned output receipt has not
been recorded.

### Production delta

Add a bench-wizard next-operator-step branch for
`awaiting_separate_attempt_03_output`. It validates the recorded attempt-03
authorization shape, names `native-controlled-angle-attempt-03-authorization.json`
and `native-controlled-angle-attempt-03-smoke.json`, and keeps the wizard from
emitting authorization or hardware-output commands.

### Non-goals

No authorization receipt, no hardware output, no HID open, no output command,
no readiness promotion, and no change to the attempt-03 output command shape.

### Acceptance

- A synthetic lane with attempt-03 preflight and authorization receipts reports
  `next_operator_step.kind=awaiting_separate_attempt_03_output`.
- The wizard still reports `hardware_output_authorized=false` and
  `authorization_receipt_created=false`.
- The wizard safe command list contains no `authorize-controlled-angle-output`
  or `--confirm-controlled-angle` command.
- Markdown names the recorded authorization receipt and planned output receipt.

### Proof commands

```powershell
python scripts/cargo_fmt_workspace.py
cargo test --locked -p wheelctl --bin wheelctl bench_wizard -- --nocapture
cargo test --locked -p wheelctl --bin wheelctl controlled_angle -- --nocapture
cargo test --locked -p wheelctl --bin wheelctl moza -- --nocapture
cargo clippy --locked -p wheelctl --bin wheelctl -- -D warnings
cargo run --locked -p openracing-tools --bin package-surface -- --check
git diff --check
```

### Rollback

Remove only the post-authorization wizard branch and regression test. Preserve
any real attempt-03 authorization or output receipts if they exist.

## Work item: surface-attempt-03-planned-artifacts

Status: completed
Linked proposal: docs/proposals/OR-PROP-0001-moza-native-visible-lane.md
Linked spec: docs/specs/OR-SPEC-0001-moza-native-visible-lane.md
Linked ADR: docs/adr/0009-hardware-validation-evidence-state-machine.md
Blocks: clear artifact-index navigation while attempt-03 is blocked
Blocked by: n/a

### Goal

Make the artifact index explicitly show the planned attempt-03 authorization and
output receipts before they exist, without making them readiness claims.

### Production delta

Extend artifact-index navigation requirements to include
`native-controlled-angle-attempt-03-authorization.json` and
`native-controlled-angle-attempt-03-smoke.json`. Missing entries render with
`claim_status=planned_missing` and `native_visible_not_claimed`, and the checked
in lane `index.md` is regenerated from `wheelctl moza artifact-index`.

### Non-goals

No authorization receipt, no hardware output, no HID open, no output command,
no readiness promotion, no support-bundle gate expansion, and no change to the
attempt-03 command shape.

### Acceptance

- Artifact-index required table includes both attempt-03 planned artifacts.
- Missing planned artifacts use `claim_status=planned_missing`.
- Native-visible readiness remains false.
- Checked-in `ci/hardware/moza-r5/2026-05-13/index.md` matches the renderer.

### Proof commands

```powershell
python scripts/cargo_fmt_workspace.py
cargo test --locked -p wheelctl --bin wheelctl artifact_index -- --nocapture
cargo test --locked -p wheelctl --bin wheelctl moza -- --nocapture
cargo clippy --locked -p wheelctl --bin wheelctl -- -D warnings
cargo run --locked -p openracing-tools --bin package-surface -- --check
python scripts/policy_file.py
git diff --check
```

### Rollback

Remove only the planned artifact-index requirements and regenerate
`ci/hardware/moza-r5/2026-05-13/index.md`. Do not alter any real attempt-03
authorization or output receipts if they exist.

## Work item: refresh-input-semantic-candidate-artifacts

Status: completed
Linked proposal: docs/proposals/OR-PROP-0001-moza-native-visible-lane.md
Linked spec: docs/specs/OR-SPEC-0001-moza-native-visible-lane.md
Linked ADR: docs/adr/0009-hardware-validation-evidence-state-machine.md
Blocks: current no-output input-topology evidence while native-visible is blocked
Blocked by: n/a

### Goal

Refresh the checked-in no-output input analysis receipts so they record the
candidate-only R5 V1 extended slots for brake, clutch, and handbrake.

### Production delta

Regenerate `lane-capture-analysis.json` and `role-status-sync.json` with the
current analyzer. The refreshed receipts include `semantic_candidates` for the
generic-aux roles and keep every candidate at `readiness_claim=false`. The
artifact index renderer was re-run; the checked-in `index.md` already matched
the current output.

### Non-goals

No hardware output, no HID open, no authorization receipt, no output receipt,
no native-visible promotion, no parser remapping, and no role-specific semantic
claim for SR-P or HBP controls.

### Acceptance

- `lane-capture-analysis.json` includes semantic candidates for brake, clutch,
  and handbrake generic-aux roles.
- `role-status-sync.json` reports `stale_control_count=0` and
  `manifest_written=false`.
- Candidate metadata remains diagnostic-only with `readiness_claim=false`.
- Passive verification still passes.
- Native-visible remains blocked separately by the visible-motion gate.

### Proof commands

```powershell
cargo run --locked -p wheelctl --bin wheelctl -- moza analyze-lane --lane ci/hardware/moza-r5/2026-05-13 --json-out ci/hardware/moza-r5/2026-05-13/lane-capture-analysis.json --json
cargo run --locked -p wheelctl --bin wheelctl -- moza sync-role-status --lane ci/hardware/moza-r5/2026-05-13 --json-out ci/hardware/moza-r5/2026-05-13/role-status-sync.json --json
cargo run --locked -p wheelctl --bin wheelctl -- moza artifact-index --lane ci/hardware/moza-r5/2026-05-13 --json-out target/moza-artifact-index-input-semantics.json --md-out ci/hardware/moza-r5/2026-05-13/index.md --json
cargo run --locked -p wheelctl --bin wheelctl -- moza sync-role-status --lane ci/hardware/moza-r5/2026-05-13 --check --json-out target/moza-role-status-check.json --json
cargo run --locked -p wheelctl --bin wheelctl -- moza verify-bundle --lane ci/hardware/moza-r5/2026-05-13 --stage passive --json-out target/moza-passive-after-input-semantics-refresh.json --json
cargo run --locked -p openracing-tools --bin package-surface -- --check
python scripts/policy_file.py
git diff --check
```

### Rollback

Restore only the previous generated analysis receipts. Do not alter the
underlying passive captures, manifest topology, attempt-03 preflight,
authorization, or output receipts.

## Work item: surface-pit-house-compatibility-progress

Status: completed
Linked proposal: docs/proposals/OR-PROP-0001-moza-native-visible-lane.md
Linked spec: docs/specs/OR-SPEC-0001-moza-native-visible-lane.md
Linked ADR: docs/adr/0009-hardware-validation-evidence-state-machine.md
Blocks: clearer external-smoke navigation while native-visible is blocked
Blocked by: n/a

### Goal

Make the no-output navigation surfaces show partial Pit House coexistence
progress without making Pit House a native-control prerequisite or converting
case artifacts into smoke-ready claims.

### Production delta

`wheelctl moza artifact-index` and `wheelctl moza bench-wizard` now include a
`pit_house_compatibility` summary. It reads the existing availability artifact,
the required case artifacts, and the parent coexistence gate. The summary
records recorded cases, missing cases, availability state, and the parent gate
status while keeping `readiness_claim=false`, `blocks_native_control=false`, and
`blocks_native_visible=false`.

### Non-goals

No hardware output, no HID open, no Pit House evidence capture, no new
coexistence artifact, no simulator artifact, no authorization receipt, and no
native-visible or smoke-ready promotion.

### Acceptance

- The artifact index and bench wizard surface the recorded closed Pit House case
  separately from the missing parent `pit-house-coexistence.json`.
- Missing open/direct/mode-change/firmware-page cases stay visible as external
  smoke blockers.
- Pit House progress reports `readiness_claim=false` and does not affect native
  readiness.
- Markdown renderers state that Pit House compatibility is external-smoke
  navigation only and does not authorize output.

### Proof commands

```powershell
python scripts/cargo_fmt_workspace.py
cargo test --locked -p wheelctl --bin wheelctl artifact_navigation_surfaces_pit_house -- --nocapture
cargo test --locked -p wheelctl --bin wheelctl artifact_index -- --nocapture
cargo test --locked -p wheelctl --bin wheelctl bench_wizard -- --nocapture
cargo clippy --locked -p wheelctl --bin wheelctl -- -D warnings
cargo run --locked -p openracing-tools --bin package-surface -- --check
git diff --check
```

### Rollback

Remove only the Pit House compatibility summary fields, renderer section, and
tests. Do not alter any Pit House receipts, attempt-03 artifacts, or verifier
gates.

## Work item: surface-simulator-compatibility-progress

Status: completed
Linked proposal: docs/proposals/OR-PROP-0001-moza-native-visible-lane.md
Linked spec: docs/specs/OR-SPEC-0001-moza-native-visible-lane.md
Linked ADR: docs/adr/0009-hardware-validation-evidence-state-machine.md
Blocks: clearer external-smoke navigation while native-visible is blocked
Blocked by: n/a

### Goal

Make the no-output navigation surfaces show simulator telemetry and bounded
simulator FFB progress without making simulator evidence a native-control
prerequisite or converting missing/present-but-unaccepted simulator artifacts
into smoke-ready claims.

### Production delta

`wheelctl moza artifact-index` and `wheelctl moza bench-wizard` now include a
`simulator_compatibility` summary. It reads the simulator telemetry and bounded
simulator FFB artifacts, checks their verifier gates, and records missing,
present-not-accepted, or accepted state while keeping `readiness_claim=false`,
`blocks_native_control=false`, and `blocks_native_visible=false`.

### Non-goals

No hardware output, no HID open, no simulator launch, no telemetry capture, no
bounded FFB receipt, no authorization receipt, and no native-visible or
smoke-ready promotion.

### Acceptance

- The artifact index and bench wizard surface simulator telemetry separately
  from bounded simulator FFB.
- Missing simulator artifacts stay visible as external smoke blockers.
- Present-but-unaccepted simulator receipts do not surface output-looking fields
  as trusted evidence.
- Simulator progress reports `readiness_claim=false` and does not affect native
  readiness.
- Markdown renderers state that simulator compatibility is external-smoke
  navigation only and does not authorize output.

### Proof commands

```powershell
python scripts/cargo_fmt_workspace.py
cargo test --locked -p wheelctl --bin wheelctl artifact_navigation_surfaces_simulator -- --nocapture
cargo test --locked -p wheelctl --bin wheelctl simulator_navigation -- --nocapture
cargo test --locked -p wheelctl --bin wheelctl artifact_index -- --nocapture
cargo test --locked -p wheelctl --bin wheelctl bench_wizard -- --nocapture
cargo clippy --locked -p wheelctl --bin wheelctl -- -D warnings
cargo run --locked -p openracing-tools --bin package-surface -- --check
git diff --check
```

### Rollback

Remove only the simulator compatibility summary fields, renderer section, and
tests. Do not alter any simulator receipts, attempt-03 artifacts, authorization
logic, or verifier gates.

## Work item: surface-passive-sniff-navigation

Status: completed
Linked proposal: docs/proposals/OR-PROP-0001-moza-native-visible-lane.md
Linked spec: docs/specs/OR-SPEC-0001-moza-native-visible-lane.md
Linked ADR: docs/adr/0009-hardware-validation-evidence-state-machine.md
Blocks: clearer protocol-research navigation while native-visible is blocked
Blocked by: n/a

### Goal

Make the no-output navigation surfaces show passive USB sniff scenarios for Pit
House, SimHub, and simulator protocol research without making sniff artifacts a
native-control prerequisite or converting support evidence into readiness
claims.

### Production delta

`wheelctl moza artifact-index` and `wheelctl moza bench-wizard` now include a
`passive_sniff_navigation` summary. It lists the planned passive sniff
scenarios, detects non-claiming plan/receipt/summary artifacts when present, and
keeps missing scenarios as navigation-only work with `readiness_claim=false`,
`blocks_native_control=false`, `blocks_native_visible=false`, and
`blocks_smoke_ready=false`.

### Non-goals

No hardware output, no HID open, no USBPcap/Wireshark capture, no raw pcapng
commit, no sniff bundle generation, no authorization receipt, and no
native-visible or smoke-ready promotion.

### Acceptance

- The artifact index and bench wizard surface Pit House, SimHub, and simulator
  passive sniff scenarios separately from native-visible readiness.
- Missing sniff scenarios are navigation-only gaps and never native-visible or
  smoke-ready blockers.
- Present sniff artifacts must be non-claiming before they are displayed as
  recorded.
- Markdown renderers state that passive sniffing is protocol research/support
  navigation only and does not authorize output.

### Proof commands

```powershell
python scripts/cargo_fmt_workspace.py
cargo test --locked -p wheelctl --bin wheelctl passive_sniff -- --nocapture
cargo test --locked -p wheelctl --bin wheelctl artifact_index -- --nocapture
cargo test --locked -p wheelctl --bin wheelctl bench_wizard -- --nocapture
cargo clippy --locked -p wheelctl --bin wheelctl -- -D warnings
cargo run --locked -p openracing-tools --bin package-surface -- --check
python scripts/policy_file.py
git diff --check
```

### Rollback

Remove only the passive sniff navigation summary fields, renderer section, and
tests. Do not alter sniff schemas, sniff command behavior, hardware artifacts,
attempt-03 artifacts, authorization logic, or verifier gates.

## Work item: attempt-03-authorization

Status: completed
Linked proposal: docs/proposals/OR-PROP-0001-moza-native-visible-lane.md
Linked spec: docs/specs/OR-SPEC-0001-moza-native-visible-lane.md
Linked ADR: docs/adr/0009-hardware-validation-evidence-state-machine.md
Blocks: attempt-03-output
Blocked by: n/a

### Goal

Create one exact authorization receipt for attempt 03 only after the operator
provides fresh bench-clear evidence for the exact command.

### Production delta

Ran no-output readiness and native-visible verification first. Then created
`native-controlled-angle-attempt-03-authorization.json` with the exact command
shape from the preflight and command-bound operator evidence. The authorization
was consumed by the single recorded attempt and authorizes no further output.

Required bench-clear evidence:

```text
bench clear for exactly one Moza controlled-angle attempt 03: target 1 degree, max 5%, timeout 2000 ms, strategy pidff-bounded-effect, profile bounded-pidff-effect-lifecycle-v1, R5 stable, KS attached securely, hands clear, wheel clear, prior undertravel receipts preserved
```

Expected command shape after fresh bench-clear:

```powershell
wheelctl moza authorize-controlled-angle-output `
  --lane ci/hardware/moza-r5/2026-05-13 `
  --device hid-0x346E-0x0004-if2-0x0001-0x0004 `
  --operator Steven `
  --bench-clear-evidence "bench clear for exactly one Moza controlled-angle attempt 03: target 1 degree, max 5%, timeout 2000 ms, strategy pidff-bounded-effect, profile bounded-pidff-effect-lifecycle-v1, R5 stable, KS attached securely, hands clear, wheel clear, prior undertravel receipts preserved" `
  --prior-response-proof ci/hardware/moza-r5/2026-05-13/native-actuator-visible-smoke.json `
  --prior-actuator-proof ci/hardware/moza-r5/2026-05-13/native-actuator-profile-smoke.json `
  --steering-proof ci/hardware/moza-r5/2026-05-13/steering-angle-stream-proof.json `
  --controlled-angle-preflight ci/hardware/moza-r5/2026-05-13/native-controlled-angle-attempt-03-preflight.json `
  --planned-output ci/hardware/moza-r5/2026-05-13/native-controlled-angle-attempt-03-smoke.json `
  --target-degrees 1 `
  --profile bounded-pidff-effect-lifecycle-v1 `
  --strategy pidff-bounded-effect `
  --max-percent 5 `
  --timeout-ms 2000 `
  --json-out ci/hardware/moza-r5/2026-05-13/native-controlled-angle-attempt-03-authorization.json `
  --json
```

### Non-goals

No hardware output, no motion claim, no force increase, no longer dwell, no
larger angle, and no external compatibility claim.

### Acceptance

- Authorization binds lane, device, profile, target, max percent, timeout,
  strategy, prior proofs, preflight, planned output path, and operator evidence.
- The prior undertravel receipts remain preserved.

### Proof commands

```powershell
wheelctl moza pre-output-readiness --lane ci/hardware/moza-r5/2026-05-13 --json-out target/moza-pre-output-before-attempt-03.json --json
wheelctl moza verify-bundle --lane ci/hardware/moza-r5/2026-05-13 --stage native-visible-ready --json-out target/moza-native-visible-before-attempt-03.json --json
```

### Rollback

The authorization has been consumed by output. Preserve it with the output
receipt and failure analysis.

## Work item: attempt-03-output

Status: completed
Linked proposal: docs/proposals/OR-PROP-0001-moza-native-visible-lane.md
Linked spec: docs/specs/OR-SPEC-0001-moza-native-visible-lane.md
Linked ADR: docs/adr/0009-hardware-validation-evidence-state-machine.md
Blocks: native-visible-promotion or attempt-03-analysis
Blocked by: n/a

### Goal

Run exactly one 1 degree, 5 percent, 2000 ms controlled-angle attempt using
`bounded-pidff-effect-lifecycle-v1`.

### Production delta

Created `native-controlled-angle-attempt-03-smoke.json` from exactly one hardware
output command. The command stopped on timeout before target and sent final Stop
All.

### Non-goals

No rerun, no 3/5/10/30/90 degree step, no force increase, no dwell extension,
no direct report `0x20`, no high torque, no serial config, no firmware, and no
DFU.

### Acceptance

- Exactly one output command runs.
- Receipt records target status, angle delta, writes, write errors, final Stop
  All, post-stop stability, authorization consumption, and forbidden path
  booleans.

### Proof commands

```powershell
wheelctl moza controlled-angle-smoke `
  --device hid-0x346E-0x0004-if2-0x0001-0x0004 `
  --lane ci/hardware/moza-r5/2026-05-13 `
  --prior-actuator-proof ci/hardware/moza-r5/2026-05-13/native-actuator-profile-smoke.json `
  --steering-proof ci/hardware/moza-r5/2026-05-13/steering-angle-stream-proof.json `
  --authorization-proof ci/hardware/moza-r5/2026-05-13/native-controlled-angle-attempt-03-authorization.json `
  --target-degrees 1 `
  --profile bounded-pidff-effect-lifecycle-v1 `
  --max-percent 5 `
  --timeout-ms 2000 `
  --strategy pidff-bounded-effect `
  --confirm-controlled-angle `
  --json-out ci/hardware/moza-r5/2026-05-13/native-controlled-angle-attempt-03-smoke.json `
  --json
```

### Rollback

Do not delete the hardware-output receipt. Preserve it with the consumed
authorization and analysis.

## Work item: attempt-03-analysis

Status: completed
Linked proposal: docs/proposals/OR-PROP-0001-moza-native-visible-lane.md
Linked spec: docs/specs/OR-SPEC-0001-moza-native-visible-lane.md
Linked ADR: docs/adr/0009-hardware-validation-evidence-state-machine.md
Blocks: no-output protocol architecture diagnosis
Blocked by: n/a

### Goal

Classify the single attempt-03 output receipt without running any further
hardware output.

### Production delta

Added `native-controlled-angle-attempt-03-failure-analysis.json`. It classifies
attempt 03 as safe undertravel in the same response band, records the standard
PIDFF effect lifecycle as ineffective in the current R5 device mode, and keeps
native-visible, smoke-ready, release-ready, rerun, and force-escalation claims
false.

### Non-goals

No output rerun, no new authorization, no force increase, no dwell extension, no
larger angle, no direct report `0x20`, no high torque, no serial config, no
firmware, no DFU, and no readiness promotion.

### Acceptance

- Attempt-03 authorization, output, verifier, and analysis receipts are
  preserved.
- Analysis records write_errors=0, final Stop All sent, post-stop stable, no
  high torque, no direct report `0x20`, no serial config, and no firmware/DFU.
- Planned next output remains disallowed.
- Native visible motion remains unclaimed.

### Proof commands

```powershell
cargo run --locked -p wheelctl --bin wheelctl -- moza verify-bundle --lane ci/hardware/moza-r5/2026-05-13 --stage native-visible-ready --json-out ci/hardware/moza-r5/2026-05-13/native-visible-verification.json --json
cargo run --locked -p wheelctl --bin wheelctl -- moza artifact-index --lane ci/hardware/moza-r5/2026-05-13 --json-out target/moza-artifact-index-after-attempt-03.json --md-out ci/hardware/moza-r5/2026-05-13/index.md --json
cargo run --locked -p openracing-tools --bin package-surface -- --check
python scripts/policy_file.py
git diff --check
```

### Rollback

Do not remove the attempt-03 receipts. If wording needs correction, add a
corrective analysis/doc patch that preserves the hardware evidence.

## Work item: standard-pidff-path-diagnosis

Status: completed
Linked proposal: docs/proposals/OR-PROP-0001-moza-native-visible-lane.md
Linked spec: docs/specs/OR-SPEC-0001-moza-native-visible-lane.md
Linked ADR: docs/adr/0009-hardware-validation-evidence-state-machine.md
Blocks: Moza vendor-specific enable/control path investigation
Blocked by: n/a

### Goal

Record a no-output protocol architecture diagnosis after three bounded
standard-PIDFF-family controlled-angle attempts all failed safely in the same
undertravel band.

### Production delta

Added `native-pidff-standard-path-diagnosis.json` and
`docs/hardware/moza-r5-standard-pidff-path-diagnosis.md`. The diagnosis
classifies the standard PIDFF controlled-angle path as
`standard_pidff_controlled_angle_path_ineffective_in_current_r5_mode`, preserves
all three hardware receipts, keeps native-visible and smoke-ready claims false,
and records Moza vendor-specific enable/control path investigation as the next
no-output research branch.

### Non-goals

No hardware output, no authorization receipt, no rerun, no force increase, no
longer dwell, no 3/5/30/90 degree target, no direct report `0x20`, no high
torque, no serial config, no firmware, no DFU, and no readiness promotion.

### Acceptance

- Diagnosis records that transport writes, steering feedback, cleanup, and
  post-stop stability work.
- Diagnosis records that standard PIDFF writes are accepted but the effect
  lifecycle still does not produce visible controlled motion.
- `native_visible_claimed=false`, `smoke_ready_claimed=false`, and
  `planned_next_output.allowed=false`.
- Native-visible remains blocked on `native_actuator_visible_smoke`.
- Next work is no-output vendor-specific protocol investigation before any
  future output plan.

### Proof commands

```powershell
cargo run --locked -p wheelctl --bin wheelctl -- moza verify-bundle --lane ci/hardware/moza-r5/2026-05-13 --stage native-visible-ready --json-out target/moza-native-visible-after-standard-pidff-diagnosis.json --json
cargo run --locked -p wheelctl --bin wheelctl -- moza artifact-index --lane ci/hardware/moza-r5/2026-05-13 --json-out target/moza-artifact-index-after-standard-pidff-diagnosis.json --md-out ci/hardware/moza-r5/2026-05-13/index.md --json
cargo run --locked -p openracing-tools --bin package-surface -- --check
python scripts/policy_file.py
git diff --check
```

### Rollback

Do not remove the controlled-angle receipts. If wording needs correction, add a
corrective no-output diagnosis/doc patch that preserves the hardware evidence
and keeps planned output disallowed.

## Work item: vendor-control-sniff-plans

Status: completed
Linked proposal: docs/proposals/OR-PROP-0001-moza-native-visible-lane.md
Linked spec: docs/specs/OR-SPEC-0001-moza-native-visible-lane.md
Linked ADR: docs/adr/0009-hardware-validation-evidence-state-machine.md
Blocks: passive vendor USB captures and report decoding
Blocked by: n/a

### Goal

Materialize the no-output sniff plan artifacts for the Moza vendor-specific
enable/control investigation identified by the standard PIDFF diagnosis.

### Production delta

Added five `wheelctl hardware sniff-plan` JSON artifacts under
`ci/hardware/sniff/moza-r5/2026-05-13`:

- `pit-house-open-idle/sniff-plan.json`
- `pit-house-setting-change/sniff-plan.json`
- `simhub-open-idle/sniff-plan.json`
- `simhub-output-session/sniff-plan.json`
- `simulator-session-start-stop/sniff-plan.json`

Added `docs/hardware/moza-r5-vendor-control-investigation.md` to define the
plan-only state, claim ceiling, forbidden actions, and the next evidence needed
before vendor report decoding or a reviewed vendor-control plan.

### Non-goals

No hardware output, no OpenRacing HID output reports, no OpenRacing HID feature
reports, no pcap capture, no sniff receipt, no sniff summary, no raw pcap commit,
no authorization receipt, no native-visible promotion, no smoke-ready promotion,
no serial config, no firmware, and no DFU.

### Acceptance

- Each plan has `command=wheelctl hardware sniff-plan`.
- Each plan has `native_control_evidence=false`,
  `openracing_hardware_output=false`,
  `satisfies_native_visible_ready=false`, and `satisfies_smoke_ready=false`.
- Artifact index reports each plan as `present_non_claiming`.
- Scenarios remain `partial_or_unaccepted` until `sniff-receipt.json` and
  `sniff-summary.json` exist.
- Native-visible remains blocked on `native_actuator_visible_smoke`.

### Proof commands

```powershell
cargo run --locked -p wheelctl --bin wheelctl -- moza artifact-index --lane ci/hardware/moza-r5/2026-05-13 --json-out target/moza-artifact-index-after-vendor-sniff-plans.json --md-out ci/hardware/moza-r5/2026-05-13/index.md --json
cargo run --locked -p wheelctl --bin wheelctl -- moza verify-bundle --lane ci/hardware/moza-r5/2026-05-13 --stage native-visible-ready --json-out target/moza-native-visible-after-vendor-sniff-plans.json --json
cargo run --locked -p openracing-tools --bin package-surface -- --check
python scripts/policy_file.py
git diff --check
```

### Rollback

Remove only the plan-only sniff artifacts and documentation. Do not remove
controlled-angle receipts, PIDFF diagnoses, verifier receipts, or safety
evidence.

## Work item: bench-wizard-vendor-sniff-next-step

Status: completed
Linked proposal: docs/proposals/OR-PROP-0001-moza-native-visible-lane.md
Linked spec: docs/specs/OR-SPEC-0001-moza-native-visible-lane.md
Linked ADR: docs/adr/0009-hardware-validation-evidence-state-machine.md
Blocks: command-bound passive sniff capture handoff
Blocked by: n/a

### Goal

Make the read-only bench wizard stop pointing at stale attempt-03
classification once `native-pidff-standard-path-diagnosis.json` exists. The
next operator step must instead point to the first plan-only passive sniff
scenario that needs a `sniff-receipt.json` and `sniff-summary.json`.

### Production delta

Updated `wheelctl moza bench-wizard` so `next_operator_step.kind` becomes
`capture_passive_vendor_sniff` for the current lane. The step records the
planned scenario, local pcapng path, committed receipt/summary paths, and
command-bound no-output `wheelctl hardware sniff-receipt`, `sniff-summary`, and
`sniff-bundle` commands. Markdown output now renders those next-step commands.
The native-visible verifier guidance also stops asking for stale attempt-03
classification once the standard-PIDFF diagnosis exists and points to the same
passive sniff handoff.

### Non-goals

No hardware output, no authorization receipt, no HID open, no OpenRacing HID
output or feature reports, no pcap capture, no sniff receipt, no sniff summary,
no raw pcap commit, no native-visible promotion, and no smoke-ready promotion.

### Acceptance

- With attempt-03 output and standard-PIDFF diagnosis present, bench wizard
  reports `next_operator_step.kind=capture_passive_vendor_sniff`.
- Native-visible verifier operator actions no longer request attempt-03
  classification after the standard-PIDFF diagnosis exists.
- The first current scenario is `pit-house-open-idle`.
- The step includes command-bound `wheelctl hardware sniff-receipt`,
  `wheelctl hardware sniff-summary`, and `wheelctl hardware sniff-bundle`
  commands.
- The step keeps `hardware_output_allowed_now=false` and
  `no_openracing_output=true`.
- `verify-bundle --stage native-visible-ready` remains blocked on
  `native_actuator_visible_smoke`.

### Proof commands

```powershell
cargo test --locked -p wheelctl --bin wheelctl bench_wizard -- --nocapture
cargo run --locked -p wheelctl --bin wheelctl -- moza bench-wizard --lane ci/hardware/moza-r5/2026-05-13 --json-out target/moza-bench-wizard-after-sniff-next.json --md-out target/moza-bench-wizard-after-sniff-next.md --json
cargo run --locked -p wheelctl --bin wheelctl -- moza verify-bundle --lane ci/hardware/moza-r5/2026-05-13 --stage native-visible-ready --json-out target/moza-native-visible-after-bench-wizard-sniff-next.json --json
cargo run --locked -p openracing-tools --bin package-surface -- --check
python scripts/policy_file.py
git diff --check
```

### Rollback

Revert only the bench-wizard next-step and Markdown rendering changes plus this
plan entry. Do not remove any sniff plan artifacts or hardware receipts.

## Work item: bench-wizard-sniff-command-validation

Status: completed
Linked proposal: docs/proposals/OR-PROP-0001-moza-native-visible-lane.md
Linked spec: docs/specs/OR-SPEC-0001-moza-native-visible-lane.md
Linked ADR: docs/adr/0009-hardware-validation-evidence-state-machine.md
Blocks: command-bound passive sniff handoff reliability
Blocked by: n/a

### Goal

Make the current no-output passive sniff handoff self-checking: the
bench-wizard-generated `wheelctl hardware sniff-receipt`, `sniff-summary`, and
`sniff-bundle` command strings must parse through the real CLI parser before
they are trusted as operator handoff text.

### Production delta

Added a focused `bench_wizard_sniff_next_operator_commands_parse` unit test that
constructs the diagnosed attempt-03 frontier, asks bench-wizard for the first
passive sniff next step, and parses each generated handoff command with the
same generated-command splitter and `clap` parser used for verifier
`next_commands`. The test also asserts these commands stay in the no-output
`wheelctl hardware` namespace and do not contain authorization or
controlled-angle output tokens.

### Non-goals

No production behavior change, no hardware output, no authorization receipt, no
HID open, no pcap capture, no sniff receipt, no sniff summary, no bundle
artifact, no raw pcap commit, no native-visible promotion, and no smoke-ready
promotion.

### Acceptance

- The diagnosed attempt-03 bench-wizard next step still emits exactly the
  `record_sniff_receipt`, `summarize_sniff_capture`, and
  `bundle_sniff_evidence` handoff commands.
- Each generated command parses through `wheelctl`.
- Each generated command reports `output_enabled=false`.
- The command text contains no authorization or controlled-angle output token.

### Proof commands

```powershell
cargo test --locked -p wheelctl --bin wheelctl bench_wizard_sniff_next_operator_commands_parse -- --nocapture
cargo test --locked -p wheelctl --bin wheelctl bench_wizard -- --nocapture
cargo run --locked -p wheelctl --bin wheelctl -- moza bench-wizard --lane ci/hardware/moza-r5/2026-05-13 --json-out target/moza-bench-wizard-after-sniff-command-validation.json --md-out target/moza-bench-wizard-after-sniff-command-validation.md --json
cargo run --locked -p openracing-tools --bin package-surface -- --check
python scripts/policy_file.py
git diff --check
```

### Rollback

Revert only the command-parse test and this plan entry. Do not remove any sniff
plan artifacts or hardware receipts.

## Work item: bench-wizard-sniff-capture-checklist

Status: completed
Linked proposal: docs/proposals/OR-PROP-0001-moza-native-visible-lane.md
Linked spec: docs/specs/OR-SPEC-0001-moza-native-visible-lane.md
Linked ADR: docs/adr/0009-hardware-validation-evidence-state-machine.md
Blocks: operator-captured passive USB sniff evidence
Blocked by: n/a

### Goal

Make the next passive sniff event operationally unambiguous by having
bench-wizard emit a structured external capture checklist for the first pending
scenario. The checklist must explain what the operator does in USBPcap,
Wireshark, or `tshark`, where the local pcapng belongs, what notes are
required, and which actions remain forbidden.

### Production delta

`wheelctl moza bench-wizard` now includes
`next_operator_step.external_capture_checklist` for
`capture_passive_vendor_sniff`. The checklist records the operator-owned
external capture boundary, local scratch directory, local `capture.pcapng` path,
capture tools, scenario-specific action text, required notes, forbidden actions,
and the claim ceiling. Markdown output renders this as an
`External Capture Checklist` before the no-output `wheelctl hardware`
receipt/summary/bundle commands.

### Non-goals

No hardware output, no OpenRacing HID open, no OpenRacing output or feature
reports, no authorization receipt, no pcap capture, no sniff receipt, no sniff
summary, no bundle artifact, no raw pcap commit, no native-visible promotion,
and no smoke-ready promotion.

### Acceptance

- The current bench-wizard next step still reports
  `capture_passive_vendor_sniff` for `pit-house-open-idle`.
- The step includes `external_capture_checklist.owner=operator_external_capture_tool`.
- The checklist keeps `openracing_output=false`, `openracing_hid_open=false`,
  and `external_app_may_send_output=true`.
- The checklist names the local `capture.pcapng`, the scenario-specific Pit
  House action, operator notes, and forbidden firmware/DFU/driver actions.
- Markdown renders the external capture checklist before the OpenRacing
  no-output artifact commands.

### Proof commands

```powershell
cargo test --locked -p wheelctl --bin wheelctl bench_wizard_points_diagnosed_attempt_03_to_passive_sniff_capture -- --nocapture
cargo test --locked -p wheelctl --bin wheelctl bench_wizard -- --nocapture
cargo run --locked -p wheelctl --bin wheelctl -- moza bench-wizard --lane ci/hardware/moza-r5/2026-05-13 --json-out target/moza-bench-wizard-after-sniff-capture-checklist.json --md-out target/moza-bench-wizard-after-sniff-capture-checklist.md --json
cargo run --locked -p openracing-tools --bin package-surface -- --check
python scripts/policy_file.py
git diff --check
```

### Rollback

Revert only the checklist helper, Markdown rendering, focused assertions, and
this plan entry. Do not remove sniff plan artifacts or hardware receipts.

## Work item: passive-sniff-bundle-json-out

Status: completed
Linked proposal: docs/proposals/OR-PROP-0001-moza-native-visible-lane.md
Linked spec: docs/specs/OR-SPEC-0001-moza-native-visible-lane.md
Linked ADR: docs/adr/0009-hardware-validation-evidence-state-machine.md
Blocks: checked-in passive sniff bundle manifest receipts
Blocked by: n/a

### Goal

Make passive sniff bundle manifests writable as ordinary JSON artifacts outside
the local ZIP bundle.

### Production delta

`wheelctl hardware sniff-bundle` now accepts `--json-out` and writes the same
non-claiming bundle manifest JSON that is embedded in the ZIP. The Moza bench
wizard generated `bundle_sniff_evidence` command includes `--json-out` pointed
at the scenario's `sniff-bundle-manifest.json` path, so later capture evidence
can review the manifest without opening the local ZIP.

### Non-goals

No hardware output, no OpenRacing HID open, no pcap capture, no raw pcap
commit, no sniff receipt, no sniff summary, no checked-in bundle artifact, no
authorization receipt, no native-visible promotion, and no smoke-ready
promotion.

### Acceptance

- `hardware sniff-bundle --json-out` writes a schema-compatible non-claiming
  manifest.
- Generated bench-wizard sniff-bundle commands parse and include `--json-out`.
- Passive sniff navigation still reports `readiness_claim=false`.
- No readiness, native-control, authorization, or output claim changes.

### Proof commands

```powershell
python scripts/cargo_fmt_workspace.py
cargo test --locked -p wheelctl --bin wheelctl sniff_bundle -- --nocapture
cargo test --locked -p wheelctl --bin wheelctl passive_sniff -- --nocapture
cargo test --locked -p wheelctl --bin wheelctl parse_hardware_sniff_bundle -- --nocapture
cargo clippy --locked -p wheelctl --bin wheelctl --all-features -- -D warnings
cargo run --locked -p openracing-tools --bin package-surface -- --check
python scripts/policy_file.py
git diff --check
```

### Rollback

Revert only the `sniff-bundle --json-out` CLI plumbing, generated handoff
command update, focused tests, and this plan entry. Do not remove sniff plans,
receipts, summaries, raw local captures, or controlled-angle evidence.

## Work item: closed-loop-native-motion-ladder

Status: completed
Linked proposal: docs/proposals/OR-PROP-0001-moza-native-visible-lane.md
Linked spec: docs/specs/OR-SPEC-0001-moza-native-visible-lane.md
Linked ADR: docs/adr/0009-hardware-validation-evidence-state-machine.md
Blocks: next no-output protocol investigation
Blocked by: n/a

### Goal

Replace blind standard-PIDFF-family retry guidance with one bounded
feedback-driven `closed-loop-pidff-angle-v1` rung that samples steering angle,
computes torque from target error, clamps force, records observed motion, and
always performs final cleanup.

### Production delta

`wheelctl moza controlled-angle-smoke` supports
`closed-loop-pidff-angle-v1`. The current lane records its no-output preflight,
exact consumed authorization, real hardware output receipt, and no-output
failure analysis. The attempt wrote 672 bounded PIDFF reports with zero write
errors, sent final Stop All/final zero, and failed safely below the 1 degree
visible-motion threshold.

### Non-goals

No native-visible promotion, no rerun permission, no force escalation, no
longer dwell, no 3/5/30/90 degree attempt, no direct report `0x20`, no high
torque, no serial config, no firmware/DFU, and no Pit House, SimHub, simulator,
or passive sniff prerequisite for native control.

### Acceptance

- The preflight opens no HID device and sends no reports.
- The real attempt is bound to exact authorization and consumes it.
- The output receipt records commanded target, torque envelope, observed angle,
  stop reason, write accounting, and final-zero proof.
- The failure analysis keeps native-visible and smoke-ready unclaimed and
  requires no-output protocol evidence before any future output plan.

### Proof commands

```powershell
python scripts/cargo_fmt_workspace.py
cargo test --locked -p wheelctl --bin wheelctl controlled_angle -- --nocapture
cargo test --locked -p wheelctl --bin wheelctl native_visible -- --nocapture
cargo clippy --locked -p wheelctl --bin wheelctl --all-features -- -D warnings
cargo run --locked -p openracing-tools --bin package-surface -- --check
python scripts/policy_file.py
git diff --check
```

### Rollback

Revert only the closed-loop profile code, tests, docs, and closed-loop lane
artifacts. Do not remove earlier attempt receipts or passive sniff plans.

## Work item: passive-sniff-report-classification

Status: completed
Linked proposal: docs/proposals/OR-PROP-0001-moza-native-visible-lane.md
Linked spec: docs/specs/OR-SPEC-0001-moza-native-visible-lane.md
Linked ADR: docs/adr/0009-hardware-validation-evidence-state-machine.md
Blocks: passive vendor-control evidence review before any future output family
Blocked by: n/a

### Goal

Make `wheelctl hardware sniff-summary` classify observed passive USB report IDs
so Pit House, SimHub, and simulator captures can distinguish standard PIDFF
traffic from vendor/device-specific host-to-device candidates.

### Production delta

The summary receipt now includes per-report classification and a top-level
classification summary. Host-to-device report IDs matching the standard PIDFF
output/control set are labeled as standard PIDFF; other host-to-device report
IDs are conservative vendor/device-specific decode candidates; device-to-host
reports are input/status traffic. The `tshark` JSON reader now asks for the
full JSON tree so descriptor-only packets are not hidden by protocol-layer
filters.

### Non-goals

No hardware output, no OpenRacing HID open, no pcap capture, no raw pcap
commit, no sniff receipt, no checked-in capture evidence, no authorization
receipt, no native-visible promotion, no smoke-ready promotion, no vendor
output plan, no serial config, no firmware, and no DFU.

### Acceptance

- `sniff-summary.schema.json` requires per-report classification and a
  non-claiming classification summary.
- Unknown host-to-device reports are marked as decode candidates, not native
  control evidence.
- Standard PIDFF report IDs are labeled but do not create readiness claims.
- Device-to-host reports are classified as input/status.
- Full `tshark -T json` descriptor trees remain parseable.

### Proof commands

```powershell
python scripts/cargo_fmt_workspace.py
cargo test --locked -p wheelctl --bin wheelctl hardware_sniff_summary -- --nocapture
cargo test --locked -p wheelctl --bin wheelctl sniff_summary -- --nocapture
cargo clippy --locked -p wheelctl --bin wheelctl --all-features -- -D warnings
cargo run --locked -p openracing-tools --bin package-surface -- --check
python scripts/policy_file.py
git diff --check
```

### Rollback

Revert only the sniff-summary classification fields, schema expansion, focused
tests, and this plan entry. Do not remove sniff plans or controlled-angle
evidence.

## Work item: pit-house-open-idle-sniff-evidence

Status: completed
Linked proposal: docs/proposals/OR-PROP-0001-moza-native-visible-lane.md
Linked spec: docs/specs/OR-SPEC-0001-moza-native-visible-lane.md
Linked ADR: docs/adr/0009-hardware-validation-evidence-state-machine.md
Blocks: passive protocol evidence review before any future output family
Blocked by: n/a

### Goal

Record the first Pit House open-idle passive USB sniff capture as checked-in,
non-claiming support evidence after the report-classification path landed.

### Production delta

Added `sniff-receipt.json`, `sniff-summary.json`, and
`sniff-bundle-manifest.json` under
`ci/hardware/sniff/moza-r5/2026-05-13/pit-house-open-idle`. The summary records
only device-to-host report `0x01`, no standard PIDFF output reports, and no
vendor/device-specific host-to-device decode candidates. The bundle manifest
keeps the raw pcapng and ZIP local while preserving hashes for the plan,
receipt, summary, operator notes, and filtered pcap hash file.

### Non-goals

No hardware output, no OpenRacing HID open, no raw pcap commit, no bundle ZIP
commit, no vendor report decode claim, no native-control claim, no
native-visible promotion, no smoke-ready promotion, no serial config, no
firmware, and no DFU.

### Acceptance

- The receipt records `openracing_hardware_output=false`,
  `openracing_hid_device_opened=false`, and all readiness claims false.
- The classified summary records only input/status traffic for report `0x01`
  and `decode_recommended=false`.
- The bundle manifest records `includes_raw_pcapng=false`.
- The artifact index records `pit-house-open-idle` as present non-claiming
  evidence while leaving native-visible and smoke-ready blocked.

### Proof commands

```powershell
cargo run --locked -p wheelctl --bin wheelctl -- hardware sniff-receipt --plan ci/hardware/sniff/moza-r5/2026-05-13/pit-house-open-idle/sniff-plan.json --pcapng target/sniff/pit-house-open-idle/capture.pcapng --operator Steven --app "MOZA Pit House" --scenario pit-house-open-idle --evidence <operator-evidence> --json-out ci/hardware/sniff/moza-r5/2026-05-13/pit-house-open-idle/sniff-receipt.json --json
cargo run --locked -p wheelctl --bin wheelctl -- hardware sniff-summary --pcapng target/sniff/pit-house-open-idle/capture.pcapng --vendor 0x346E --product 0x0004 --json-out ci/hardware/sniff/moza-r5/2026-05-13/pit-house-open-idle/sniff-summary.json --md-out target/sniff/pit-house-open-idle/sniff-summary.md --json
cargo run --locked -p wheelctl --bin wheelctl -- hardware sniff-bundle --plan ci/hardware/sniff/moza-r5/2026-05-13/pit-house-open-idle/sniff-plan.json --receipt ci/hardware/sniff/moza-r5/2026-05-13/pit-house-open-idle/sniff-receipt.json --summary ci/hardware/sniff/moza-r5/2026-05-13/pit-house-open-idle/sniff-summary.json --operator-notes target/sniff/pit-house-open-idle/operator-notes.md --out target/sniff/pit-house-open-idle/openracing-sniff-bundle.zip --json-out ci/hardware/sniff/moza-r5/2026-05-13/pit-house-open-idle/sniff-bundle-manifest.json --json
cargo run --locked -p wheelctl --bin wheelctl -- moza artifact-index --lane ci/hardware/moza-r5/2026-05-13 --json-out target/moza-current/artifact-index-after-pit-house-open-idle.json --md-out ci/hardware/moza-r5/2026-05-13/index.md --json
cargo run --locked -p wheelctl --bin wheelctl -- moza verify-bundle --lane ci/hardware/moza-r5/2026-05-13 --stage native-visible-ready --json-out target/moza-current/native-visible-after-pit-house-open-idle.json --json
if ($LASTEXITCODE -ne 4) { throw "expected native-visible verifier to remain blocked" }
cargo run --locked -p openracing-tools --bin package-surface -- --check
python scripts/policy_file.py
git diff --check
```

### Rollback

Remove only the Pit House open-idle receipt, summary, bundle manifest,
artifact-index refresh, and this plan entry. Do not remove sniff plans,
controlled-angle receipts, or local raw pcap artifacts.

## Work item: passive-sniff-operator-notes-template

Status: completed
Linked proposal: docs/proposals/OR-PROP-0001-moza-native-visible-lane.md
Linked spec: docs/specs/OR-SPEC-0001-moza-native-visible-lane.md
Linked ADR: docs/adr/0009-hardware-validation-evidence-state-machine.md
Blocks: passive capture handoff hardening before remaining external evidence captures
Blocked by: n/a

### Goal

Make passive sniff capture handoffs produce a repeatable operator-notes
template from the accepted sniff plan, and require refreshed plans to carry the
capture checklist fields that keep no-output claim boundaries reviewable.

### Production delta

Added `wheelctl hardware sniff-notes-template --plan <sniff-plan.json> --out
<operator-notes.md>`. The command reads a validated non-claiming sniff plan,
writes a Markdown operator-notes template, and prints a non-claiming receipt to
stdout. It does not open HID, send output, create authorization, or write a
readiness receipt.

Expanded `sniff-plan.schema.json`, generated sniff plans, and plan validation
with:

- `pre_capture_checklist`
- `post_capture_checklist`
- `operator_notes_required`
- `raw_pcap_commit_default=false`

The bench wizard now includes the no-output `sniff-notes-template` command in
passive capture handoffs, and stale sniff plans without the notes-template
handoff are not accepted as navigation-ready. The Pit House open-idle bundle
manifest was refreshed because the checked-in plan hash changed; raw pcapng and
bundle ZIP remain local.

### Non-goals

No hardware output, no OpenRacing HID open, no raw pcap commit, no bundle ZIP
commit, no vendor report decode claim, no native-control claim, no
native-visible promotion, no smoke-ready promotion, no serial config, no
firmware, and no DFU.

### Acceptance

- `sniff-plan` artifacts include operator capture handoff fields and
  `raw_pcap_commit_default=false`.
- `sniff-notes-template` renders required note fields and claim-boundary
  confirmations from the validated plan.
- Bench-wizard passive sniff commands parse and remain in the no-output
  `wheelctl hardware` namespace.
- Stale sniff plans missing the notes-template handoff are rejected.
- Native-visible and smoke-ready remain blocked.

### Proof commands

```powershell
python scripts/cargo_fmt_workspace.py
cargo test --locked -p wheelctl --bin wheelctl sniff_plan -- --nocapture
cargo test --locked -p wheelctl --bin wheelctl sniff_notes_template -- --nocapture
cargo test --locked -p wheelctl --bin wheelctl bench_wizard_sniff_next_operator_commands_parse -- --nocapture
cargo clippy --locked -p wheelctl --bin wheelctl --all-features -- -D warnings
.\target\debug\wheelctl.exe moza verify-bundle --lane ci/hardware/moza-r5/2026-05-13 --stage native-visible-ready --json-out target/moza-native-visible-after-sniff-notes-template.json --json
if ($LASTEXITCODE -eq 4) { exit 0 } else { throw "expected native-visible verifier to remain blocked" }
cargo run --locked -p openracing-tools --bin package-surface -- --check
python scripts/policy_file.py
git diff --check
```

### Rollback

Remove only the `sniff-notes-template` CLI path, sniff-plan checklist schema and
artifact refreshes, bench-wizard handoff command, refreshed Pit House
open-idle bundle manifest, and this work-item entry. Do not remove passive
sniff receipts, summaries, closed-loop output receipts, or raw local capture
artifacts.

## Work item: pit-house-install-source-guidance

Status: completed
Linked proposal: docs/proposals/OR-PROP-0001-moza-native-visible-lane.md
Linked spec: docs/specs/OR-SPEC-0001-moza-native-visible-lane.md
Linked ADR: docs/adr/0009-hardware-validation-evidence-state-machine.md
Blocks: Pit House witness/coexistence operator navigation
Blocked by: n/a

### Goal

Surface the official MOZA Pit House download/source guidance in no-output
availability receipts and normal operator navigation, without treating Pit
House as a native-control dependency.

### Production delta

Add the official MOZA Pit House Downloads support page and install-source
guidance to `pit-house-availability` receipts, Pit House compatibility summary
JSON, artifact-index Markdown, and bench-wizard Markdown.

### Non-goals

No install, launch, passive capture, HID open, hardware output, Pit House
coexistence proof, smoke-ready claim, native-visible claim, package-manager
assumption, firmware/DFU guidance, or semantic control promotion.

### Acceptance

- Availability receipts include the official MOZA Pit House Downloads page and
  install guidance while preserving `satisfies_pit_house_coexistence=false`.
- Artifact-index and bench-wizard Pit House sections surface the guidance as
  external-smoke navigation only.
- Existing availability receipts without the new fields remain parseable and
  default to the same guidance.
- Native-visible verification remains blocked.

### Proof commands

```powershell
python scripts/cargo_fmt_workspace.py
cargo test --locked -p wheelctl --bin wheelctl pit_house -- --nocapture
cargo test --locked -p wheelctl --bin wheelctl artifact_index -- --nocapture
cargo test --locked -p wheelctl --bin wheelctl bench_wizard -- --nocapture
cargo clippy --locked -p wheelctl --bin wheelctl --all-features -- -D warnings
.\target\debug\wheelctl.exe moza verify-bundle --lane ci/hardware/moza-r5/2026-05-13 --stage native-visible-ready --json-out target/moza-native-visible-after-pit-house-install-guidance.json --json
if ($LASTEXITCODE -eq 4) { exit 0 } else { throw "expected native-visible verifier to remain blocked" }
cargo run --locked -p openracing-tools --bin package-surface -- --check
python scripts/policy_file.py
git diff --check
```

### Rollback

Remove only the Pit House download/source fields, Markdown rendering, tests, and
this work-item entry. Do not alter Pit House sniff receipts, availability
snapshots, coexistence gates, native-control receipts, or semantic-control
artifacts.

## Work item: brake-hbp-semantic-promotion

Status: completed
Linked proposal: docs/proposals/OR-PROP-0001-moza-native-visible-lane.md
Linked spec: docs/specs/OR-SPEC-0001-moza-native-visible-lane.md
Linked ADR: docs/adr/0009-hardware-validation-evidence-state-machine.md
Blocks: input topology cleanup for checked-in SR-P/HBP captures
Blocked by: n/a

### Goal

Promote only the checked-in isolated R5 V1 through-hub brake and HBP
handbrake evidence from generic auxiliary slots to parser semantic axes.

### Production delta

Map the live R5 V1 extended byte-11 axis to `brake_u16` and byte-13 axis to
`handbrake_u16`, then regenerate the no-output capture validation,
fixture-promotion, lane-analysis, role-status, blocked native-visible verifier,
and artifact-index receipts from the stored captures.

### Non-goals

No clutch semantic promotion, wheel-button naming, rotary semantic promotion,
native-visible promotion, smoke-ready promotion, Pit House coexistence claim,
hardware output, authorization receipt, HID open, or new capture.

### Acceptance

- Brake reports `semantic_status=proven` with `moving_required_axes=["brake_u16"]`.
- Handbrake reports `semantic_status=proven` with
  `moving_required_axes=["handbrake_u16"]`.
- Clutch remains `generic_aux` and `input_semantic_mapping_complete=false`.
- Native-visible verification remains blocked.

### Proof commands

```powershell
python scripts/cargo_fmt_workspace.py
cargo test --locked -p racing-wheel-moza-wheelbase-report --all-features -- --nocapture
cargo test --locked -p wheelctl --bin wheelctl input_role -- --nocapture
cargo test --locked -p wheelctl --bin wheelctl artifact_index -- --nocapture
cargo run --locked -p wheelctl --bin wheelctl -- moza validate-captures --lane ci/hardware/moza-r5/2026-05-13 --json-out ci/hardware/moza-r5/2026-05-13/parser-fixture-validation.json --json
cargo run --locked -p wheelctl --bin wheelctl -- moza promote-fixtures --lane ci/hardware/moza-r5/2026-05-13 --fixture-dir crates/hid-moza-protocol/fixtures/moza-r5-2026-05-13 --json-out ci/hardware/moza-r5/2026-05-13/fixture-promotion.json --overwrite --json
cargo run --locked -p wheelctl --bin wheelctl -- moza analyze-lane --lane ci/hardware/moza-r5/2026-05-13 --json-out ci/hardware/moza-r5/2026-05-13/lane-capture-analysis.json --json
cargo run --locked -p wheelctl --bin wheelctl -- moza sync-role-status --lane ci/hardware/moza-r5/2026-05-13 --json-out ci/hardware/moza-r5/2026-05-13/role-status-sync.json --json
.\target\debug\wheelctl.exe moza verify-bundle --lane ci/hardware/moza-r5/2026-05-13 --stage native-visible-ready --json-out ci/hardware/moza-r5/2026-05-13/native-visible-verification.json --json
if ($LASTEXITCODE -eq 4) { exit 0 } else { throw "expected native-visible verifier to remain blocked" }
cargo run --locked -p wheelctl --bin wheelctl -- moza artifact-index --lane ci/hardware/moza-r5/2026-05-13 --json-out target/moza-artifact-index-after-brake-hbp-semantics.json --md-out ci/hardware/moza-r5/2026-05-13/index.md --json
.\target\debug\wheelctl.exe moza verify-bundle --lane ci/hardware/moza-r5/2026-05-13 --stage native-visible-ready --json-out target/moza-native-visible-after-brake-hbp-semantics.json --json
if ($LASTEXITCODE -eq 4) { exit 0 } else { throw "expected native-visible verifier to remain blocked" }
cargo clippy --locked -p racing-wheel-moza-wheelbase-report --all-features -- -D warnings
cargo clippy --locked -p wheelctl --bin wheelctl --all-features -- -D warnings
cargo run --locked -p openracing-tools --bin package-surface -- --check
python scripts/policy_file.py
git diff --check
```

### Rollback

Remove only the R5 V1 brake/HBP parser mapping, regenerated no-output receipts,
tests, and this work-item entry. Do not remove source captures, closed-loop
output receipts, Pit House artifacts, or native-visible failure evidence.

## Work item: pit-house-full-controls-sniff-evidence

Status: completed
Linked proposal: docs/proposals/OR-PROP-0001-moza-native-visible-lane.md
Linked spec: docs/specs/OR-SPEC-0001-moza-native-visible-lane.md
Linked ADR: docs/adr/0009-hardware-validation-evidence-state-machine.md
Blocks: passive protocol evidence review before any future output family
Blocked by: n/a

### Goal

Record the Pit House full-controls passive USB sniff capture as checked-in,
non-claiming protocol/support evidence after the open-idle capture.

### Production delta

Added the `pit-house-full-controls` passive sniff scenario, generated its
`sniff-plan.json`, and checked in `sniff-receipt.json`,
`sniff-summary.json`, and `sniff-bundle-manifest.json` under
`ci/hardware/sniff/moza-r5/2026-05-13/pit-house-full-controls`.

The receipt records the operator's 60.131 second Pit House 1.3.8.38 release
capture, the action order of wheel, HBP handbrake, gas, brake, clutch, and
wheel-button movement, and confirms that OpenRacing opened no HID device and
sent no output, feature, serial, firmware, or DFU commands. The summary records
only device-to-host input/status report `0x01`, no standard PIDFF output
reports, and no vendor/device-specific host-to-device decode candidates.

### Non-goals

No hardware output, no OpenRacing HID open, no raw pcap commit, no bundle ZIP
commit, no vendor report decode claim, no native-control claim, no
native-visible promotion, no smoke-ready promotion, no Pit House coexistence
claim, no simulator claim, no semantic-control promotion, no serial config, no
firmware, and no DFU.

### Acceptance

- The scenario taxonomy accepts `pit-house-full-controls` for plans and
  receipts.
- The receipt records `openracing_hardware_output=false`,
  `openracing_hid_device_opened=false`, and all readiness claims false.
- The classified summary records only input/status traffic for report `0x01`
  and `decode_recommended=false`.
- The bundle manifest records `includes_raw_pcapng=false`.
- The artifact index records `pit-house-full-controls` as present non-claiming
  evidence while leaving native-visible, smoke-ready, coexistence, and release
  claims blocked.

### Proof commands

```powershell
cargo run --locked -p wheelctl --bin wheelctl -- hardware sniff-plan --family moza-r5 --scenario pit-house-full-controls --lane ci/hardware/moza-r5/2026-05-13 --operator Steven --device-note "Moza R5 PID 0x0004 with KS/ES wheels, SR-P pedals, and HBP handbrake attached through the R5 hub" --capture-tool usbpcap --capture-tool wireshark --capture-tool tshark --platform-hint windows --json-out ci/hardware/sniff/moza-r5/2026-05-13/pit-house-full-controls/sniff-plan.json --json
cargo run --locked -p wheelctl --bin wheelctl -- hardware sniff-receipt --plan ci/hardware/sniff/moza-r5/2026-05-13/pit-house-full-controls/sniff-plan.json --pcapng target/sniff/pit-house-full-controls/capture.pcapng --operator Steven --app "MOZA Pit House 1.3.8.38 release" --scenario pit-house-full-controls --evidence <operator-evidence> --json-out ci/hardware/sniff/moza-r5/2026-05-13/pit-house-full-controls/sniff-receipt.json --json
cargo run --locked -p wheelctl --bin wheelctl -- hardware sniff-summary --pcapng target/sniff/pit-house-full-controls/capture.pcapng --vendor 0x346E --product 0x0004 --json-out ci/hardware/sniff/moza-r5/2026-05-13/pit-house-full-controls/sniff-summary.json --md-out target/sniff/pit-house-full-controls/sniff-summary.md --json
cargo run --locked -p wheelctl --bin wheelctl -- hardware sniff-bundle --plan ci/hardware/sniff/moza-r5/2026-05-13/pit-house-full-controls/sniff-plan.json --receipt ci/hardware/sniff/moza-r5/2026-05-13/pit-house-full-controls/sniff-receipt.json --summary ci/hardware/sniff/moza-r5/2026-05-13/pit-house-full-controls/sniff-summary.json --operator-notes target/sniff/pit-house-full-controls/operator-notes.md --out target/sniff/pit-house-full-controls/openracing-sniff-bundle.zip --json-out ci/hardware/sniff/moza-r5/2026-05-13/pit-house-full-controls/sniff-bundle-manifest.json --json
cargo run --locked -p wheelctl --bin wheelctl -- moza artifact-index --lane ci/hardware/moza-r5/2026-05-13 --json-out target/moza-current/artifact-index-after-pit-house-full-controls.json --md-out ci/hardware/moza-r5/2026-05-13/index.md --json
.\target\debug\wheelctl.exe moza verify-bundle --lane ci/hardware/moza-r5/2026-05-13 --stage native-visible-ready --json-out target/moza-current/native-visible-after-pit-house-full-controls.json --json
if ($LASTEXITCODE -eq 4) { exit 0 } else { throw "expected native-visible verifier to remain blocked" }
cargo test --locked -p wheelctl --bin wheelctl passive_sniff -- --nocapture
cargo test --locked -p wheelctl --bin wheelctl artifact_index -- --nocapture
cargo test --locked -p wheelctl --bin wheelctl bench_wizard -- --nocapture
cargo clippy --locked -p wheelctl --bin wheelctl --all-features -- -D warnings
cargo run --locked -p openracing-tools --bin package-surface -- --check
python scripts/policy_file.py
git diff --check
```

### Rollback

Remove only the `pit-house-full-controls` scenario wiring, generated
non-claiming sniff artifacts, artifact-index refresh, tests, and this work-item
entry. Do not remove Pit House open-idle evidence, controlled-angle receipts,
semantic-input artifacts, or local raw pcap artifacts.

## Work item: moza-current-state-status-refresh

Status: completed
Linked proposal: docs/proposals/OR-PROP-0001-moza-native-visible-lane.md
Linked spec: docs/specs/OR-SPEC-0001-moza-native-visible-lane.md
Linked ADR: docs/adr/0009-hardware-validation-evidence-state-machine.md
Blocks: accurate source-of-truth handoff after semantic-input and passive-sniff slices
Blocked by: n/a

### Goal

Refresh the human source-of-truth status docs so they match the current
checked-in lane after the brake/HBP semantic promotion and Pit House
full-controls sniff evidence slices.

### Production delta

Update the active goal work-item ledger, this implementation plan, the blocked
handoff, and the completion audit to record that brake and HBP handbrake are
parser-proven, SR-P clutch remains generic auxiliary evidence, Pit House
availability is recorded, and passive sniff navigation has two recorded
non-claiming scenarios out of six.

### Non-goals

No hardware output, HID open, new capture, raw pcap commit, receipt rewrite,
native-control claim, native-visible promotion, smoke-ready promotion, Pit
House coexistence claim, simulator claim, firmware, serial config, DFU, or
release-ready claim.

### Acceptance

- The handoff names `pit-house-open-idle` and `pit-house-full-controls` as
  recorded non-claiming passive sniff evidence.
- The completion audit records brake and HBP handbrake as parser-proven while
  keeping SR-P clutch generic and semantic mapping incomplete.
- Pit House availability and official install-source guidance are surfaced as
  non-claiming navigation only.
- Native-visible verification remains blocked.

### Proof commands

```powershell
cargo run --locked -p wheelctl --bin wheelctl -- moza artifact-index --lane ci/hardware/moza-r5/2026-05-13 --json-out target/moza-current/artifact-index-after-status-refresh.json --md-out target/moza-current/artifact-index-after-status-refresh.md --json
cargo run --locked -p wheelctl --bin wheelctl -- moza bench-wizard --lane ci/hardware/moza-r5/2026-05-13 --json-out target/moza-current/bench-wizard-after-status-refresh.json --md-out target/moza-current/bench-wizard-after-status-refresh.md --json
cargo run --locked -p wheelctl --bin wheelctl -- moza verify-bundle --lane ci/hardware/moza-r5/2026-05-13 --stage native-visible-ready --json-out target/moza-current/native-visible-after-status-refresh.json --json
if ($LASTEXITCODE -eq 4) { exit 0 } else { throw "expected native-visible verifier to remain blocked" }
cargo run --locked -p openracing-tools --bin package-surface -- --check
python scripts/policy_file.py
git diff --check
```

### Rollback

Remove only this source-of-truth status refresh from the active goal, plan,
handoff, and audit. Do not alter checked-in hardware receipts, sniff artifacts,
parser fixtures, or generated lane indexes.

## Work item: refresh-pre-output-readiness-receipt

Status: completed
Linked proposal: docs/proposals/OR-PROP-0001-moza-native-visible-lane.md
Linked spec: docs/specs/OR-SPEC-0001-moza-native-visible-lane.md
Linked ADR: docs/adr/0009-hardware-validation-evidence-state-machine.md
Blocks: current no-output readiness navigation
Blocked by: n/a

### Goal

Regenerate the checked-in `pre-output-readiness.json` receipt so the no-output
readiness summary matches the current lane receipts after the brake/HBP
semantic promotion, Pit House install-source guidance, and closed-loop
undertravel evidence slices.

### Production delta

Refresh `ci/hardware/moza-r5/2026-05-13/pre-output-readiness.json` from
`wheelctl moza pre-output-readiness`. The regenerated receipt records six
parser-proven input roles, one remaining generic clutch role, current Pit House
availability/source guidance, native response proven, and native visible motion
still false.

### Non-goals

No hardware output, HID open, new capture, raw pcap commit, authorization,
native-visible promotion, smoke-ready promotion, Pit House coexistence claim,
simulator claim, firmware, serial config, DFU, or release-ready claim.

### Acceptance

- `pre-output-readiness.json` reports brake and HBP handbrake as proven axes.
- SR-P clutch remains `generic_aux` with `readiness_claim=false`.
- Pit House availability/source guidance is recorded without proving
  coexistence.
- Native-visible verification remains blocked.

### Proof commands

```powershell
cargo run --locked -p wheelctl --bin wheelctl -- moza pre-output-readiness --lane ci/hardware/moza-r5/2026-05-13 --json-out ci/hardware/moza-r5/2026-05-13/pre-output-readiness.json --json
cargo run --locked -p wheelctl --bin wheelctl -- moza verify-bundle --lane ci/hardware/moza-r5/2026-05-13 --stage native-visible-ready --json-out target/moza-current/native-visible-after-pre-output-refresh.json --json
if ($LASTEXITCODE -eq 4) { exit 0 } else { throw "expected native-visible verifier to remain blocked" }
cargo run --locked -p openracing-tools --bin package-surface -- --check
python scripts/policy_file.py
git diff --check
```

### Rollback

Restore the previous pre-output readiness receipt and remove this work-item
entry. Do not alter parser mappings, Pit House receipts, sniff artifacts, or
native-visible undertravel receipts.

## Work item: pit-house-open-standard-case-evidence

Status: completed
Linked proposal: docs/proposals/OR-PROP-0001-moza-native-visible-lane.md
Linked spec: docs/specs/OR-SPEC-0001-moza-native-visible-lane.md
Linked ADR: docs/adr/0009-hardware-validation-evidence-state-machine.md
Blocks: partial Pit House coexistence progress before final parent proof
Blocked by: Pit House running and a verifier-matching process/window snapshot

### Goal

Record the Pit House open-idle standard-mode coexistence case as checked-in,
non-claiming external compatibility evidence while the native-control
vendor-authority lane remains blocked on fresh bench-clear and exclusive serial
access.

### Production delta

Add the process/window snapshot evidence, observation receipt, and generated
case artifact for `pit_house_open_idle_standard` under
`ci/hardware/moza-r5/2026-05-13`, then refresh the lane artifact index. The case
links to the existing `init-standard.json` source receipt and records
`result=standard_ok` without creating the final parent
`pit-house-coexistence.json` proof.

### Non-goals

No hardware output, HID open, serial open, authorization, vendor-authority
attempt, simulator artifact, Pit House parent coexistence proof,
native-visible promotion, smoke-ready promotion, firmware, DFU, high torque, or
release-ready claim.

### Acceptance

- `pit-house-evidence-open-standard.json` records a matching Pit House
  process/window snapshot with no HID, FFB, serial config, firmware, or DFU
  commands.
- `pit-house-observation-open-standard.json` references that snapshot using
  `evidence_kind=process_window_snapshot`.
- `pit-house-open-standard.json` records
  `case=pit_house_open_idle_standard`, `result=standard_ok`,
  `source_receipt=init-standard.json`, `high_torque=false`, and no
  serial/firmware/DFU commands.
- Artifact-index and bench-wizard navigation show 2/5 Pit House cases recorded,
  while `pit-house-coexistence.json`, native-visible, and smoke-ready remain
  blocked.

### Proof commands

```powershell
wheelctl moza pit-house-evidence --case open-standard --operator Steven --evidence "Pit House open and idle while standard mode completed." --require-match --json-out ci/hardware/moza-r5/2026-05-13/pit-house-evidence-open-standard.json --overwrite --json
wheelctl moza pit-house-observation --case open-standard --evidence-kind process-window-snapshot --evidence-artifact pit-house-evidence-open-standard.json --operator Steven --evidence "Pit House open and idle while standard mode completed." --json-out ci/hardware/moza-r5/2026-05-13/pit-house-observation-open-standard.json --overwrite --json
wheelctl moza pit-house-case --lane ci/hardware/moza-r5/2026-05-13 --case open-standard --observation-artifact pit-house-observation-open-standard.json --evidence "Pit House open and idle; standard mode completed without conflict." --json-out ci/hardware/moza-r5/2026-05-13/pit-house-open-standard.json --overwrite --json
wheelctl moza artifact-index --lane ci/hardware/moza-r5/2026-05-13 --json-out target/moza-current/artifact-index-after-pit-house-open-standard.json --md-out ci/hardware/moza-r5/2026-05-13/index.md --json
wheelctl moza bench-wizard --lane ci/hardware/moza-r5/2026-05-13 --json-out target/moza-current/bench-wizard-after-pit-house-open-standard.json --md-out target/moza-current/bench-wizard-after-pit-house-open-standard.md --json
wheelctl moza verify-bundle --lane ci/hardware/moza-r5/2026-05-13 --stage native-visible-ready --json-out target/moza-current/native-visible-after-pit-house-open-standard.json --json
if ($LASTEXITCODE -eq 4) { exit 0 } else { throw "expected native-visible verifier to remain blocked" }
cargo test --locked -p wheelctl --bin wheelctl pit_house -- --nocapture
cargo test --locked -p wheelctl --bin wheelctl artifact_index -- --nocapture
cargo test --locked -p wheelctl --bin wheelctl bench_wizard -- --nocapture
cargo run --locked -p openracing-tools --bin package-surface -- --check
python scripts/policy_file.py
git diff --check
```

### Rollback

Remove only the open-standard Pit House evidence, observation, case artifact,
artifact-index refresh, and this work-item entry. Do not alter closed Pit House,
Pit House sniff, native-control, native-visible, or simulator receipts.

## Work item: refresh-pre-output-readiness-after-pit-house-open-standard

Status: completed
Linked proposal: docs/proposals/OR-PROP-0001-moza-native-visible-lane.md
Linked spec: docs/specs/OR-SPEC-0001-moza-native-visible-lane.md
Linked ADR: docs/adr/0009-hardware-validation-evidence-state-machine.md
Blocks: current pre-output navigation after the Pit House open-standard case
Blocked by: checked-in `pit-house-open-standard.json`

### Goal

Refresh the generated pre-output readiness receipt so it reflects the
checked-in Pit House open-standard case while preserving all native-visible and
smoke-ready claim boundaries.

### Production delta

Regenerate `ci/hardware/moza-r5/2026-05-13/pre-output-readiness.json` with
`wheelctl moza pre-output-readiness`. The receipt now records 2 of 5 Pit House
compatibility cases, keeps the parent `pit-house-coexistence.json` gate failed,
and leaves native-visible motion unproven.

### Non-goals

No hardware output, HID open, serial open, authorization receipt, vendor
authority attempt, Pit House parent coexistence proof, native-visible
promotion, smoke-ready promotion, simulator artifact, firmware, DFU, high
torque, or release-ready claim.

### Acceptance

- `pre-output-readiness.json` records `recorded_case_count=2` for Pit House
  compatibility.
- `pit_house_open_idle_standard` is no longer listed as a missing Pit House
  case.
- `pit_house_coexistence_claimed=false` and `readiness_claim=false` remain set.
- Native-visible verification still fails closed.

### Proof commands

```powershell
cargo run --locked -p wheelctl --bin wheelctl -- moza pre-output-readiness --lane ci/hardware/moza-r5/2026-05-13 --json-out ci/hardware/moza-r5/2026-05-13/pre-output-readiness.json --json
cargo run --locked -p wheelctl --bin wheelctl -- moza verify-bundle --lane ci/hardware/moza-r5/2026-05-13 --stage native-visible-ready --json-out target/moza-current/native-visible-after-pit-house-open-standard-pre-output-refresh.json --json
if ($LASTEXITCODE -eq 4) { exit 0 } else { throw "expected native-visible verifier to remain blocked" }
cargo run --locked -p openracing-tools --bin package-surface -- --check
python scripts/policy_file.py
git diff --check
```

### Rollback

Restore the previous pre-output readiness receipt and remove this work-item
entry. Do not alter Pit House case receipts, vendor-authority navigation,
native-control receipts, native-visible undertravel evidence, or simulator
artifacts.

## Work item: post-authority-pidff-regression-doc-refresh

Status: completed
Linked proposal: docs/proposals/OR-PROP-0001-moza-native-visible-lane.md
Linked specs:
- docs/specs/OR-SPEC-0001-moza-native-visible-lane.md
- docs/specs/OR-SPEC-0002-moza-r5-vendor-authority-test-lane.md
Linked ADR: docs/adr/0009-hardware-validation-evidence-state-machine.md
Blocks: accurate no-output handoff after the consumed vendor-authority attempt
Blocked by: n/a

### Goal

Refresh source-of-truth docs after the post-authority PIDFF response comparison
recorded a regression rather than a native-visible unlock.

### Production delta

Add `docs/hardware/moza-r5-post-authority-pidff-response.md` and refresh the
handoff/completion-audit current-state text so the lane records
`post_authority_pidff_response_regressed`, keeps native-visible blocked, and
returns to no-output protocol review before any future output family.

### Non-goals

No hardware output, no authorization receipt, no vendor-authority retry, no
post-authority PIDFF rerun, no force increase, no longer dwell, no larger angle,
no direct HID report `0xaf`, no high torque, no serial config, no firmware, no
DFU, and no readiness promotion.

### Acceptance

- Docs identify the consumed vendor-authority attempt and post-authority PIDFF
  response comparison as non-claiming evidence.
- The recorded comparison values are preserved: baseline
  `0.18127718013275285`, post-authority `0.032959487296864154`, and delta
  change `-0.1483176928358887`.
- Handoff no longer points at pre-#664 state as the current frontier.
- Native-visible verifier remains blocked on `native_actuator_visible_smoke`.

### Proof commands

```powershell
cargo run --locked -p wheelctl --bin wheelctl -- moza bench-wizard --lane ci/hardware/moza-r5/2026-05-13 --json-out target/moza-current/bench-wizard-after-post-authority-doc-refresh.json --md-out target/moza-current/bench-wizard-after-post-authority-doc-refresh.md --json
cargo run --locked -p wheelctl --bin wheelctl -- moza verify-bundle --lane ci/hardware/moza-r5/2026-05-13 --stage native-visible-ready --json-out target/moza-current/native-visible-after-post-authority-doc-refresh.json --json; if ($LASTEXITCODE -eq 4) { exit 0 } else { throw "expected native-visible verifier to remain blocked" }
cargo run --locked -p openracing-tools --bin package-surface -- --check
python scripts/policy_file.py
git diff --check
```

### Rollback

Revert only the docs/source-of-truth refresh. Do not remove the consumed
vendor-authority attempt, post-authority PIDFF receipts, or earlier undertravel
receipts.

## Work item: vendor-protocol-evidence-review

Status: completed
Linked proposal: docs/proposals/OR-PROP-0001-moza-native-visible-lane.md
Linked specs:
- docs/specs/OR-SPEC-0001-moza-native-visible-lane.md
- docs/specs/OR-SPEC-0002-moza-r5-vendor-authority-test-lane.md
Linked ADR: docs/adr/0009-hardware-validation-evidence-state-machine.md
Blocks: reviewed protocol evidence boundary before any future output family
Blocked by: post-authority PIDFF regression receipt and checked-in passive sniff summaries

### Goal

Record a no-output protocol-evidence review that ties together the current
passive sniff summaries, vendor command registry, consumed `estop_set_ffb`
attempt, and post-authority PIDFF response regression before any future
vendor-control output plan.

### Production delta

Add `wheelctl moza vendor-protocol-evidence-review`, its receipt schema, tests,
and the checked-in receipt
`ci/hardware/moza-r5/2026-05-13/vendor-protocol-evidence-review.json`.

The receipt classifies the current state as
`estop_set_ffb_regressed_and_protocol_enable_path_still_undecoded`, records that
only two of six passive sniff scenarios have checked-in summaries, and keeps
`planned_next_output.allowed=false`.

### Non-goals

No HID open, serial open, read-only query send, hardware output, authorization
receipt, `estop_set_ffb` retry, PIDFF rerun, force increase, direct HID report
`0xaf`, high torque, serial config, firmware, DFU, native-control claim,
native-visible claim, smoke-ready claim, Pit House coexistence claim, simulator
claim, or release-ready claim.

### Acceptance

- The command reads checked-in receipts and summaries only.
- Receipt validation pins `native_control_evidence=false`,
  `hardware_output_authorized=false`, `native_visible_ready=false`,
  `smoke_ready=false`, and `planned_next_output.allowed=false`.
- Current passive summaries remain non-claiming and do not decode a new output
  candidate.
- The consumed `estop_set_ffb` attempt and post-authority PIDFF regression are
  preserved as negative evidence, not retried.
- Native-visible verifier remains blocked on `native_actuator_visible_smoke`.

### Proof commands

```powershell
python scripts/cargo_fmt_workspace.py
cargo test --locked -p wheelctl --bin wheelctl vendor_protocol_evidence_review -- --nocapture
cargo test --locked -p wheelctl --test cli_comprehensive_e2e_tests help_snapshots::snapshot_moza_help -- --nocapture
cargo run --locked -p wheelctl --bin wheelctl -- moza vendor-protocol-evidence-review --lane ci/hardware/moza-r5/2026-05-13 --json-out ci/hardware/moza-r5/2026-05-13/vendor-protocol-evidence-review.json --json --overwrite
cargo run --locked -p wheelctl --bin wheelctl -- moza artifact-index --lane ci/hardware/moza-r5/2026-05-13 --json-out target/moza-current/artifact-index-after-vendor-protocol-evidence-review.json --md-out ci/hardware/moza-r5/2026-05-13/index.md --json
cargo run --locked -p wheelctl --bin wheelctl -- moza bench-wizard --lane ci/hardware/moza-r5/2026-05-13 --json-out target/moza-current/bench-wizard-after-vendor-protocol-evidence-review.json --md-out target/moza-current/bench-wizard-after-vendor-protocol-evidence-review.md --json
cargo run --locked -p wheelctl --bin wheelctl -- moza verify-bundle --lane ci/hardware/moza-r5/2026-05-13 --stage native-visible-ready --json-out target/moza-current/native-visible-after-vendor-protocol-evidence-review.json --json; if ($LASTEXITCODE -eq 4) { exit 0 } else { throw "expected native-visible verifier to remain blocked" }
cargo clippy --locked -p wheelctl --bin wheelctl --all-features -- -D warnings
cargo run --locked -p openracing-tools --bin package-surface -- --check
python scripts/policy_file.py
git diff --check
```

### Rollback

Remove the command, schema, tests, and generated
`vendor-protocol-evidence-review.json` receipt. Do not remove the passive sniff
artifacts, vendor command registry, consumed vendor-authority attempt,
post-authority PIDFF response receipts, or prior undertravel evidence.

## Work item: passive-sniff-decode-gap-review

Status: completed
Linked proposal: docs/proposals/OR-PROP-0001-moza-native-visible-lane.md
Linked specs:
- docs/specs/OR-SPEC-0001-moza-native-visible-lane.md
- docs/specs/OR-SPEC-0002-moza-r5-vendor-authority-test-lane.md
Linked ADR: docs/adr/0009-hardware-validation-evidence-state-machine.md
Blocks: reviewed protocol evidence boundary before any future output family
Blocked by: checked-in passive sniff summaries with host-to-device traffic

### Goal

Make passive sniff evidence distinguish "no decoded output candidate" from
"host-to-device traffic exists but the summary cannot map it to report IDs."
The latter is the current checked-in state and should route work to raw payload
decode/export review, not to output.

### Production delta

Extend `wheelctl hardware sniff-summary` classification output for future
summaries with host-to-device classified/unclassified packet counts and a
decode-gap flag. Refresh `wheelctl moza vendor-protocol-evidence-review` so it
derives the same gap from existing checked-in summaries and records it in the
review receipt.

### Non-goals

No HID open, serial open, read-only query send, hardware output, authorization
receipt, PIDFF rerun, force increase, direct HID report `0xaf`, high torque,
serial config, firmware, DFU, native-control claim, native-visible claim,
smoke-ready claim, Pit House coexistence claim, simulator claim, or release-ready
claim.

### Acceptance

- Future `sniff-summary` receipts expose host-to-device packet coverage and
  decode gaps without serializing raw payload samples by default.
- Existing checked-in Pit House summaries remain non-claiming but the protocol
  review records their host-to-device decode gap.
- `planned_next_output.allowed=false`, `native_control_evidence=false`,
  `hardware_output_authorized=false`, `native_visible_ready=false`, and
  `smoke_ready=false` remain pinned.
- Native-visible verifier remains blocked on `native_actuator_visible_smoke`.

### Proof commands

```powershell
python scripts/cargo_fmt_workspace.py
cargo test --locked -p wheelctl --bin wheelctl sniff_summary_surfaces_host_to_device_decode_gaps -- --nocapture
cargo test --locked -p wheelctl --bin wheelctl vendor_protocol_evidence_review -- --nocapture
cargo run --locked -p wheelctl --bin wheelctl -- moza vendor-protocol-evidence-review --lane ci/hardware/moza-r5/2026-05-13 --json-out ci/hardware/moza-r5/2026-05-13/vendor-protocol-evidence-review.json --json --overwrite
cargo run --locked -p wheelctl --bin wheelctl -- moza artifact-index --lane ci/hardware/moza-r5/2026-05-13 --json-out target/moza-current/artifact-index-after-passive-sniff-decode-gap-review.json --md-out ci/hardware/moza-r5/2026-05-13/index.md --json
cargo run --locked -p wheelctl --bin wheelctl -- moza verify-bundle --lane ci/hardware/moza-r5/2026-05-13 --stage native-visible-ready --json-out target/moza-current/native-visible-after-passive-sniff-decode-gap-review.json --json; if ($LASTEXITCODE -eq 4) { exit 0 } else { throw "expected native-visible verifier to remain blocked" }
cargo clippy --locked -p wheelctl --bin wheelctl --all-features -- -D warnings
cargo run --locked -p openracing-tools --bin package-surface -- --check
python scripts/policy_file.py
git diff --check
```

### Rollback

Revert only the decode-gap fields, tests, schema additions, refreshed protocol
review receipt, and source-of-truth updates. Do not remove passive sniff
artifacts, consumed hardware attempts, or prior undertravel evidence.

## Work item: passive-sniff-payload-export-coverage

Status: completed
Linked proposal: docs/proposals/OR-PROP-0001-moza-native-visible-lane.md
Linked spec: docs/specs/OR-SPEC-0001-moza-native-visible-lane.md
Linked ADR: docs/adr/0009-hardware-validation-evidence-state-machine.md
Blocks: raw payload export review before decoded report work
Blocked by: checked-in Pit House passive sniff summaries with host-to-device decode gaps

### Goal

Distinguish a generic host-to-device decode gap from a sharper payload export
gap where tshark reports nonzero `usb.data_len` on host-to-device packets but
the stored summary extracts no payload bytes.

### Production delta

Extend `wheelctl hardware sniff-summary` to parse `usb.data_len` and record
host-to-device data-length packet counts, declared data bytes, extracted
payload counts, extracted payload bytes, missing payload packet counts, and a
payload export gap flag. Refresh the Pit House `open-idle` and `full-controls`
summaries, their bundle manifests, and `vendor-protocol-evidence-review.json`.

That checked-in review recorded 3,248 host-to-device packets with declared USB
data length and no extracted payload bytes across the two completed Pit House
scenarios. This was non-claiming protocol evidence and routed the next work to
tshark/raw pcap export review before another output family.

### Non-goals

No HID open, serial open, read-only query send, hardware output, authorization
receipt, PIDFF rerun, force increase, direct HID report `0xaf`, high torque,
serial config, firmware, DFU, native-control claim, native-visible claim,
smoke-ready claim, Pit House coexistence claim, simulator claim, release-ready
claim, or raw `.pcapng` commit.

### Acceptance

- Future `sniff-summary` receipts expose host-to-device payload export coverage
  without serializing raw payload samples by default.
- The checked-in Pit House summaries record `host_to_device_payload_export_gap=true`.
- `vendor-protocol-evidence-review.json` records
  `host_to_device_payload_export_gap_detected=true`,
  `total_host_to_device_payload_missing_packets=3248`, and
  `total_host_to_device_payload_extracted_bytes=0`.
- `planned_next_output.allowed=false`, `native_control_evidence=false`,
  `hardware_output_authorized=false`, `native_visible_ready=false`, and
  `smoke_ready=false` remain pinned.
- Native-visible verifier remains blocked on `native_actuator_visible_smoke`.

### Proof commands

```powershell
python scripts/cargo_fmt_workspace.py
cargo test --locked -p wheelctl --bin wheelctl sniff_summary_surfaces_host_to_device_decode_gaps -- --nocapture
cargo test --locked -p wheelctl --bin wheelctl vendor_protocol_evidence_review -- --nocapture
cargo run --locked -p wheelctl --bin wheelctl -- hardware sniff-summary --pcapng target/sniff/pit-house-open-idle/capture.pcapng --vendor 0x346E --product 0x0004 --json-out ci/hardware/sniff/moza-r5/2026-05-13/pit-house-open-idle/sniff-summary.json --md-out target/sniff/pit-house-open-idle/sniff-summary.md --json
cargo run --locked -p wheelctl --bin wheelctl -- hardware sniff-summary --pcapng target/sniff/pit-house-full-controls/capture.pcapng --vendor 0x346E --product 0x0004 --json-out ci/hardware/sniff/moza-r5/2026-05-13/pit-house-full-controls/sniff-summary.json --md-out target/sniff/pit-house-full-controls/sniff-summary.md --json
cargo run --locked -p wheelctl --bin wheelctl -- hardware sniff-bundle --plan ci/hardware/sniff/moza-r5/2026-05-13/pit-house-open-idle/sniff-plan.json --receipt ci/hardware/sniff/moza-r5/2026-05-13/pit-house-open-idle/sniff-receipt.json --summary ci/hardware/sniff/moza-r5/2026-05-13/pit-house-open-idle/sniff-summary.json --operator-notes target/sniff/pit-house-open-idle/operator-notes.md --out target/sniff/pit-house-open-idle/openracing-sniff-bundle.zip --json-out ci/hardware/sniff/moza-r5/2026-05-13/pit-house-open-idle/sniff-bundle-manifest.json --json
cargo run --locked -p wheelctl --bin wheelctl -- hardware sniff-bundle --plan ci/hardware/sniff/moza-r5/2026-05-13/pit-house-full-controls/sniff-plan.json --receipt ci/hardware/sniff/moza-r5/2026-05-13/pit-house-full-controls/sniff-receipt.json --summary ci/hardware/sniff/moza-r5/2026-05-13/pit-house-full-controls/sniff-summary.json --operator-notes target/sniff/pit-house-full-controls/operator-notes.md --out target/sniff/pit-house-full-controls/openracing-sniff-bundle.zip --json-out ci/hardware/sniff/moza-r5/2026-05-13/pit-house-full-controls/sniff-bundle-manifest.json --json
cargo run --locked -p wheelctl --bin wheelctl -- moza vendor-protocol-evidence-review --lane ci/hardware/moza-r5/2026-05-13 --json-out ci/hardware/moza-r5/2026-05-13/vendor-protocol-evidence-review.json --json --overwrite
cargo run --locked -p wheelctl --bin wheelctl -- moza artifact-index --lane ci/hardware/moza-r5/2026-05-13 --json-out target/moza-current/artifact-index-after-passive-sniff-payload-coverage.json --md-out ci/hardware/moza-r5/2026-05-13/index.md --json
cargo run --locked -p wheelctl --bin wheelctl -- moza verify-bundle --lane ci/hardware/moza-r5/2026-05-13 --stage native-visible-ready --json-out target/moza-current/native-visible-after-passive-sniff-payload-coverage.json --json; if ($LASTEXITCODE -eq 4) { exit 0 } else { throw "expected native-visible verifier to remain blocked" }
cargo clippy --locked -p wheelctl --bin wheelctl --all-features -- -D warnings
cargo run --locked -p openracing-tools --bin package-surface -- --check
python scripts/policy_file.py
git diff --check
```

### Rollback

Revert only the payload-export coverage fields, tests, schema additions,
refreshed sniff summaries, bundle manifests, protocol review receipt, and
source-of-truth updates. Do not remove passive sniff plans, raw local capture
artifacts, consumed hardware attempts, or prior undertravel evidence.

## Work item: passive-sniff-usbcom-payload-extraction

Status: completed
Linked proposal: docs/proposals/OR-PROP-0001-moza-native-visible-lane.md
Linked spec: docs/specs/OR-SPEC-0001-moza-native-visible-lane.md
Linked ADR: docs/adr/0009-hardware-validation-evidence-state-machine.md
Blocks: decoded vendor protocol review before any future output family
Blocked by: checked-in Pit House passive sniff summaries with host-to-device
payload export gaps

### Goal

Extract USB CDC/serial payload fields from passive Pit House captures so the
lane can move from "payload bytes missing" toward decoded vendor frame review.

### Production delta

Extend `wheelctl hardware sniff-summary` payload extraction to include
TShark's `usbcom.data.out_payload` and `usbcom.data.in_payload` fields.
Refresh the Pit House `open-idle` and `full-controls` summaries, their bundle
manifests, and `vendor-protocol-evidence-review.json`.

The checked-in review now records candidate host-to-device frame/report ID
`0x7E`, 3,246 extracted host-to-device payload packets, 53,988 extracted
host-to-device payload bytes, and two residual data-length packets without
extracted payload bytes across the two completed Pit House scenarios. This is
non-claiming protocol evidence and routes next work to decoding the `0x7E`
USBCOM frame stream before another output family.

### Non-goals

No HID open, serial open, read-only query send, hardware output, authorization
receipt, PIDFF rerun, force increase, direct HID report `0xaf`, high torque,
serial config, firmware, DFU, native-control claim, native-visible claim,
smoke-ready claim, Pit House coexistence claim, simulator claim, release-ready
claim, raw `.pcapng` commit, or decoded command claim.

### Acceptance

- `sniff-summary` extracts USB CDC/serial payloads from
  `usbcom.data.out_payload` and `usbcom.data.in_payload`.
- The checked-in Pit House summaries record host-to-device candidate
  frame/report ID `0x7E`.
- `vendor-protocol-evidence-review.json` records
  `total_host_to_device_payload_extracted_packet_count=3246`,
  `total_host_to_device_payload_extracted_bytes=53988`, and
  `total_host_to_device_payload_missing_packets=2`.
- `planned_next_output.allowed=false`, `native_control_evidence=false`,
  `hardware_output_authorized=false`, `native_visible_ready=false`, and
  `smoke_ready=false` remain pinned.
- Native-visible verifier remains blocked on `native_actuator_visible_smoke`.

### Proof commands

```powershell
python scripts/cargo_fmt_workspace.py
cargo test --locked -p wheelctl --bin wheelctl sniff_summary_extracts_usbcom_host_to_device_payloads -- --nocapture
cargo test --locked -p wheelctl --bin wheelctl sniff_summary -- --nocapture
cargo test --locked -p wheelctl --bin wheelctl vendor_protocol_evidence_review -- --nocapture
cargo run --locked -p wheelctl --bin wheelctl -- hardware sniff-summary --pcapng target/sniff/pit-house-open-idle/capture.pcapng --vendor 0x346E --product 0x0004 --json-out ci/hardware/sniff/moza-r5/2026-05-13/pit-house-open-idle/sniff-summary.json --md-out target/sniff/pit-house-open-idle/sniff-summary.md --json
cargo run --locked -p wheelctl --bin wheelctl -- hardware sniff-summary --pcapng target/sniff/pit-house-full-controls/capture.pcapng --vendor 0x346E --product 0x0004 --json-out ci/hardware/sniff/moza-r5/2026-05-13/pit-house-full-controls/sniff-summary.json --md-out target/sniff/pit-house-full-controls/sniff-summary.md --json
cargo run --locked -p wheelctl --bin wheelctl -- hardware sniff-bundle --plan ci/hardware/sniff/moza-r5/2026-05-13/pit-house-open-idle/sniff-plan.json --receipt ci/hardware/sniff/moza-r5/2026-05-13/pit-house-open-idle/sniff-receipt.json --summary ci/hardware/sniff/moza-r5/2026-05-13/pit-house-open-idle/sniff-summary.json --operator-notes target/sniff/pit-house-open-idle/operator-notes.md --out target/sniff/pit-house-open-idle/openracing-sniff-bundle.zip --json-out ci/hardware/sniff/moza-r5/2026-05-13/pit-house-open-idle/sniff-bundle-manifest.json --json
cargo run --locked -p wheelctl --bin wheelctl -- hardware sniff-bundle --plan ci/hardware/sniff/moza-r5/2026-05-13/pit-house-full-controls/sniff-plan.json --receipt ci/hardware/sniff/moza-r5/2026-05-13/pit-house-full-controls/sniff-receipt.json --summary ci/hardware/sniff/moza-r5/2026-05-13/pit-house-full-controls/sniff-summary.json --operator-notes target/sniff/pit-house-full-controls/operator-notes.md --out target/sniff/pit-house-full-controls/openracing-sniff-bundle.zip --json-out ci/hardware/sniff/moza-r5/2026-05-13/pit-house-full-controls/sniff-bundle-manifest.json --json
cargo run --locked -p wheelctl --bin wheelctl -- moza vendor-protocol-evidence-review --lane ci/hardware/moza-r5/2026-05-13 --json-out ci/hardware/moza-r5/2026-05-13/vendor-protocol-evidence-review.json --json --overwrite
cargo run --locked -p wheelctl --bin wheelctl -- moza artifact-index --lane ci/hardware/moza-r5/2026-05-13 --json-out target/moza-current/artifact-index-after-passive-sniff-usbcom-payload-extraction.json --md-out ci/hardware/moza-r5/2026-05-13/index.md --json
cargo run --locked -p wheelctl --bin wheelctl -- moza verify-bundle --lane ci/hardware/moza-r5/2026-05-13 --stage native-visible-ready --json-out target/moza-current/native-visible-after-passive-sniff-usbcom-payload-extraction.json --json; if ($LASTEXITCODE -eq 4) { exit 0 } else { throw "expected native-visible verifier to remain blocked" }
cargo clippy --locked -p wheelctl --bin wheelctl --all-features -- -D warnings
cargo run --locked -p openracing-tools --bin package-surface -- --check
python scripts/policy_file.py
git diff --check
```

### Rollback

Revert only the USBCOM payload field extraction, tests, schema addition,
refreshed sniff summaries, bundle manifests, protocol review receipt, and
source-of-truth updates. Do not remove passive sniff plans, raw local capture
artifacts, consumed hardware attempts, or prior undertravel evidence.

## Work item: passive-sniff-usbcom-frame-shape-review

Status: completed
Linked proposal: docs/proposals/OR-PROP-0001-moza-native-visible-lane.md
Linked spec: docs/specs/OR-SPEC-0001-moza-native-visible-lane.md
Linked ADR: docs/adr/0009-hardware-validation-evidence-state-machine.md
Blocks: semantic vendor protocol decode before any future output family
Blocked by: checked-in Pit House passive sniff summaries with extracted
host-to-device USB CDC payloads

### Goal

Decode the extracted Pit House USB CDC payload stream far enough to preserve
length-prefixed `0x7E` serial-frame shape, checksum validity, commandless-frame
counts, and tuple IDs as non-claiming protocol evidence.

### Production Delta

Extend `wheelctl hardware sniff-summary` with `usbcom_serial_frame_summary`
inside `report_classification_summary`. The summary parses host-to-device USB
CDC payloads as length-prefixed `0x7E` serial-frame candidates using the same
magic-13 wrapping checksum model as the fixture decoder, records tuple counts,
and pins `native_control_evidence=false` and `readiness_claim=false`.

Refresh the Pit House `open-idle` and `full-controls` summaries, bundle
manifests, and `vendor-protocol-evidence-review.json`.

The checked-in review now records 7,863 parsed host-to-device `0x7E` candidate
frames across 3,246 payload packets. All 7,863 have valid checksums, zero
checksum-invalid frames, zero truncated frames, and no frame-shape decode gap.
There are 1,467 commandless frames and 30 distinct tuple IDs. This is still not
a semantic command decode and does not make any tuple sendable.

### Non-goals

No HID open, serial open, read-only query send, hardware output, authorization
receipt, PIDFF rerun, force increase, direct HID report `0xaf`, high torque,
serial config, firmware, DFU, native-control claim, native-visible claim,
smoke-ready claim, Pit House coexistence claim, simulator claim, release-ready
claim, raw `.pcapng` commit, or semantic command/sendability claim.

### Acceptance

- `sniff-summary` records `usbcom_serial_frame_summary` for passive summaries.
- The checked-in Pit House summaries record parsed candidate frame counts,
  checksum-valid counts, tuple counts, and `frame_shape_decode_gap=false`.
- `vendor-protocol-evidence-review.json` records
  `total_host_to_device_serial_frame_count=7863`,
  `total_host_to_device_serial_frame_checksum_valid_count=7863`,
  `total_host_to_device_serial_frame_checksum_invalid_count=0`, and
  `host_to_device_serial_frame_shape_decode_gap_detected=false`.
- `planned_next_output.allowed=false`, `native_control_evidence=false`,
  `hardware_output_authorized=false`, `native_visible_ready=false`, and
  `smoke_ready=false` remain pinned.
- Native-visible verifier remains blocked on `native_actuator_visible_smoke`.

### Proof Commands

```powershell
python scripts/cargo_fmt_workspace.py
cargo test --locked -p wheelctl --bin wheelctl sniff_summary_extracts_usbcom_host_to_device_payloads -- --nocapture
cargo test --locked -p wheelctl --bin wheelctl sniff_summary -- --nocapture
cargo test --locked -p wheelctl --bin wheelctl vendor_protocol_evidence_review -- --nocapture
cargo run --locked -p wheelctl --bin wheelctl -- hardware sniff-summary --pcapng target/sniff/pit-house-open-idle/capture.pcapng --vendor 0x346E --product 0x0004 --json-out ci/hardware/sniff/moza-r5/2026-05-13/pit-house-open-idle/sniff-summary.json --md-out target/sniff/pit-house-open-idle/sniff-summary.md --json
cargo run --locked -p wheelctl --bin wheelctl -- hardware sniff-summary --pcapng target/sniff/pit-house-full-controls/capture.pcapng --vendor 0x346E --product 0x0004 --json-out ci/hardware/sniff/moza-r5/2026-05-13/pit-house-full-controls/sniff-summary.json --md-out target/sniff/pit-house-full-controls/sniff-summary.md --json
cargo run --locked -p wheelctl --bin wheelctl -- hardware sniff-bundle --plan ci/hardware/sniff/moza-r5/2026-05-13/pit-house-open-idle/sniff-plan.json --receipt ci/hardware/sniff/moza-r5/2026-05-13/pit-house-open-idle/sniff-receipt.json --summary ci/hardware/sniff/moza-r5/2026-05-13/pit-house-open-idle/sniff-summary.json --operator-notes target/sniff/pit-house-open-idle/operator-notes.md --out target/sniff/pit-house-open-idle/openracing-sniff-bundle.zip --json-out ci/hardware/sniff/moza-r5/2026-05-13/pit-house-open-idle/sniff-bundle-manifest.json --json
cargo run --locked -p wheelctl --bin wheelctl -- hardware sniff-bundle --plan ci/hardware/sniff/moza-r5/2026-05-13/pit-house-full-controls/sniff-plan.json --receipt ci/hardware/sniff/moza-r5/2026-05-13/pit-house-full-controls/sniff-receipt.json --summary ci/hardware/sniff/moza-r5/2026-05-13/pit-house-full-controls/sniff-summary.json --operator-notes target/sniff/pit-house-full-controls/operator-notes.md --out target/sniff/pit-house-full-controls/openracing-sniff-bundle.zip --json-out ci/hardware/sniff/moza-r5/2026-05-13/pit-house-full-controls/sniff-bundle-manifest.json --json
cargo run --locked -p wheelctl --bin wheelctl -- moza vendor-protocol-evidence-review --lane ci/hardware/moza-r5/2026-05-13 --json-out ci/hardware/moza-r5/2026-05-13/vendor-protocol-evidence-review.json --json --overwrite
cargo run --locked -p wheelctl --bin wheelctl -- moza artifact-index --lane ci/hardware/moza-r5/2026-05-13 --json-out target/moza-current/artifact-index-after-passive-sniff-usbcom-frame-shape-review.json --md-out ci/hardware/moza-r5/2026-05-13/index.md --json
cargo run --locked -p wheelctl --bin wheelctl -- moza verify-bundle --lane ci/hardware/moza-r5/2026-05-13 --stage native-visible-ready --json-out target/moza-current/native-visible-after-passive-sniff-usbcom-frame-shape-review.json --json; if ($LASTEXITCODE -eq 4) { exit 0 } else { throw "expected native-visible verifier to remain blocked" }
cargo clippy --locked -p wheelctl --bin wheelctl --all-features -- -D warnings
cargo run --locked -p openracing-tools --bin package-surface -- --check
python scripts/policy_file.py
git diff --check
```

### Rollback

Revert only the USBCOM frame-shape parser, tests, schema additions, refreshed
sniff summaries, bundle manifests, protocol review receipt, and source-of-truth
updates. Do not remove passive sniff plans, raw local capture artifacts,
consumed hardware attempts, or prior undertravel evidence.

## Work item: passive-sniff-tuple-registry-coverage

Status: completed
Linked proposal: docs/proposals/OR-PROP-0001-moza-native-visible-lane.md
Linked specs:
- docs/specs/OR-SPEC-0001-moza-native-visible-lane.md
- docs/specs/OR-SPEC-0002-moza-r5-vendor-authority-test-lane.md
Linked ADR: docs/adr/0009-hardware-validation-evidence-state-machine.md
Blocks: reviewed semantic command evidence before any future output family
Blocked by: checked-in Pit House passive sniff summaries with parsed `0x7E`
tuple IDs and the current semantic vendor command registry

### Goal

Compare the checked-in passive Pit House USBCOM tuple IDs against the semantic
vendor command registry so the lane distinguishes known read-only status
tuples, commandless tuples, and unknown commanded tuples before any future
output-family plan.

### Production Delta

Extend `wheelctl moza vendor-protocol-evidence-review` with
`passive_tuple_registry_coverage`. The receipt compares
`host_to_device_serial_frame_tuple_ids` from the checked-in passive sniff
summaries to `fixtures/moza/r5/vendor-command-registry.json`, preserving
registry matches and fencing all unknown or commandless tuples as
`unknown_do_not_send`.

Refresh `ci/hardware/moza-r5/2026-05-13/vendor-protocol-evidence-review.json`
and this source-of-truth stack.

The checked-in review now records 30 distinct passive tuple IDs. Exactly one
tuple matches the current registry: `0x28/0x13/0x02`
(`base_gain_get_overall_strength`), a read-only `vendor_status` tuple. The
remaining passive tuple evidence is 12 commandless tuple IDs and 17 unknown
commanded tuple IDs. There are zero known write-like tuple matches and zero
malformed tuple IDs.

### Non-goals

No HID open, serial open, read-only query send, hardware output, authorization
receipt, PIDFF rerun, force increase, direct HID report `0xaf`, high torque,
serial config, firmware, DFU, native-control claim, native-visible claim,
smoke-ready claim, Pit House coexistence claim, simulator claim, release-ready
claim, raw `.pcapng` commit, or tuple sendability claim.

### Acceptance

- `vendor-protocol-evidence-review.json` records
  `passive_tuple_registry_coverage.total_tuple_id_count=30`.
- `known_registry_tuple_count=1` and `known_read_only_status_tuple_ids`
  contains `0x28/0x13/0x02`.
- `commandless_tuple_count=12`, `unknown_commanded_tuple_count=17`,
  `known_write_like_tuple_count=0`, and `malformed_tuple_count=0`.
- `unknown_tuple_risk_class=unknown_do_not_send`.
- `protocol_evidence_sufficient_for_output_plan=false`,
  `hardware_output_authorized=false`, `native_control_evidence=false`, and
  `output_sendability_claim=false` remain pinned.
- Native-visible verifier remains blocked on `native_actuator_visible_smoke`.

### Proof Commands

```powershell
python scripts/cargo_fmt_workspace.py
cargo test --locked -p wheelctl --bin wheelctl vendor_protocol_evidence_review -- --nocapture
cargo run --locked -p wheelctl --bin wheelctl -- moza vendor-protocol-evidence-review --lane ci/hardware/moza-r5/2026-05-13 --json-out ci/hardware/moza-r5/2026-05-13/vendor-protocol-evidence-review.json --json --overwrite
cargo run --locked -p wheelctl --bin wheelctl -- moza artifact-index --lane ci/hardware/moza-r5/2026-05-13 --json-out target/moza-current/artifact-index-after-passive-tuple-registry-review.json --md-out ci/hardware/moza-r5/2026-05-13/index.md --json
cargo run --locked -p wheelctl --bin wheelctl -- moza verify-bundle --lane ci/hardware/moza-r5/2026-05-13 --stage native-visible-ready --json-out target/moza-current/native-visible-after-passive-tuple-registry-review.json --json; if ($LASTEXITCODE -eq 4) { exit 0 } else { throw "expected native-visible verifier to remain blocked" }
cargo clippy --locked -p wheelctl --bin wheelctl --all-features -- -D warnings
cargo run --locked -p openracing-tools --bin package-surface -- --check
python scripts/policy_file.py
git diff --check
```

### Rollback

Revert only the passive tuple-to-registry coverage code, schema additions,
refreshed protocol review receipt, and source-of-truth updates. Do not remove
passive sniff plans, raw local capture artifacts, consumed hardware attempts,
or prior undertravel evidence.

## Work item: passive-sniff-tuple-frequency-review

Status: completed
Linked proposal: docs/proposals/OR-PROP-0001-moza-native-visible-lane.md
Linked specs:
- docs/specs/OR-SPEC-0001-moza-native-visible-lane.md
- docs/specs/OR-SPEC-0002-moza-r5-vendor-authority-test-lane.md
Linked ADR: docs/adr/0009-hardware-validation-evidence-state-machine.md
Blocks: reviewed semantic command evidence before any future output family
Blocked by: checked-in Pit House passive sniff summaries with parsed `0x7E`
tuple counts and passive tuple registry coverage

### Goal

Preserve per-scenario passive tuple counts and total frequency rankings from
the checked-in Pit House summaries so vendor protocol decode work can focus on
the most repeated unknown commanded tuples without treating frequency as
sendability evidence.

### Production Delta

Extend `wheelctl moza vendor-protocol-evidence-review` so each reviewed passive
sniff scenario records `host_to_device_serial_frame_tuple_counts` and
`passive_tuple_registry_coverage` records `tuple_frequency_summary`,
`highest_frequency_tuple_ids`, and
`highest_frequency_unknown_commanded_tuple_ids`.

Refresh `schemas/moza-vendor-protocol-evidence-review.schema.json`,
`ci/hardware/moza-r5/2026-05-13/vendor-protocol-evidence-review.json`, the
artifact index, and this source-of-truth stack.

The checked-in review now ranks the highest-frequency unknown commanded tuples
as:

| Tuple | Total count | Payload bytes | Scenarios |
| --- | ---: | ---: | ---: |
| `0x5A/0x1B/0x00` | 1,896 | 0 | 2 |
| `0x5D/0x1B/0x01` | 1,894 | 2 | 2 |
| `0x25/0x19/0x01` | 624 | 2 | 2 |
| `0x25/0x19/0x02` | 624 | 2 | 2 |
| `0x25/0x19/0x03` | 624 | 2 | 2 |

The review keeps those tuples classified as `unknown_commanded` with
`unknown_tuple_risk_class=unknown_do_not_send`.

### Non-goals

No HID open, serial open, read-only query send, hardware output, authorization
receipt, PIDFF rerun, force increase, direct HID report `0xaf`, high torque,
serial config, firmware, DFU, native-control claim, native-visible claim,
smoke-ready claim, Pit House coexistence claim, simulator claim, release-ready
claim, raw `.pcapng` commit, semantic command decode, or tuple sendability
claim.

### Acceptance

- `vendor-protocol-evidence-review.json` records
  `host_to_device_serial_frame_tuple_counts` for each completed passive sniff
  scenario with parsed `0x7E` serial frames.
- `passive_tuple_registry_coverage.tuple_frequency_summary` ranks tuple IDs by
  descending total count and includes per-scenario counts.
- `highest_frequency_unknown_commanded_tuple_ids` starts with
  `0x5A/0x1B/0x00`, `0x5D/0x1B/0x01`, `0x25/0x19/0x01`,
  `0x25/0x19/0x02`, and `0x25/0x19/0x03`.
- Frequency-ranked unknown commanded tuples remain
  `unknown_tuple_risk_class=unknown_do_not_send`.
- `protocol_evidence_sufficient_for_output_plan=false`,
  `hardware_output_authorized=false`, `native_control_evidence=false`, and
  `output_sendability_claim=false` remain pinned.
- Native-visible verifier remains blocked on `native_actuator_visible_smoke`.

### Proof Commands

```powershell
python scripts/cargo_fmt_workspace.py
cargo test --locked -p wheelctl --bin wheelctl vendor_protocol_evidence_review -- --nocapture
cargo run --locked -p wheelctl --bin wheelctl -- moza vendor-protocol-evidence-review --lane ci/hardware/moza-r5/2026-05-13 --json-out ci/hardware/moza-r5/2026-05-13/vendor-protocol-evidence-review.json --json --overwrite
cargo run --locked -p wheelctl --bin wheelctl -- moza artifact-index --lane ci/hardware/moza-r5/2026-05-13 --json-out target/moza-current/artifact-index-after-passive-tuple-frequency-review.json --md-out ci/hardware/moza-r5/2026-05-13/index.md --json
cargo run --locked -p wheelctl --bin wheelctl -- moza verify-bundle --lane ci/hardware/moza-r5/2026-05-13 --stage native-visible-ready --json-out target/moza-current/native-visible-after-passive-tuple-frequency-review.json --json; if ($LASTEXITCODE -eq 4) { exit 0 } else { throw "expected native-visible verifier to remain blocked" }
cargo test --locked -p wheelctl --bin wheelctl checked_in_moza_lane_index_matches_artifact_index_renderer -- --nocapture
cargo clippy --locked -p wheelctl --bin wheelctl --all-features -- -D warnings
cargo run --locked -p openracing-tools --bin package-surface -- --check
python scripts/policy_file.py
git diff --check
```

### Rollback

Revert only the passive tuple-frequency code, schema additions, refreshed
protocol review receipt, artifact-index refresh, and source-of-truth updates.
Do not remove passive sniff plans, raw local capture artifacts, consumed
hardware attempts, prior undertravel evidence, or the tuple-to-registry
coverage receipt fields.

## Work item: passive-sniff-decode-priority-navigation

Status: completed
Linked proposal: docs/proposals/OR-PROP-0001-moza-native-visible-lane.md
Linked specs:
- docs/specs/OR-SPEC-0001-moza-native-visible-lane.md
- docs/specs/OR-SPEC-0002-moza-r5-vendor-authority-test-lane.md
Linked ADR: docs/adr/0009-hardware-validation-evidence-state-machine.md
Blocks: reviewed semantic command evidence before any future output family
Blocked by: checked-in `vendor-protocol-evidence-review.json` with tuple
frequency ranking

### Goal

Surface the frequency-ranked unknown commanded tuple decode priority through
normal artifact-index and bench-wizard navigation so operators and future PRs do
not need to inspect the large protocol review receipt by hand.

### Production Delta

Extend the shared vendor-authority navigation summary with
`vendor_protocol_decode_priority`, derived from
`passive_tuple_registry_coverage.tuple_frequency_summary` in the checked-in
`vendor-protocol-evidence-review.json` receipt. Render the top unknown
commanded tuples in artifact-index and bench-wizard Markdown.

Refresh `ci/hardware/moza-r5/2026-05-13/index.md` and this source-of-truth
stack.

### Non-goals

No HID open, serial open, read-only query send, hardware output, authorization
receipt, PIDFF rerun, force increase, direct HID report `0xaf`, high torque,
serial config, firmware, DFU, native-control claim, native-visible claim,
smoke-ready claim, Pit House coexistence claim, simulator claim, release-ready
claim, raw `.pcapng` commit, semantic command decode, registry promotion, or
tuple sendability claim.

### Acceptance

- Artifact-index and bench-wizard receipts include
  `vendor_authority_navigation.vendor_protocol_decode_priority`.
- The decode-priority object records `claim_scope` as
  `no_output_vendor_protocol_decode_priority_navigation`.
- The top unknown commanded tuples begin with `0x5A/0x1B/0x00` and
  `0x5D/0x1B/0x01`.
- `hardware_output_authorized=false`, `native_control_evidence=false`,
  `native_visible_ready=false`, `protocol_evidence_sufficient_for_output_plan=false`,
  and `output_sendability_claim=false` remain pinned.
- Artifact-index and bench-wizard Markdown render the decode-priority table
  without emitting a hardware attempt command.
- Native-visible verifier remains blocked on `native_actuator_visible_smoke`.

### Proof Commands

```powershell
python scripts/cargo_fmt_workspace.py
cargo test --locked -p wheelctl --bin wheelctl vendor_authority_navigation_surfaces_decode_priority_without_claims -- --nocapture
cargo test --locked -p wheelctl --bin wheelctl artifact_index -- --nocapture
cargo test --locked -p wheelctl --bin wheelctl bench_wizard -- --nocapture
cargo run --locked -p wheelctl --bin wheelctl -- moza artifact-index --lane ci/hardware/moza-r5/2026-05-13 --json-out target/moza-current/artifact-index-after-decode-priority-navigation.json --md-out ci/hardware/moza-r5/2026-05-13/index.md --json
cargo run --locked -p wheelctl --bin wheelctl -- moza bench-wizard --lane ci/hardware/moza-r5/2026-05-13 --json-out target/moza-current/bench-wizard-after-decode-priority-navigation.json --md-out target/moza-current/bench-wizard-after-decode-priority-navigation.md --json
cargo run --locked -p wheelctl --bin wheelctl -- moza verify-bundle --lane ci/hardware/moza-r5/2026-05-13 --stage native-visible-ready --json-out target/moza-current/native-visible-after-decode-priority-navigation.json --json; if ($LASTEXITCODE -eq 4) { exit 0 } else { throw "expected native-visible verifier to remain blocked" }
cargo test --locked -p wheelctl --bin wheelctl checked_in_moza_lane_index_matches_artifact_index_renderer -- --nocapture
cargo clippy --locked -p wheelctl --bin wheelctl --all-features -- -D warnings
cargo run --locked -p openracing-tools --bin package-surface -- --check
python scripts/policy_file.py
git diff --check
```

### Rollback

Revert only the decode-priority navigation code, tests, refreshed artifact
index, and source-of-truth updates. Do not remove the protocol evidence review
receipt, passive tuple frequency fields, passive sniff plans, raw local capture
artifacts, consumed hardware attempts, or prior undertravel evidence.

## Work item: passive-sniff-tuple-sample-fixtures

Status: completed
Linked proposal: docs/proposals/OR-PROP-0001-moza-native-visible-lane.md
Linked specs:
- docs/specs/OR-SPEC-0001-moza-native-visible-lane.md
- docs/specs/OR-SPEC-0002-moza-r5-vendor-authority-test-lane.md
Linked ADR: docs/adr/0009-hardware-validation-evidence-state-machine.md
Blocks: reviewed semantic command evidence before any future output family
Blocked by: checked-in Pit House summaries with parsed `0x7E` tuple IDs

### Goal

Preserve bounded passive sample frames for checked-in Pit House tuple IDs so the
next semantic decoder work can start from concrete fixture examples instead of
frequency counts alone.

### Production Delta

Extend `hardware sniff-summary` to store up to three checksum-valid
`sample_frames` per USB CDC serial tuple, including frame hex, payload hex,
packet/frame ordinals, checksum status, and pinned false output/sendability
gates. Extend `vendor-protocol-evidence-review.json` with
`host_to_device_serial_frame_tuple_sample_count` and decode-candidate sample
fixtures for the highest-frequency unknown commanded tuples.

Artifact-index and bench-wizard navigation now surface the sample fixture count
and representative first frames in `vendor_protocol_decode_priority`.

Refresh the two checked-in Pit House summaries, bundle manifests,
`vendor-protocol-evidence-review.json`, `ci/hardware/moza-r5/2026-05-13/index.md`,
and this source-of-truth stack.

### Non-goals

No HID open, serial open, read-only query send, hardware output, authorization
receipt, PIDFF rerun, force increase, direct HID report `0xaf`, high torque,
serial config, firmware, DFU, native-control claim, native-visible claim,
smoke-ready claim, Pit House coexistence claim, simulator claim, release-ready
claim, raw `.pcapng` commit, semantic command decode, registry promotion, or
tuple sendability claim.

### Acceptance

- `sniff-summary.json` records bounded `sample_frames` under each parsed
  `usbcom_serial_frame_summary.tuple_counts` item.
- `vendor-protocol-evidence-review.json` records
  `host_to_device_serial_frame_tuple_sample_count=159`.
- `passive_tuple_registry_coverage.decode_candidate_sample_count=30` and the
  sample fixture tuple IDs begin with `0x5A/0x1B/0x00` and
  `0x5D/0x1B/0x01`.
- Sample fixtures pin `checksum_valid=true`, `hardware_output_authorized=false`,
  and `output_sendability_claim=false`.
- Artifact-index and bench-wizard Markdown render sample fixture counts and
  representative frames without emitting a hardware attempt command.
- Native-visible verifier remains blocked on `native_actuator_visible_smoke`.

### Proof Commands

```powershell
python scripts/cargo_fmt_workspace.py
cargo test --locked -p wheelctl --bin wheelctl sniff_summary_extracts_usbcom_host_to_device_payloads -- --nocapture
cargo test --locked -p wheelctl --bin wheelctl sniff_summary -- --nocapture
cargo test --locked -p wheelctl --bin wheelctl vendor_protocol_evidence_review -- --nocapture
cargo run --locked -p wheelctl --bin wheelctl -- hardware sniff-summary --pcapng target/sniff/pit-house-open-idle/capture.pcapng --vendor 0x346E --product 0x0004 --json-out ci/hardware/sniff/moza-r5/2026-05-13/pit-house-open-idle/sniff-summary.json --md-out target/sniff/pit-house-open-idle/sniff-summary.md --json
cargo run --locked -p wheelctl --bin wheelctl -- hardware sniff-summary --pcapng target/sniff/pit-house-full-controls/capture.pcapng --vendor 0x346E --product 0x0004 --json-out ci/hardware/sniff/moza-r5/2026-05-13/pit-house-full-controls/sniff-summary.json --md-out target/sniff/pit-house-full-controls/sniff-summary.md --json
cargo run --locked -p wheelctl --bin wheelctl -- hardware sniff-bundle --plan ci/hardware/sniff/moza-r5/2026-05-13/pit-house-open-idle/sniff-plan.json --receipt ci/hardware/sniff/moza-r5/2026-05-13/pit-house-open-idle/sniff-receipt.json --summary ci/hardware/sniff/moza-r5/2026-05-13/pit-house-open-idle/sniff-summary.json --operator-notes target/sniff/pit-house-open-idle/operator-notes.md --out target/sniff/pit-house-open-idle/openracing-sniff-bundle.zip --json-out ci/hardware/sniff/moza-r5/2026-05-13/pit-house-open-idle/sniff-bundle-manifest.json --json
cargo run --locked -p wheelctl --bin wheelctl -- hardware sniff-bundle --plan ci/hardware/sniff/moza-r5/2026-05-13/pit-house-full-controls/sniff-plan.json --receipt ci/hardware/sniff/moza-r5/2026-05-13/pit-house-full-controls/sniff-receipt.json --summary ci/hardware/sniff/moza-r5/2026-05-13/pit-house-full-controls/sniff-summary.json --operator-notes target/sniff/pit-house-full-controls/operator-notes.md --out target/sniff/pit-house-full-controls/openracing-sniff-bundle.zip --json-out ci/hardware/sniff/moza-r5/2026-05-13/pit-house-full-controls/sniff-bundle-manifest.json --json
cargo run --locked -p wheelctl --bin wheelctl -- moza vendor-protocol-evidence-review --lane ci/hardware/moza-r5/2026-05-13 --json-out ci/hardware/moza-r5/2026-05-13/vendor-protocol-evidence-review.json --json --overwrite
cargo run --locked -p wheelctl --bin wheelctl -- moza artifact-index --lane ci/hardware/moza-r5/2026-05-13 --json-out target/moza-current/artifact-index-after-passive-tuple-samples.json --md-out ci/hardware/moza-r5/2026-05-13/index.md --json
cargo run --locked -p wheelctl --bin wheelctl -- moza verify-bundle --lane ci/hardware/moza-r5/2026-05-13 --stage native-visible-ready --json-out target/moza-current/native-visible-after-passive-tuple-samples.json --json; if ($LASTEXITCODE -eq 4) { exit 0 } else { throw "expected native-visible verifier to remain blocked" }
cargo test --locked -p wheelctl --bin wheelctl checked_in_moza_lane_index_matches_artifact_index_renderer -- --nocapture
cargo clippy --locked -p wheelctl --bin wheelctl --all-features -- -D warnings
cargo run --locked -p openracing-tools --bin package-surface -- --check
python scripts/policy_file.py
git diff --check
```

### Rollback

Revert only the tuple sample fixture code, schema additions, refreshed passive
sniff summaries/manifests, protocol review receipt, artifact-index refresh, and
source-of-truth updates. Do not remove passive sniff plans, raw local capture
artifacts, consumed hardware attempts, prior undertravel evidence, or tuple
frequency/registry coverage fields.

## Work item: passive-sniff-observed-tuple-decoder-regression

Status: completed
Linked proposal: docs/proposals/OR-PROP-0001-moza-native-visible-lane.md
Linked specs:
- docs/specs/OR-SPEC-0001-moza-native-visible-lane.md
- docs/specs/OR-SPEC-0002-moza-r5-vendor-authority-test-lane.md
Linked ADR: docs/adr/0009-hardware-validation-evidence-state-machine.md
Blocks: future semantic decode of passive vendor-control tuples
Blocked by: checked-in `vendor-protocol-evidence-review.json` decode-candidate sample frames

### Goal

Make the serial fixture decoder consume the checked-in passive tuple sample
frames as observed wire-shape fixtures without pretending unknown tuples are
semantic commands.

### Production Delta

Add an observed-frame decode helper in the Moza serial frame module. The helper
validates start byte, declared length, payload slice, checksum, tuple fields,
and optional registry lookup. Unlike `decode_fixture_frame`, it returns
`command=None` for unknown tuples instead of treating a valid passive frame
shape as a codec failure.

Add a protocol crate regression test that reads the checked-in
`vendor-protocol-evidence-review.json` decode-candidate sample fixtures,
decodes all 30 sample frames for the five highest-frequency unknown commanded
tuples, and proves they remain unknown to the semantic registry and non-sendable.

### Non-goals

No HID open, serial open, read-only query send, hardware output, authorization
receipt, PIDFF rerun, force increase, direct HID report `0xaf`, high torque,
serial config, firmware, DFU, native-control claim, native-visible claim,
smoke-ready claim, Pit House coexistence claim, simulator claim, release-ready
claim, raw `.pcapng` commit, semantic command decode, registry promotion, or
tuple sendability claim.

### Acceptance

- `decode_observed_frame_shape` validates checksum-valid passive sample frame
  shape while preserving optional registry lookup.
- The existing synthetic semantic fixture decoder still rejects unknown tuples.
- The checked-in decode-candidate sample fixtures decode to observed frames.
- The five fixture tuple IDs remain the frequency-ranked unknown commanded
  queue headed by `0x5A/0x1B/0x00` and `0x5D/0x1B/0x01`.
- All 30 passive sample frames keep `hardware_output_authorized=false` and
  `output_sendability_claim=false`.
- No readiness or sendability claim changes.

### Proof Commands

```powershell
python scripts/cargo_fmt_workspace.py
cargo test --locked -p racing-wheel-hid-moza-protocol --test vendor_passive_tuple_samples -- --nocapture
cargo test --locked -p racing-wheel-hid-moza-protocol --test vendor_serial_codec_fixtures -- --nocapture
cargo clippy --locked -p racing-wheel-hid-moza-protocol --all-features -- -D warnings
cargo run --locked -p openracing-tools --bin package-surface -- --check
python scripts/policy_file.py
git diff --check
```

### Rollback

Revert only the observed-frame decode helper, passive tuple sample regression
test, and source-of-truth updates. Do not remove checked-in passive sniff
summaries, `vendor-protocol-evidence-review.json`, sample-frame preservation,
consumed hardware attempts, prior undertravel evidence, or tuple frequency and
registry coverage fields.

## Work item: passive-sniff-tuple-sequence-regression

Status: completed
Linked proposal: docs/proposals/OR-PROP-0001-moza-native-visible-lane.md
Linked spec: docs/specs/OR-SPEC-0002-moza-r5-vendor-authority-test-lane.md
Linked ADR: docs/adr/0009-hardware-validation-evidence-state-machine.md
Blocks: future semantic decode of repeated passive vendor-control tuple groups
Blocked by: checked-in `vendor-protocol-evidence-review.json` decode-candidate sample frames

### Goal

Preserve repeated packet-order hints in the checked-in passive tuple samples so
future decode work can reason about observed tuple groups instead of only
frequency counts.

### Production Delta

Add a protocol crate regression test that reads the checked-in
`vendor-protocol-evidence-review.json` decode-candidate sample fixtures and
checks two recurring order patterns:

- `0x5A/0x1B/0x00` is followed in the same packet by `0x5D/0x1B/0x01`.
- `0x25/0x19/0x02`, `0x25/0x19/0x03`, and `0x25/0x19/0x01` repeat as an
  ordered same-packet triad.

Each sampled frame is still decoded only as an observed wire-shape fixture and
still rejected by the semantic fixture decoder as an unknown command.

### Non-goals

No HID open, serial open, read-only query send, hardware output, authorization
receipt, PIDFF rerun, force increase, direct HID report `0xaf`, high torque,
serial config, firmware, DFU, native-control claim, native-visible claim,
smoke-ready claim, Pit House coexistence claim, simulator claim, release-ready
claim, raw `.pcapng` commit, semantic command decode, registry promotion, or
tuple sendability claim.

### Acceptance

- The checked-in `0x5A/0x1B/0x00` samples are paired with
  `0x5D/0x1B/0x01` samples in the same scenario and packet with adjacent frame
  ordinals.
- The checked-in `0x25/0x19/0x02` samples are followed by
  `0x25/0x19/0x03` and `0x25/0x19/0x01` in the same scenario and packet with
  adjacent frame ordinals.
- Every sample used by the sequence regression still keeps
  `hardware_output_authorized=false` and `output_sendability_claim=false`.
- The observed-frame decoder accepts the sample frame shape, while the semantic
  fixture decoder still rejects the tuples as unknown.
- No readiness or sendability claim changes.

### Proof Commands

```powershell
python scripts/cargo_fmt_workspace.py
cargo test --locked -p racing-wheel-hid-moza-protocol --test vendor_passive_tuple_samples -- --nocapture
cargo test --locked -p racing-wheel-hid-moza-protocol --test vendor_serial_codec_fixtures -- --nocapture
cargo clippy --locked -p racing-wheel-hid-moza-protocol --all-features -- -D warnings
cargo run --locked -p openracing-tools --bin package-surface -- --check
python scripts/policy_file.py
git diff --check
```

### Rollback

Revert only the passive tuple sequence regression and source-of-truth updates.
Do not remove checked-in passive sniff summaries,
`vendor-protocol-evidence-review.json`, sample-frame preservation, observed
frame-shape decoding, consumed hardware attempts, prior undertravel evidence, or
tuple frequency and registry coverage fields.

## Work item: passive-sniff-tuple-payload-shape-review

Status: completed
Linked proposal: docs/proposals/OR-PROP-0001-moza-native-visible-lane.md
Linked spec: docs/specs/OR-SPEC-0002-moza-r5-vendor-authority-test-lane.md
Linked ADR: docs/adr/0009-hardware-validation-evidence-state-machine.md
Blocks: future semantic decode of high-frequency passive vendor-control tuples
Blocked by: checked-in `vendor-protocol-evidence-review.json` decode-candidate sample frames

### Goal

Preserve payload-shape morphology for the highest-frequency unknown commanded
passive tuple samples so future semantic decode work can distinguish empty,
zero-filled, and non-zero observed payload families without treating the tuples
as sendable.

### Production Delta

Extend `wheelctl moza vendor-protocol-evidence-review` with
`decode_candidate_payload_shape_summary`, derived from the existing
decode-candidate sample frames. Surface the same summary through
artifact-index and bench-wizard decode-priority navigation, refresh
`vendor-protocol-evidence-review.json` and the checked-in lane index, and add a
protocol crate regression that pins the current sample payload shapes.

### Non-goals

No HID open, serial open, read-only query send, hardware output, authorization
receipt, PIDFF rerun, force increase, direct HID report `0xaf`, high torque,
serial config, firmware, DFU, native-control claim, native-visible claim,
smoke-ready claim, Pit House coexistence claim, simulator claim, release-ready
claim, raw `.pcapng` commit, semantic command decode, registry promotion, or
tuple sendability claim.

### Acceptance

- `vendor-protocol-evidence-review.json` records
  `decode_candidate_payload_shape_summary.claim_scope` as
  `no_output_passive_tuple_payload_shape_review`.
- The payload-shape summary reports five tuple shapes and 30 samples for the
  highest-frequency unknown commanded tuples.
- All sampled tuple payloads are recorded as empty or zero-filled, with
  `0x5A/0x1B/0x00` empty and `0x5D/0x1B/0x01` plus the `0x25/0x19/0x01..03`
  triad zero-filled as `0000`.
- The summary pins `hardware_output_authorized=false`,
  `native_control_evidence=false`,
  `output_sendability_claim=false`, and
  `protocol_evidence_sufficient_for_output_plan=false`.
- Artifact-index and bench-wizard Markdown render payload-shape navigation
  without emitting a hardware attempt command.
- Native-visible verifier remains blocked on `native_actuator_visible_smoke`.

### Proof Commands

```powershell
python scripts/cargo_fmt_workspace.py
cargo test --locked -p wheelctl --bin wheelctl vendor_protocol_evidence_review -- --nocapture
cargo test --locked -p wheelctl --bin wheelctl vendor_authority_navigation_surfaces_decode_priority_without_claims -- --nocapture
cargo test --locked -p racing-wheel-hid-moza-protocol --test vendor_passive_tuple_samples -- --nocapture
cargo run --locked -p wheelctl --bin wheelctl -- moza vendor-protocol-evidence-review --lane ci/hardware/moza-r5/2026-05-13 --json-out ci/hardware/moza-r5/2026-05-13/vendor-protocol-evidence-review.json --json --overwrite
cargo run --locked -p wheelctl --bin wheelctl -- moza artifact-index --lane ci/hardware/moza-r5/2026-05-13 --json-out target/moza-current/artifact-index-after-passive-payload-shapes.json --md-out ci/hardware/moza-r5/2026-05-13/index.md --json
cargo run --locked -p wheelctl --bin wheelctl -- moza verify-bundle --lane ci/hardware/moza-r5/2026-05-13 --stage native-visible-ready --json-out target/moza-current/native-visible-after-passive-payload-shapes.json --json; if ($LASTEXITCODE -eq 4) { exit 0 } else { throw "expected native-visible verifier to remain blocked" }
cargo clippy --locked -p wheelctl --bin wheelctl --all-features -- -D warnings
cargo clippy --locked -p racing-wheel-hid-moza-protocol --all-features -- -D warnings
cargo run --locked -p openracing-tools --bin package-surface -- --check
python scripts/policy_file.py
git diff --check
```

### Rollback

Revert only the payload-shape summary generation, schema additions, protocol
regression, refreshed protocol review receipt, refreshed artifact index, and
source-of-truth updates. Do not remove checked-in passive sniff summaries,
sample-frame preservation, observed frame-shape decoding, packet-order
regression coverage, consumed hardware attempts, prior undertravel evidence, or
tuple frequency and registry coverage fields.

## Work item: passive-sniff-tuple-packet-group-review

Status: completed
Linked proposal: docs/proposals/OR-PROP-0001-moza-native-visible-lane.md
Linked spec: docs/specs/OR-SPEC-0002-moza-r5-vendor-authority-test-lane.md
Linked ADR: docs/adr/0009-hardware-validation-evidence-state-machine.md
Blocks: future semantic decode of repeated passive vendor-control tuple groups
Blocked by: checked-in `vendor-protocol-evidence-review.json` decode-candidate sample frames

### Goal

Preserve packet-local grouping and repeated contiguous motif evidence for the
highest-frequency unknown commanded passive tuple samples so future semantic
decode work can reason about observed command clusters without treating any
tuple as sendable.

### Production Delta

Extend `wheelctl moza vendor-protocol-evidence-review` with
`decode_candidate_packet_group_summary`, derived from the existing
decode-candidate sample frame `scenario`, `packet_ordinal`, and
`frame_ordinal_in_packet` fields. Surface the same summary through
artifact-index and bench-wizard decode-priority navigation, refresh
`vendor-protocol-evidence-review.json` and the checked-in lane index, and add a
protocol crate regression that pins the current packet-group patterns and
repeated motifs.

### Non-goals

No HID open, serial open, read-only query send, hardware output, authorization
receipt, PIDFF rerun, force increase, direct HID report `0xaf`, high torque,
serial config, firmware, DFU, native-control claim, native-visible claim,
smoke-ready claim, Pit House coexistence claim, simulator claim, release-ready
claim, raw `.pcapng` commit, semantic command decode, registry promotion, or
tuple sendability claim.

### Acceptance

- `vendor-protocol-evidence-review.json` records
  `decode_candidate_packet_group_summary.claim_scope` as
  `no_output_passive_tuple_packet_group_review`.
- The packet-group summary reports 11 packet groups and 30 samples for the
  highest-frequency unknown commanded tuples.
- The summary reports three full-packet patterns and four repeated contiguous
  motifs.
- The repeated `0x5A/0x1B/0x00` -> `0x5D/0x1B/0x01` motif is observed six
  times across the two checked-in Pit House scenarios.
- The repeated `0x25/0x19/0x02` -> `0x25/0x19/0x03` ->
  `0x25/0x19/0x01` motif is observed six times across the two checked-in Pit
  House scenarios.
- The summary pins `hardware_output_authorized=false`,
  `native_control_evidence=false`, `output_sendability_claim=false`, and
  `protocol_evidence_sufficient_for_output_plan=false`.
- Artifact-index and bench-wizard Markdown render packet-group navigation
  without emitting a hardware attempt command.
- Native-visible verifier remains blocked on `native_actuator_visible_smoke`.

### Proof Commands

```powershell
python scripts/cargo_fmt_workspace.py
cargo test --locked -p wheelctl --bin wheelctl vendor_protocol_evidence_review -- --nocapture
cargo test --locked -p wheelctl --bin wheelctl vendor_authority_navigation_surfaces_decode_priority_without_claims -- --nocapture
cargo test --locked -p racing-wheel-hid-moza-protocol --test vendor_passive_tuple_samples -- --nocapture
cargo run --locked -p wheelctl --bin wheelctl -- moza vendor-protocol-evidence-review --lane ci/hardware/moza-r5/2026-05-13 --json-out ci/hardware/moza-r5/2026-05-13/vendor-protocol-evidence-review.json --json --overwrite
cargo run --locked -p wheelctl --bin wheelctl -- moza artifact-index --lane ci/hardware/moza-r5/2026-05-13 --json-out target/moza-current/artifact-index-after-passive-packet-groups.json --md-out ci/hardware/moza-r5/2026-05-13/index.md --json
cargo run --locked -p wheelctl --bin wheelctl -- moza bench-wizard --lane ci/hardware/moza-r5/2026-05-13 --json-out target/moza-current/bench-wizard-after-passive-packet-groups.json --md-out target/moza-current/bench-wizard-after-passive-packet-groups.md --json
cargo run --locked -p wheelctl --bin wheelctl -- moza verify-bundle --lane ci/hardware/moza-r5/2026-05-13 --stage native-visible-ready --json-out target/moza-current/native-visible-after-passive-packet-groups.json --json; if ($LASTEXITCODE -eq 4) { exit 0 } else { throw "expected native-visible verifier to remain blocked" }
cargo clippy --locked -p wheelctl --bin wheelctl --all-features -- -D warnings
cargo clippy --locked -p racing-wheel-hid-moza-protocol --all-features -- -D warnings
cargo run --locked -p openracing-tools --bin package-surface -- --check
python scripts/policy_file.py
git diff --check
```

### Rollback

Revert only the packet-group summary generation, schema additions, protocol
regression, refreshed protocol review receipt, refreshed artifact index, and
source-of-truth updates. Do not remove checked-in passive sniff summaries,
sample-frame preservation, observed frame-shape decoding, packet-order
regression coverage, payload-shape summary, consumed hardware attempts, prior
undertravel evidence, or tuple frequency and registry coverage fields.

## Work item: passive-sniff-payload-gap-examples

Status: completed
Linked proposal: docs/proposals/OR-PROP-0001-moza-native-visible-lane.md
Linked spec: docs/specs/OR-SPEC-0002-moza-r5-vendor-authority-test-lane.md
Linked ADR: docs/adr/0009-hardware-validation-evidence-state-machine.md
Blocks: semantic vendor protocol decode before any future output family
Blocked by: checked-in Pit House summaries with two residual host-to-device
payload export gaps

### Goal

Turn the remaining passive payload export gap from aggregate counts into
bounded packet/frame locators so the next protocol review can inspect the two
missing-payload USB transfers without committing raw `.pcapng` files or
pretending the gap is sendable protocol evidence.

### Production Delta

Extend `wheelctl hardware sniff-summary` so host-to-device packets with
nonzero `usb.data_len` but no extracted payload bytes keep capped
`host_to_device_payload_missing_packet_examples`. Each example records the
packet ordinal, optional tshark frame number, device/interface/endpoint
locator, declared data length, and pinned false native-control/output flags.

Extend `wheelctl moza vendor-protocol-evidence-review` with
`sniff_evidence.payload_export_gap_summary`, copy the examples into the
per-scenario review, and surface the same summary through artifact-index and
bench-wizard decode-priority navigation.

Refresh the two checked-in Pit House summaries, their bundle manifests,
`vendor-protocol-evidence-review.json`, `ci/hardware/moza-r5/2026-05-13/index.md`,
and this source-of-truth stack. The current review now records two residual
missing-payload packets, one in `pit-house-open-idle` and one in
`pit-house-full-controls`, each as locator evidence only.

### Non-goals

No HID open, serial open, read-only query send, hardware output, authorization
receipt, PIDFF rerun, force increase, direct HID report `0xaf`, high torque,
serial config, firmware, DFU, native-control claim, native-visible claim,
smoke-ready claim, Pit House coexistence claim, simulator claim, release-ready
claim, raw `.pcapng` commit, semantic command decode, registry promotion, or
tuple sendability claim.

### Acceptance

- `sniff-summary.json` records
  `host_to_device_payload_missing_packet_examples` under
  `report_classification_summary`.
- `vendor-protocol-evidence-review.json` records
  `sniff_evidence.payload_export_gap_summary.claim_scope` as
  `no_output_passive_payload_export_gap_review`.
- The payload gap summary records `total_missing_packet_count=2` and
  `scenario_count=2`.
- Each missing-payload example pins `payload_extracted=false`,
  `native_control_evidence=false`, `hardware_output_authorized=false`, and
  `output_sendability_claim=false`.
- Artifact-index and bench-wizard Markdown render residual payload export gap
  navigation without emitting a hardware attempt command.
- Native-visible verifier remains blocked on `native_actuator_visible_smoke`.

### Proof Commands

```powershell
python scripts/cargo_fmt_workspace.py
cargo test --locked -p wheelctl --bin wheelctl hardware_sniff_summary -- --nocapture
cargo test --locked -p wheelctl --bin wheelctl vendor_protocol_evidence_review -- --nocapture
cargo test --locked -p wheelctl --bin wheelctl vendor_authority_navigation_surfaces_decode_priority_without_claims -- --nocapture
cargo run --locked -p wheelctl --bin wheelctl -- hardware sniff-summary --pcapng target/sniff/pit-house-open-idle/capture.pcapng --vendor 0x346E --product 0x0004 --include-payload-samples --max-samples-per-report 2 --json-out ci/hardware/sniff/moza-r5/2026-05-13/pit-house-open-idle/sniff-summary.json --md-out target/sniff/pit-house-open-idle/sniff-summary.md
cargo run --locked -p wheelctl --bin wheelctl -- hardware sniff-summary --pcapng target/sniff/pit-house-full-controls/capture.pcapng --vendor 0x346E --product 0x0004 --include-payload-samples --max-samples-per-report 2 --json-out ci/hardware/sniff/moza-r5/2026-05-13/pit-house-full-controls/sniff-summary.json --md-out target/sniff/pit-house-full-controls/sniff-summary.md
cargo run --locked -p wheelctl --bin wheelctl -- --json hardware sniff-bundle --plan ci/hardware/sniff/moza-r5/2026-05-13/pit-house-open-idle/sniff-plan.json --receipt ci/hardware/sniff/moza-r5/2026-05-13/pit-house-open-idle/sniff-receipt.json --summary ci/hardware/sniff/moza-r5/2026-05-13/pit-house-open-idle/sniff-summary.json --operator-notes target/sniff/pit-house-open-idle/operator-notes.md --include-pcapng target/sniff/pit-house-open-idle/capture.pcapng --out target/sniff/pit-house-open-idle/openracing-sniff-bundle.zip --json-out ci/hardware/sniff/moza-r5/2026-05-13/pit-house-open-idle/sniff-bundle-manifest.json
cargo run --locked -p wheelctl --bin wheelctl -- --json hardware sniff-bundle --plan ci/hardware/sniff/moza-r5/2026-05-13/pit-house-full-controls/sniff-plan.json --receipt ci/hardware/sniff/moza-r5/2026-05-13/pit-house-full-controls/sniff-receipt.json --summary ci/hardware/sniff/moza-r5/2026-05-13/pit-house-full-controls/sniff-summary.json --operator-notes target/sniff/pit-house-full-controls/operator-notes.md --include-pcapng target/sniff/pit-house-full-controls/capture.pcapng --out target/sniff/pit-house-full-controls/openracing-sniff-bundle.zip --json-out ci/hardware/sniff/moza-r5/2026-05-13/pit-house-full-controls/sniff-bundle-manifest.json
cargo run --locked -p wheelctl --bin wheelctl -- moza vendor-protocol-evidence-review --lane ci/hardware/moza-r5/2026-05-13 --json-out ci/hardware/moza-r5/2026-05-13/vendor-protocol-evidence-review.json --json --overwrite
cargo run --locked -p wheelctl --bin wheelctl -- moza artifact-index --lane ci/hardware/moza-r5/2026-05-13 --json-out target/moza-current/artifact-index-after-passive-payload-gap.json --md-out ci/hardware/moza-r5/2026-05-13/index.md --json
cargo run --locked -p wheelctl --bin wheelctl -- moza bench-wizard --lane ci/hardware/moza-r5/2026-05-13 --json-out target/moza-current/bench-wizard-after-passive-payload-gap.json --md-out target/moza-current/bench-wizard-after-passive-payload-gap.md --json
cargo run --locked -p wheelctl --bin wheelctl -- moza verify-bundle --lane ci/hardware/moza-r5/2026-05-13 --stage native-visible-ready --json-out target/moza-current/native-visible-after-passive-payload-gap.json --json; if ($LASTEXITCODE -eq 4) { exit 0 } else { throw "expected native-visible verifier to remain blocked" }
cargo clippy --locked -p wheelctl --bin wheelctl --all-features -- -D warnings
cargo run --locked -p openracing-tools --bin package-surface -- --check
python scripts/policy_file.py
git diff --check
```

### Rollback

Revert only the payload-gap example fields, schema additions, refreshed passive
sniff summaries/manifests, protocol review receipt, artifact-index refresh, and
source-of-truth updates. Do not remove passive sniff plans, raw local capture
artifacts, consumed hardware attempts, prior undertravel evidence, or existing
tuple frequency, sample fixture, payload-shape, and packet-group evidence.

## Work item: passive-sniff-semantic-hypothesis-review

Status: completed
Linked proposal: docs/proposals/OR-PROP-0001-moza-native-visible-lane.md
Linked spec: docs/specs/OR-SPEC-0002-moza-r5-vendor-authority-test-lane.md
Linked ADR: docs/adr/0009-hardware-validation-evidence-state-machine.md
Blocks: future semantic vendor protocol decode before any future output family
Blocked by: checked-in `vendor-protocol-evidence-review.json` top unknown
commanded tuple morphology

### Goal

Preserve low-confidence semantic hypotheses for the highest-frequency unknown
commanded Pit House tuple samples so future protocol work has explicit decode
questions without treating any tuple as decoded, sendable, or registry-ready.

### Production Delta

Extend `wheelctl moza vendor-protocol-evidence-review` with
`decode_candidate_semantic_hypothesis_summary`, derived from the existing
frequency, payload-shape, and packet-group review fields. Surface the same
summary through artifact-index and bench-wizard decode-priority navigation,
refresh `vendor-protocol-evidence-review.json` and the checked-in lane index,
and add protocol crate regression coverage that pins the current hypotheses as
non-sendable pattern-only evidence.

The current review classifies the repeated `0x5A/0x1B/*` and `0x5D/0x1B/*`
pair as `session_or_status_keepalive_candidate`, and the repeated
`0x25/0x19/*` triad as `base_status_or_mode_poll_candidate`. Each hypothesis
has `confidence=low_pattern_only` and requires external correlation plus
fixture-backed semantic decoder coverage before any registry promotion.

### Non-goals

No HID open, serial open, read-only query send, hardware output, authorization
receipt, PIDFF rerun, force increase, direct HID report `0xaf`, high torque,
serial config, firmware, DFU, native-control claim, native-visible claim,
smoke-ready claim, Pit House coexistence claim, simulator claim, release-ready
claim, raw `.pcapng` commit, semantic command decode, registry promotion, or
tuple sendability claim.

### Acceptance

- `vendor-protocol-evidence-review.json` records
  `decode_candidate_semantic_hypothesis_summary.claim_scope` as
  `no_output_passive_tuple_semantic_hypothesis_review`.
- The semantic hypothesis summary reports five hypotheses for the
  highest-frequency unknown commanded tuples.
- `0x5A/0x1B/0x00` and `0x5D/0x1B/0x01` are recorded as
  `session_or_status_keepalive_candidate`.
- `0x25/0x19/0x01`, `0x25/0x19/0x02`, and `0x25/0x19/0x03` are recorded as
  `base_status_or_mode_poll_candidate`.
- Every hypothesis keeps `confidence=low_pattern_only`,
  `semantic_decode_claim=false`, `registry_promotion_claim=false`,
  `hardware_output_authorized=false`, and `output_sendability_claim=false`.
- Artifact-index and bench-wizard Markdown render the semantic hypothesis
  navigation without emitting a hardware attempt command.
- Native-visible verifier remains blocked on `native_actuator_visible_smoke`.

### Proof Commands

```powershell
python scripts/cargo_fmt_workspace.py
cargo test --locked -p wheelctl --bin wheelctl vendor_protocol_evidence_review -- --nocapture
cargo test --locked -p wheelctl --bin wheelctl vendor_authority_navigation_surfaces_decode_priority_without_claims -- --nocapture
cargo test --locked -p racing-wheel-hid-moza-protocol --test vendor_passive_tuple_samples -- --nocapture
cargo run --locked -p wheelctl --bin wheelctl -- moza vendor-protocol-evidence-review --lane ci/hardware/moza-r5/2026-05-13 --json-out ci/hardware/moza-r5/2026-05-13/vendor-protocol-evidence-review.json --json --overwrite
cargo run --locked -p wheelctl --bin wheelctl -- moza artifact-index --lane ci/hardware/moza-r5/2026-05-13 --json-out target/moza-current/artifact-index-after-passive-semantic-hypotheses.json --md-out ci/hardware/moza-r5/2026-05-13/index.md --json
cargo run --locked -p wheelctl --bin wheelctl -- moza bench-wizard --lane ci/hardware/moza-r5/2026-05-13 --json-out target/moza-current/bench-wizard-after-passive-semantic-hypotheses.json --md-out target/moza-current/bench-wizard-after-passive-semantic-hypotheses.md --json
cargo run --locked -p wheelctl --bin wheelctl -- moza verify-bundle --lane ci/hardware/moza-r5/2026-05-13 --stage native-visible-ready --json-out target/moza-current/native-visible-after-passive-semantic-hypotheses.json --json; if ($LASTEXITCODE -eq 4) { exit 0 } else { throw "expected native-visible verifier to remain blocked" }
cargo test --locked -p wheelctl --bin wheelctl checked_in_moza_lane_index_matches_artifact_index_renderer -- --nocapture
cargo clippy --locked -p wheelctl --bin wheelctl --all-features -- -D warnings
cargo clippy --locked -p racing-wheel-hid-moza-protocol --all-features -- -D warnings
cargo run --locked -p openracing-tools --bin package-surface -- --check
python scripts/policy_file.py
git diff --check
```

### Rollback

Revert only the semantic hypothesis summary generation, schema additions,
protocol regression, refreshed protocol review receipt, refreshed artifact
index, and source-of-truth updates. Do not remove checked-in passive sniff
summaries, sample-frame preservation, observed frame-shape decoding,
packet-order regression coverage, payload-shape summary, packet-group summary,
payload-gap locators, consumed hardware attempts, prior undertravel evidence,
or tuple frequency and registry coverage fields.

## Work item: passive-sniff-semantic-correlation-plan

Status: completed
Linked proposal: docs/proposals/OR-PROP-0001-moza-native-visible-lane.md
Linked spec: docs/specs/OR-SPEC-0002-moza-r5-vendor-authority-test-lane.md
Linked ADR: docs/adr/0009-hardware-validation-evidence-state-machine.md
Blocks: future semantic vendor protocol decode before any future output family
Blocked by: checked-in `vendor-protocol-evidence-review.json` semantic
hypotheses and passive sniff scenario state

### Goal

Turn the low-confidence tuple hypotheses into a concrete passive correlation
plan so the next protocol evidence work can test named state-transition
questions without treating any tuple as decoded, sendable, or registry-ready.

### Production Delta

Extend `wheelctl moza vendor-protocol-evidence-review` with
`decode_candidate_semantic_correlation_plan`, derived from the existing
semantic-hypothesis summary plus completed/missing passive sniff scenarios.
Surface the same plan through artifact-index and bench-wizard decode-priority
navigation, refresh `vendor-protocol-evidence-review.json` and the checked-in
lane index, and add protocol crate regression coverage that pins the
correlation targets as non-sendable capture questions.

The current plan groups the `0x5A/0x1B/*` and `0x5D/0x1B/*` pair under
`session_or_status_keepalive_candidate`, groups the `0x25/0x19/*` triad under
`base_status_or_mode_poll_candidate`, records that both groups are present in
the two completed Pit House scenarios, and names `pit-house-setting-change` as
the next capture priority before the remaining SimHub and simulator scenarios.

### Non-goals

No HID open, serial open, read-only query send, hardware output, authorization
receipt, PIDFF rerun, force increase, direct HID report `0xaf`, high torque,
serial config, firmware, DFU, native-control claim, native-visible claim,
smoke-ready claim, Pit House coexistence claim, simulator claim, release-ready
claim, raw `.pcapng` commit, semantic command decode, registry promotion, or
tuple sendability claim.

### Acceptance

- `vendor-protocol-evidence-review.json` records
  `decode_candidate_semantic_correlation_plan.claim_scope` as
  `no_output_passive_tuple_semantic_correlation_plan`.
- The correlation plan reports two targets and five source hypotheses.
- The keepalive target lists `0x5A/0x1B/0x00` and `0x5D/0x1B/0x01`.
- The status/mode-poll target lists `0x25/0x19/0x01`,
  `0x25/0x19/0x02`, and `0x25/0x19/0x03`.
- Both targets record completed observations in `pit-house-open-idle` and
  `pit-house-full-controls`.
- Both targets list the missing correlation scenarios and name
  `pit-house-setting-change` as `next_capture_priority`.
- The plan pins `semantic_decode_claim=false`,
  `registry_promotion_claim=false`, `hardware_output_authorized=false`,
  `native_control_evidence=false`, `output_sendability_claim=false`, and
  `protocol_evidence_sufficient_for_output_plan=false`.
- Artifact-index and bench-wizard Markdown render the semantic correlation
  plan without emitting a hardware attempt command.
- Native-visible verifier remains blocked on `native_actuator_visible_smoke`.

### Proof Commands

```powershell
python scripts/cargo_fmt_workspace.py
cargo test --locked -p wheelctl --bin wheelctl vendor_protocol_evidence_review -- --nocapture
cargo test --locked -p wheelctl --bin wheelctl vendor_authority_navigation_surfaces_decode_priority_without_claims -- --nocapture
cargo test --locked -p racing-wheel-hid-moza-protocol --test vendor_passive_tuple_samples -- --nocapture
cargo run --locked -p wheelctl --bin wheelctl -- moza vendor-protocol-evidence-review --lane ci/hardware/moza-r5/2026-05-13 --json-out ci/hardware/moza-r5/2026-05-13/vendor-protocol-evidence-review.json --json --overwrite
cargo run --locked -p wheelctl --bin wheelctl -- moza artifact-index --lane ci/hardware/moza-r5/2026-05-13 --json-out target/moza-current/artifact-index-after-passive-semantic-correlation.json --md-out ci/hardware/moza-r5/2026-05-13/index.md --json
cargo run --locked -p wheelctl --bin wheelctl -- moza bench-wizard --lane ci/hardware/moza-r5/2026-05-13 --json-out target/moza-current/bench-wizard-after-passive-semantic-correlation.json --md-out target/moza-current/bench-wizard-after-passive-semantic-correlation.md --json
cargo run --locked -p wheelctl --bin wheelctl -- moza verify-bundle --lane ci/hardware/moza-r5/2026-05-13 --stage native-visible-ready --json-out target/moza-current/native-visible-after-passive-semantic-correlation.json --json; if ($LASTEXITCODE -eq 4) { exit 0 } else { throw "expected native-visible verifier to remain blocked" }
cargo test --locked -p wheelctl --bin wheelctl checked_in_moza_lane_index_matches_artifact_index_renderer -- --nocapture
cargo clippy --locked -p wheelctl --bin wheelctl --all-features -- -D warnings
cargo clippy --locked -p racing-wheel-hid-moza-protocol --all-features -- -D warnings
cargo run --locked -p openracing-tools --bin package-surface -- --check
python scripts/policy_file.py
git diff --check
```

### Rollback

Revert only the semantic correlation plan generation, schema additions,
protocol regression, refreshed protocol review receipt, refreshed artifact
index, and source-of-truth updates. Do not remove checked-in passive sniff
summaries, sample-frame preservation, observed frame-shape decoding,
packet-order regression coverage, payload-shape summary, packet-group summary,
semantic-hypothesis summary, payload-gap locators, consumed hardware attempts,
prior undertravel evidence, or tuple frequency and registry coverage fields.

## Work item: passive-sniff-setting-change-notes-requirements

Status: completed
Linked proposal: docs/proposals/OR-PROP-0001-moza-native-visible-lane.md
Linked spec: docs/specs/OR-SPEC-0002-moza-r5-vendor-authority-test-lane.md
Linked ADR: docs/adr/0009-hardware-validation-evidence-state-machine.md
Blocks: next passive correlation capture for Pit House setting changes
Blocked by: `pit-house-setting-change` sniff plan exists but lacked the scenario-specific setting/value/restore evidence fields required by `docs/hardware/sniffing-scenarios.md`

### Goal

Make the `pit-house-setting-change` passive capture handoff require the exact
operator evidence needed to correlate tuple hypotheses against one intentional
Pit House setting transition.

### Production Delta

Extend `wheelctl hardware sniff-plan` so `pit-house-setting-change` plans add
operator notes for the exact Pit House setting changed, starting setting value,
ending setting value, and whether the setting value was restored. Extend the
sniff-plan schema with the same scenario-specific requirement, add regression
coverage for the generated plan and `sniff-notes-template`, and refresh the
checked-in `pit-house-setting-change/sniff-plan.json`.

### Non-goals

No HID open, serial open, read-only query send, hardware output, authorization
receipt, PIDFF rerun, force increase, direct HID report `0xaf`, high torque,
serial config, firmware, DFU, native-control claim, native-visible claim,
smoke-ready claim, Pit House coexistence claim, simulator claim,
release-ready claim, raw `.pcapng` commit, semantic command decode, registry
promotion, or tuple sendability claim.

### Acceptance

- `pit-house-setting-change/sniff-plan.json` includes required notes for exact
  Pit House setting, starting value, ending value, and restore status.
- `hardware sniff-notes-template` renders those fields as operator checklist
  lines for the setting-change plan.
- `sniff-plan.schema.json` rejects setting-change plans that omit those fields.
- The plan remains passive and non-claiming:
  `native_control_evidence=false`, `openracing_hardware_output=false`,
  `satisfies_native_visible_ready=false`, and `satisfies_smoke_ready=false`.
- No receipt, summary, bundle manifest, raw capture, hardware output, semantic
  decode, registry promotion, or readiness claim is created.

### Proof Commands

```powershell
python scripts/cargo_fmt_workspace.py
cargo test --locked -p wheelctl --bin wheelctl hardware_sniff -- --nocapture
cargo run --locked -p wheelctl --bin wheelctl -- --json hardware sniff-plan --family moza-r5 --scenario pit-house-setting-change --lane ci/hardware/moza-r5/2026-05-13 --operator Steven --device-note "Moza R5 PID 0x0004 with KS/ES wheels, SR-P pedals, and HBP handbrake attached through the R5 hub" --capture-tool usbpcap --capture-tool wireshark --capture-tool tshark --platform-hint windows --json-out ci/hardware/sniff/moza-r5/2026-05-13/pit-house-setting-change/sniff-plan.json
cargo run --locked -p wheelctl --bin wheelctl -- --json hardware sniff-notes-template --plan ci/hardware/sniff/moza-r5/2026-05-13/pit-house-setting-change/sniff-plan.json --out target/sniff/pit-house-setting-change/operator-notes.md --json
cargo clippy --locked -p wheelctl --bin wheelctl --all-features -- -D warnings
cargo run --locked -p openracing-tools --bin package-surface -- --check
python scripts/policy_file.py
git diff --check
```

### Rollback

Revert only the setting-change-specific operator note generation, schema
condition, test coverage, refreshed `pit-house-setting-change/sniff-plan.json`,
and source-of-truth updates. Do not remove existing passive sniff summaries,
semantic correlation planning, consumed hardware attempts, or protocol evidence
review receipts.

## Work item: bench-wizard-passive-correlation-capture-handoff

Status: completed
Linked proposal: docs/proposals/OR-PROP-0001-moza-native-visible-lane.md
Linked spec: docs/specs/OR-SPEC-0002-moza-r5-vendor-authority-test-lane.md
Linked ADR: docs/adr/0009-hardware-validation-evidence-state-machine.md
Blocks: next passive correlation capture after vendor protocol review
Blocked by: `vendor-protocol-evidence-review.json` is recorded, but the bench
wizard next operator step did not directly surface the next missing passive
semantic-correlation capture handoff.

### Goal

When the vendor protocol evidence review is recorded and the semantic
correlation plan still needs a passive sniff scenario, route the bench wizard's
next operator step to the existing non-claiming passive capture handoff.

### Production Delta

Update `wheelctl moza bench-wizard` next-step selection so a recorded vendor
protocol evidence review prefers the next missing non-claiming passive sniff
plan before falling back to generic protocol-investigation text. The emitted
step preserves the command-bound `sniff-receipt`, `sniff-notes-template`,
`sniff-summary`, and `sniff-bundle` handoff commands and annotates the source as
`vendor_protocol_evidence_review_recorded`.

### Non-goals

No HID open, serial open, read-only query send, hardware output, authorization
receipt, PIDFF rerun, force increase, direct HID report `0xaf`, high torque,
serial config, firmware, DFU, native-control claim, native-visible claim,
smoke-ready claim, Pit House coexistence claim, simulator claim,
release-ready claim, raw `.pcapng` commit, semantic command decode, registry
promotion, or tuple sendability claim.

### Acceptance

- With a recorded vendor protocol evidence review and completed
  `pit-house-open-idle` / `pit-house-full-controls` sniff artifacts, a missing
  `pit-house-setting-change` receipt/summary becomes the bench-wizard
  `capture_passive_vendor_sniff` next operator step.
- The handoff includes the `sniff-notes-template` command so the exact
  setting/value/restore evidence remains required.
- The handoff keeps `hardware_output_allowed_now=false`,
  `hardware_attempt_command_emitted=false`, and `no_openracing_output=true`.
- Existing generated sniff commands remain parseable through the CLI parser.

### Proof Commands

```powershell
python scripts/cargo_fmt_workspace.py
cargo test --locked -p wheelctl --bin wheelctl vendor_protocol_review_routes_next_missing_passive_correlation_capture -- --nocapture
cargo test --locked -p wheelctl --bin wheelctl bench_wizard_sniff_next_operator_commands_parse -- --nocapture
cargo run --locked -p wheelctl --bin wheelctl -- moza bench-wizard --lane ci/hardware/moza-r5/2026-05-13 --json-out target/moza-current/bench-wizard-passive-correlation-handoff.json --md-out target/moza-current/bench-wizard-passive-correlation-handoff.md --json
cargo run --locked -p wheelctl --bin wheelctl -- moza verify-bundle --lane ci/hardware/moza-r5/2026-05-13 --stage native-visible-ready --json-out target/moza-current/native-visible-after-passive-correlation-handoff.json --json; if ($LASTEXITCODE -ne 4) { throw "expected native-visible verifier to remain blocked" }; $global:LASTEXITCODE = 0
cargo clippy --locked -p wheelctl --bin wheelctl --all-features -- -D warnings
cargo run --locked -p openracing-tools --bin package-surface -- --check
python scripts/policy_file.py
git diff --check
```

### Rollback

Revert only the bench-wizard next-step selection update, focused regression
test, and source-of-truth updates. Do not remove passive sniff plans,
checked-in sniff evidence, consumed hardware attempts, or protocol evidence
review receipts.

## Work item: hardware-doctor-usbpcap-device-hints

Status: completed
Linked proposal: docs/proposals/OR-PROP-0001-moza-native-visible-lane.md
Linked spec: docs/specs/OR-SPEC-0002-moza-r5-vendor-authority-test-lane.md
Linked ADR: docs/adr/0009-hardware-validation-evidence-state-machine.md
Blocks: deterministic passive USBPcap capture setup for setting-change evidence
Blocked by: `wheelctl hardware doctor` listed USBPcap root interfaces but did
not surface the attached USBPcap device value that identifies the Moza stack for
device-filtered passive captures.

### Goal

Make the no-output hardware doctor receipt expose USBPcap extcap device-filter
hints when the Moza R5 stack is visible, so the next passive setting-change
capture can select the correct controller and device without manual
rediscovery.

### Production Delta

Extend `wheelctl hardware doctor` USBPcap diagnostics to run
`USBPcapCMD --extcap-config` for discovered USBPcap interfaces, parse attached
device entries, and record Moza-relevant device hints such as the USBPcap
interface and `--devices` value. Update the passive capture checklist to tell
operators to refresh hardware doctor and prefer
`/tools/usbpcap_descriptor_capture/usbpcap_moza_device_hints` when selecting the
capture interface/filter.

### Non-goals

No HID open, serial open, read-only query send, hardware output, authorization
receipt, PIDFF rerun, force increase, direct HID report `0xaf`, high torque,
serial config, firmware, DFU, native-control claim, native-visible claim,
smoke-ready claim, Pit House coexistence claim, simulator claim,
release-ready claim, raw `.pcapng` commit, semantic command decode, registry
promotion, or tuple sendability claim.

### Acceptance

- `hardware doctor` remains observe-only and keeps all write/output/firmware
  false flags intact.
- USBPcap extcap config parsing records Moza-relevant hints with interface,
  capture-device value, matched child devices, and suggested capture filter.
- The passive capture checklist references the hardware-doctor
  `usbpcap_moza_device_hints` path before the operator starts USBPcap/Wireshark.
- No sniff receipt, summary, bundle manifest, raw capture, semantic decode,
  registry promotion, hardware output, or readiness claim is created.

### Proof Commands

```powershell
python scripts/cargo_fmt_workspace.py
cargo test --locked -p wheelctl --bin wheelctl usbpcap -- --nocapture
cargo test --locked -p wheelctl --bin wheelctl bench_wizard_sniff_next_operator_commands_parse -- --nocapture
cargo run --locked -p wheelctl --bin wheelctl -- --json hardware doctor --json-out target/moza-current/hardware-doctor-usbpcap-device-hints.json
cargo run --locked -p wheelctl --bin wheelctl -- moza bench-wizard --lane ci/hardware/moza-r5/2026-05-13 --json-out target/moza-current/bench-wizard-usbpcap-device-hints.json --md-out target/moza-current/bench-wizard-usbpcap-device-hints.md --json
cargo clippy --locked -p wheelctl --bin wheelctl --all-features -- -D warnings
cargo run --locked -p openracing-tools --bin package-surface -- --check
python scripts/policy_file.py
git diff --check
```

### Rollback

Revert only the hardware-doctor USBPcap device-hint parsing, passive capture
checklist wording, focused tests, and source-of-truth updates. Do not remove
passive sniff plans, checked-in sniff evidence, consumed hardware attempts, or
protocol evidence review receipts.

## Work item: native-visible-promotion

Status: blocked
Linked proposal: docs/proposals/OR-PROP-0001-moza-native-visible-lane.md
Linked spec: docs/specs/OR-SPEC-0001-moza-native-visible-lane.md
Linked ADR: docs/adr/0009-hardware-validation-evidence-state-machine.md
Blocks: controlled movement ladder
Blocked by: passing native-visible receipt from a future reviewed and authorized output family

### Goal

Promote the lane to `native_visible_ready` only if the verifier accepts a real
visible-motion receipt.

### Production delta

Create native-visible verification, manifest promotion, and lane audit receipts.

### Non-goals

No smoke-ready, release-ready, high-torque, Pit House, SimHub, simulator
telemetry, simulator FFB, or passive sniff claim.

### Acceptance

- `verify-bundle --stage native-visible-ready` passes.
- Manifest promotion records native visible state without simulator or release
  claims.
- Lane audit passes for native-visible.

### Proof commands

```powershell
wheelctl moza verify-bundle --lane ci/hardware/moza-r5/2026-05-13 --stage native-visible-ready --json-out ci/hardware/moza-r5/2026-05-13/native-visible-verification.json --json
wheelctl moza promote-manifest --lane ci/hardware/moza-r5/2026-05-13 --stage native-visible-ready --json-out ci/hardware/moza-r5/2026-05-13/manifest-promotion-native-visible.json --json
wheelctl moza audit-lane --lane ci/hardware/moza-r5/2026-05-13 --stage native-visible-ready --json-out ci/hardware/moza-r5/2026-05-13/lane-audit-native-visible.json --json
```

### Rollback

If promotion was incorrect, add a corrective PR that preserves the faulty
receipt and demotes the manifest with analysis. Do not erase evidence.

## Later work

- Capture the planned passive USB sniff scenarios, generate non-claiming
  sniff receipts and summaries, then decode vendor reports, map report IDs,
  identify enable/gain/mode handshakes, and design a reviewed vendor-control
  plan before considering any output attempt.
- Repeat 1 degree, then 3, 5, 10, 30, 90 right, and 90 return controlled
  movement only after new protocol evidence justifies a future output family and
  each rung has separate authorization.
- Refresh no-output KS/SR-P/HBP input captures as needed.
- Use passive USB sniffing for Pit House, SimHub, and simulator protocol
  research without readiness claims.
- Record Pit House coexistence, simulator telemetry, and bounded simulator FFB
  as external/smoke gates after native-visible work is settled.
