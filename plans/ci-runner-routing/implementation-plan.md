# Routed Rust CI capacity implementation plan

Status: completed
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

## Work item: validate-static-runner-selectors

Status: proposed follow-up; the original rollout above remains completed
Owner: release/ci
Related PR: #308 (routing-gate portion only)
Linked proposal/spec/ADR: n/a; corrective CI policy work under this plan
Active goal: n/a; no product lane is activated

### Goal and production delta

Validate each job's own static YAML `runs-on` selector rather than scanning
an arbitrary 17-line window. The old window can borrow a neighbouring job's
capacity label; the inline regex also misses reordered bare labels and rejects
qualified inline arrays and comments. PR #308's missing-ripgrep guard does not
repair those semantic failures.

Keep the existing qualifier set and `em-ci-` group rule. Use Python/PyYAML
inside the existing shell entrypoint, install the parser in the policy job,
and run the gate's fixture suite in that job. Do not introduce a second
runtime or a new workflow lane.

### Acceptance

- Reject bare Linux/x64 self-hosted selectors regardless of label order,
  quoting, case, line breaks, aliases, or a neighbouring job's configuration.
- Accept equivalent qualified block, inline, and group/labels selectors.
- Comments and unrelated step text neither trigger nor qualify a selector.
- Missing Python/PyYAML, missing/empty workflow directories, invalid YAML,
  malformed job shapes, and file-read/traversal errors fail non-zero. Exit 1
  means a policy violation; exit 2 means the check could not complete.
- Preserve the existing qualification predicate; this does not replace the
  router's stricter capacity matrix. Dynamic expressions are reported but not
  evaluated. Reusable-workflow calls delegate runner selection to their callee;
  this guard does not fetch remote callees or claim to validate their runners.
- The candidate has exact-head policy proof before integration. Existing
  required routed proof remains required; #302 and #236 are not bypassed.

### Proof commands

```text
scripts/check_runner_routing_test.sh
scripts/check_runner_routing.sh
bash -n scripts/check_runner_routing.sh scripts/check_runner_routing_test.sh
python scripts/policy_file.py --strict
python scripts/policy_lint.py
git diff --check
OpenRacing Rust Small Result on the exact PR head
```

The self-test accepts unittest test names for bounded execution. Local fixture
validation is not proof that the full repository or hosted lane passed.

### Non-goals and rollback

No scratch deletion, runner provisioning, credential changes, fallback-result
changes, product/RT/hardware code, or support claims. Revert this work item's
script and policy-job changes together. Keep #308's scratch-cleanup work
separate from the static routing guard.
