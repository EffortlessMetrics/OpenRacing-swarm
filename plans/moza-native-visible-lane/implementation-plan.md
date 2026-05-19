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

`docs/hardware/moza-r5-completion-audit.md` maps the broader Moza lane objective
to concrete receipts and confirms that the objective is still incomplete:
native-visible, Pit House coexistence, simulator telemetry, bounded simulator
FFB, and smoke-ready promotion remain missing.

`ci/hardware/sniff/moza-r5/2026-05-13` now contains plan-only passive USB sniff
artifacts for Pit House, SimHub, and simulator protocol research. The artifact
index marks those plans as `present_non_claiming`; each scenario remains
`partial_or_unaccepted` until a matching pcap receipt and summary exist.

The latest pre-output and artifact-index receipts also surface diagnostic
candidate-only R5 V1 extended slots for the brake, clutch, and handbrake
captures. Those candidates keep the passive evidence navigable while preserving
`input_semantic_mapping_complete=false`; they do not prove role-specific input
semantics or readiness.

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
extended-slot details that the readiness and artifact-index renderers surface.
`lane-capture-analysis.json` and `role-status-sync.json` identify brake,
clutch, and handbrake candidates as diagnostic only with `readiness_claim=false`;
they still leave role-specific input semantics incomplete.

The current blocked-state handoff is
`plans/moza-native-visible-lane/handoff.md`. Use it when no active goal work
item is ready; do not invent new no-output work just to keep the lane moving.

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
