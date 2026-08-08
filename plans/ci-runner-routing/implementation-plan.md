# Routed Rust CI capacity implementation plan

Status: in progress
Owner: release/ci
Linked issue: https://github.com/EffortlessMetrics/OpenRacing-swarm/issues/216
Candidate PR: https://github.com/EffortlessMetrics/OpenRacing-swarm/pull/135
Current implementation: PR #221 merged as 06fdd7f5; exact-head hosted proof passed
Linked ADRs: n/a
Active goal: n/a; this plan supports CI infrastructure rather than the active control-stream product lane

## Problem

The routed Rust workflow still selects the legacy CX33 lane and labels it as
`rust-small`. The candidate change moves the 16 GiB lane to CPX42, adds
capacity-aware runner discovery, and makes router configuration errors fail
the normalized result. The repository does not yet carry the route/capacity
contract needed to review that workflow change safely.

## Work item: reconcile-routed-rust-capacity

Status: completed
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

## Work item: fall-back-when-selected-runner-is-unfit

Status: in progress
Blocks: merge of every pull request that routes to an unfit self-hosted runner
Blocked by: n/a

### Problem

The router runs on `ubuntu-latest` and selects a self-hosted lane purely from
the organization runner API, where the only health signals are `online` and
`busy`. A runner can satisfy both and still be unable to build — for example
when `/mnt/ci-scratch` sits below the 100 GB disk guard. The router then emits
`router_error=false` and picks that runner, the lane fails at the guard before
checkout, and the GitHub-hosted lane stays skipped because it only runs when
*no* runner is idle.

The result is a permanently unmergeable pull request: the required normalized
result can never pass while an idle-but-unfit runner keeps winning the route.
Issue #236 records the capacity side of this; this work item covers the
repository-side routing gap that turns a transient capacity problem into a
total merge stop.

### Production delta

- Publish each self-hosted lane's disk-guard verdict as a `preflight` job
  output: `unfit` when the guard tripped and the lane died before checkout,
  `ok` once it got far enough to build.
- Add a `rust-small-github-fallback` job that runs when the router succeeded
  without error and the selected self-hosted lane reported `unfit`. Keying on
  the verdict rather than on the lane result keeps build and test failures
  blocking, so an environment-sensitive defect cannot fail on the selected
  runner and be waved through by a hosted pass.
- Run the identical `cargo check` and `cargo test --lib` commands used by
  every other lane, so the fallback re-proves rather than waives.
- Accept the fallback in the normalized result only when the selected lane
  actually ran, failed, and reported `unfit`; a skipped lane is never
  rescued, and a missing verdict is not an infrastructure verdict. The
  result step enforces this independently of the fallback job's own gate.
- Emit a workflow warning and record `proof_lane` in the step summary so a
  degraded run is visible rather than silent.
- Add `scripts/check_routed_rust_result_test.sh` covering the normalized
  result contract, and run it from the policy workflow.

### Acceptance

- A selected self-hosted lane that fails its disk guard yields a passing
  normalized result via the fallback, with `proof_lane=github-fallback`.
- A genuine code failure leaves `preflight=ok`, so the fallback never runs
  and the normalized result still fails -- including when a hosted run of
  the same commit would have passed.
- The disk guard thresholds are unchanged and no lane skips build or test.
- Router error, unknown target, and single-selected-lane invariants still
  fail the normalized result.
- Policy, shell syntax, routing guard, and the new contract test pass.

### Proof commands

```text
scripts/check_routed_rust_result_test.sh
scripts/check_runner_routing.sh
scripts/check_runner_routing_test.sh
scripts/check_shell_syntax.sh
python scripts/policy_file.py --strict
python scripts/policy_lint.py
cargo check --workspace --exclude racing-wheel-ui --locked
git diff --check
OpenRacing Rust Small Result on the exact PR head
```

### Non-goals

- No change to the disk guard thresholds or to any lane's build/test commands.
- No runner provisioning, labels, secrets, or organization settings; issue
  #236 still owns restoring real capacity.
- No product or runtime Rust changes.
- No bypass of required proof; the fallback re-runs it on other hardware.

### Rollback

Revert the `rust-small-github-fallback` job, the normalized-result changes,
and the contract test. The previous behavior — selected lane must succeed —
is the rollback baseline.
