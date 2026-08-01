# Routed Rust CI capacity implementation plan

Status: active
Owner: release/ci
Linked issue: https://github.com/EffortlessMetrics/OpenRacing-swarm/issues/216
Candidate PR: https://github.com/EffortlessMetrics/OpenRacing-swarm/pull/135
Current implementation: current-main successor in review
Linked ADRs: n/a
Active goal: n/a; this plan supports CI infrastructure rather than the active control-stream product lane

## Problem

The routed Rust workflow still selects the legacy CX33 lane and labels it as
`rust-small`. The candidate change moves the 16 GiB lane to CPX42, adds
capacity-aware runner discovery, and makes router configuration errors fail
the normalized result. The repository does not yet carry the route/capacity
contract needed to review that workflow change safely.

## Work item: reconcile-routed-rust-capacity

Status: in_progress
Blocks: merge of PR #135 or a clean successor
Blocked by: this plan artifact

### Goal

Make the routed Rust CI capacity matrix, fallback behavior, and policy guard
explicit and consistent across workflow, scripts, and CI documentation.

### Production delta

- Replace the legacy CX33 16 GiB route only when the live runner contract
  supports the CPX42 and capacity labels.
- Require explicit capacity labels during runner discovery.
- Fail the normalized result on router infrastructure or configuration errors.
- Add a policy guard for bare self-hosted runner blocks.
- Document route order, labels, fallback, and the proof boundary.

### Acceptance

- The documented route matrix matches the workflow's route choices, runner
  labels, and normalized-result assertions.
- GitHub-hosted fallback remains explicit and testable.
- The routing guard rejects bare self-hosted/linux/x64 blocks and accepts
  capacity-qualified blocks.
- Workflow YAML, shell syntax, policy, and exact-head required Rust proof pass.
- The change claims CI routing behavior only; it does not claim runner
  provisioning, product/runtime behavior, or release readiness.

### Proof commands

```text
scripts/check_runner_routing.sh
bash -n scripts/check_runner_routing.sh
workflow YAML parser/linter for changed workflows
python scripts/policy_file.py --strict
focused routing guard fixtures or shell tests
OpenRacing Rust Small Result on the exact PR head
git diff --check
```

### Non-goals

- No runner provisioning, labels, secrets, or organization settings.
- No product/runtime Rust changes.
- No broad CI redesign or new expensive validation lane.
- No merge from the stale PR #135 head without current-main restacking and
  fresh proof.

### Rollback

Revert the workflow, policy guard, and documentation changes. Existing routed
Rust jobs and GitHub-hosted fallback remain the rollback baseline.
