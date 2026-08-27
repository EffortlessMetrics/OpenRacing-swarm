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

- Issue #215 remains open for reconciling the RIPR+ badge generator and
  quality-closure checker contract.
- Future runner-capacity changes must update this plan's route matrix and
  preserve the exact normalized-result proof.

## Follow-up: routed scratch prune (issue #236)

Landed after the original closeout, as a repository-owned mitigation for the
`/mnt/ci-scratch` capacity failures recorded on issue #236.

### Why

Each self-hosted lane writes its cargo target and temp trees to
`/mnt/ci-scratch/{target,tmp}/<run_id>-<attempt>` and removes them in an
`if: always()` cleanup step. A job that is cancelled, or whose runner is lost,
never reaches that step, so its run directories survive. They accumulate until
`ci-disk-guard /mnt/ci-scratch 100` refuses to start any further build, which
is the failure mode reported on #236 for both CX43 (66 GB free) and CX53
(35 GB free).

### What landed

- A `Prune abandoned scratch dirs` step on `rust-small-cx43`,
  `rust-small-cpx42`, and `rust-small-cx53`, running before each lane's
  unchanged disk guard.
- It removes only depth-1 entries under `/mnt/ci-scratch/target` and
  `/mnt/ci-scratch/tmp`. The running job's `CARGO_TARGET_DIR` and `TMPDIR` are
  skipped by name, and any candidate with activity in its first two levels
  within `SCRATCH_MAX_AGE` (6 hours) is kept. The longest `timeout-minutes` on
  any routed self-hosted lane is 150, so a live job's scratch directory cannot
  reach that age.
- The step never fails its job: it does not use `set -e`, and an entry it
  cannot remove is reported and skipped.
- The age check fails safe. If `find` cannot answer, the candidate is kept: an
  empty result must mean "nothing recent found", never "the check could not
  run", or an unparseable threshold would delete a live job's build tree.
- `/mnt/ci-cache` (shared `cargo-home` and `sccache`) and `/mnt/docker` are
  untouched, so the warm caches every lane depends on survive the prune.
- `scripts/check_scratch_prune_test.sh` extracts the step body from the
  workflow and exercises it against a fixture tree. The step cannot live in a
  repository script, because it runs before checkout; the workflow is the
  single source of truth and the test reads it from there.
- The contract test also asserts the three lanes run byte-identical logic, that
  the prune is ordered ahead of the guard, and that the scratch guard
  thresholds are still `100`, `80`, `100`.
- Wired into `.github/workflows/policy.yml` as `Routed scratch prune contract`.
  That step installs PyYAML first: `setup-python` supplies its own interpreter
  without it, and the test would otherwise skip. A skipped gate proves nothing,
  so the install is what makes the contract actually run in CI.

### Proof

- `scripts/check_scratch_prune_test.sh` — pass.
- Both harness paths exercised: with PyYAML unimportable the test reports
  `SKIPPED` and exits 0; with a seeded contract violation it reports the
  offending value and exits 1.
- Fault injection against the contract test, each mutation caught: dropping the
  current-job skip, dropping the recency check, letting one lane's body drift,
  lowering a scratch guard threshold, and reading a failed age check as
  "stale".
- `scripts/check_shell_syntax.sh` — pass, 16 scripts.
- `scripts/check_runner_routing.sh` — pass.
- `python scripts/policy_file.py --strict` — pass.
- `python scripts/policy_lint.py` — pass.
- `git diff --check` — pass.

### Claim boundary

This proves the prune step's logic against a fixture tree, and that it is
ordered ahead of an unchanged disk guard on all three self-hosted lanes. It
does **not** prove that CX43 or CX53 currently has 100 GB free, that the
reclaimed space is sufficient on either runner, or that any routed lane now
passes. No live runner evidence was obtainable: organization runner discovery
still returns HTTP 401 under issue #302, so no exact-head routed job could be
dispatched to a self-hosted lane to observe the prune.

Issue #236 therefore stays open and still owns real capacity restoration. This
change only removes the repository's own abandoned directories; if the space is
consumed by something else, or by directories younger than six hours, the guard
correctly continues to fail closed.
