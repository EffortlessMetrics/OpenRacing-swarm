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

## Follow-up

- Issue #215 was closed by PR #226, which reconciled the RIPR+ badge and
  quality-closure contract; issue #227 tracks the remaining producer work.
- Issue #236 tracks restoration of required CX43/CX53 runner capacity; no
  repository-side guard bypass or product-code change is in scope.
- Future runner-capacity changes must update this plan's route matrix and
  preserve the exact normalized-result proof.

## Routing gap found while parked on issue #236

The capacity shortfall recorded above exposed a repository-side routing gap
that is separate from runner provisioning.

Runner discovery treats `online` and not `busy` as fit to build. A runner can
satisfy both and still fail the 100 GB disk guard before checkout. When that
happens the router emits `router_target=cx43` with `router_error=false`, the
selected lane fails at its first step, and the GitHub-hosted lane stays
skipped because it was wired to run only when *no* runner is idle.

Evidence on PR #300 head `a42a0101`, run `30865505398`:

- `router_target=cx43`, `router_reason=cx43_idle`, `router_error=false`;
- `disk guard failed: /mnt/ci-scratch has 65GB free, needs 100GB`, exit 75;
- `cx43_result=failure`, `github_result=skipped`;
- `OpenRacing Rust Small Result` failed.

So an idle-but-unfit runner wins the route indefinitely and the required check
can never pass. That converts a transient capacity problem into a total merge
stop for every parked pull request.

The `fall-back-when-selected-runner-is-unfit` work item in this plan closes the
gap by re-running the identical check/test proof on the GitHub-hosted lane when
a selected self-hosted lane reports its disk-guard preflight as `unfit`. The
disk guard, its thresholds, and every lane's build and test commands are
unchanged.

The fallback keys on that preflight verdict rather than on the lane merely
failing. A `cargo check` or `cargo test` failure leaves `preflight=ok`, so the
fallback never launches and the required check stays red. That matters because
the lanes do not share an environment, so retrying any failure would let an
environment-sensitive defect fail on the selected runner and pass on hosted.

Issue #236 remains the owner of restoring real capacity — this only stops the
repository from being wedged while that work is pending.

## Current capacity follow-up (#236)

The routing contract remains intact, but required self-hosted proof is
currently blocked before checkout or compilation by host capacity:

- PR #297 exact-head run `30777894276` selected CX43 and reported
  `/mnt/ci-scratch` with 66 GB free against the 100 GB disk guard;
- PR #299 exact-head run `30778182088` selected CX53 and reported
  `/mnt/ci-scratch` with 35 GB free against the 100 GB disk guard;
- PR #300 exact-head run `30865401104` selected CX43 and reported
  `/mnt/ci-scratch` with 65 GB free against the 100 GB disk guard; its
  normalized routed result failed before checkout or compilation.

These are runner-capacity failures, not product or route-selection failures.
The disk guard must remain unchanged. Resume proof has three distinct cases:

- `router_error=true` fails closed;
- when discovery succeeds and the selected runner reports `preflight=unfit`
  before checkout, identical hosted proof may satisfy the required result;
- when the selected runner reaches build or test and fails, the required
  result remains red; hosted success cannot rescue it.

A successful router selection alone is insufficient; the normalized result
must also pass for the selected route or the explicitly permitted degraded
fallback route.
