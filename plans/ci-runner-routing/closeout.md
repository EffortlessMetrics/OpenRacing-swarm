# Routed Rust CI capacity closeout

Status: completed
Plan: `plans/ci-runner-routing/implementation-plan.md`
Issue: #216
Implementation PR: #221
Merge commit: `06fdd7f5ed1d280ab7bad271d6405e1692ee71ba`

## Landed

- Restacked the useful runner-routing changes from #135 onto current main.
- Replaced the legacy CX33 route with capacity-aware CX43, CPX42, and CX53
  selection.
- Required host and capacity labels during organization runner discovery.
- Preserved GitHub-hosted fallback when no eligible runner is online and idle.
- Failed the normalized result on router API, credential, parse, or unknown
  target errors.
- Set the heavy routed lane to `cancel-in-progress: false`.
- Added the runner-routing policy guard and Windows-safe fixture test.
- Updated the CI efficiency and Rust Small proof documentation.

## Proof

Local proof on the pre-merge exact head:

- `scripts/check_runner_routing_test.sh` — pass;
- `scripts/check_runner_routing.sh` — pass;
- Git Bash `bash -n` and `scripts/check_shell_syntax.sh` — pass;
- `python scripts/policy_file.py --strict` — pass;
- `python scripts/policy_lint.py` — pass;
- `git diff --check` — pass.

Hosted proof:

- Workflow run `30720940292` on exact head `64379348e8810c23f8e96deff3b348d4c232f853` passed.
- `Route OpenRacing Rust Small` passed.
- GitHub-hosted fallback passed.
- CX43, CPX42, and CX53 implementation jobs were skipped for the selected
  fallback route.
- `OpenRacing Rust Small Result` passed.
- The standard workspace/build/governance matrix completed without failures
  before merge; CodeRabbit remained rate-limited advisory output.

## Claim boundary

This proves the workflow's route-selection and normalized-result behavior on
the hosted fallback path, plus local guard and policy behavior. It does not
prove that a CPX42, CX43, or CX53 runner is currently online, provisioned, or
available to the organization. The local organization-runner API query was
permission-denied, so no live capacity claim is made.

## Current capacity follow-up (#236)

The routing contract remains intact, but required self-hosted proof is
currently blocked before checkout or compilation by host capacity:

- PR #297 exact-head run `30777894276` selected CX43 and reported
  `/mnt/ci-scratch` with 66 GB free against the 100 GB disk guard;
- PR #299 exact-head run `30778182088` selected CX53 and reported
  `/mnt/ci-scratch` with 35 GB free against the 100 GB disk guard.
- PR #300 exact-head run `30865401104` selected CX43 and reported
  `/mnt/ci-scratch` with 65 GB free against the 100 GB disk guard; its
  normalized routed result failed before checkout or compilation.

These are runner-capacity failures, not product or route-selection failures.
The disk guard must remain unchanged. The parked PRs may be rerun only after
each affected runner passes its configured disk preflight and a fresh exact-
head routed job reaches its build/test steps. A successful router selection
alone is insufficient; the normalized result must also pass for the selected
runner.

## Follow-up

- Issue #215 was closed by PR #226, which reconciled the RIPR+ badge and
  quality-closure contract; issue #227 tracks the remaining producer work.
- Issue #236 tracks restoration of required CX43/CX53 runner capacity; no
  repository-side guard bypass or product-code change is in scope.
- Future runner-capacity changes must update this plan's route matrix and
  preserve the exact normalized-result proof.
