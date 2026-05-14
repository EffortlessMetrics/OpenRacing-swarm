# Main Branch Protection

`main` should require the Linux correctness lane before merge. Windows and macOS
hosted runners are compatibility signals, not the default merge-safety gate for
ordinary Rust, docs, schema, parser-fixture, or hardware-receipt changes.

This document records the required merge policy for `main`. It exists because
repository rulesets are configured in GitHub, not in this repository.

## Current Audit

Audited on 2026-05-08 with:

```powershell
gh api repos/EffortlessMetrics/OpenRacing/branches/main/protection
gh api repos/EffortlessMetrics/OpenRacing/rulesets
gh api repos/EffortlessMetrics/OpenRacing/rulesets/12099933
```

Findings:

- Classic branch protection for `main` is not enabled.
- Repository ruleset `main` (`12099933`) is active for the default branch.
- The ruleset blocks branch deletion.
- The ruleset blocks non-fast-forward updates.
- The ruleset requires pull requests.
- The ruleset did not require status checks at the time of the audit.

That last point was the operational gap: a pull request could merge while long
CI jobs were still pending if a user or tool ran a merge command.

Follow-up on 2026-05-08: ruleset `12099933` was updated to require the Linux
correctness checks listed below with stale-check protection enabled. Hardware
receipt enforcement remains separate because the Moza receipt workflow is
path-filtered and should not be required globally for ordinary pull requests.

Follow-up on 2026-05-14: `Moza Focused Checks` was added to the global required
status list. The job is defined in the main CI workflow and concludes as skipped
for off-surface pull requests, so it can be required globally without blocking
docs-only or unrelated changes. For Moza parser, verifier, and receipt-plumbing
changes, it runs before merge and prevents the stable baseline from allowing an
early auto-merge while Moza-specific Rust tests or clippy are still pending.

## Required Policy

`main` must not accept a pull request until required checks have completed and
passed. This is especially important for hardware receipt PRs, where a premature
merge can make unvalidated evidence look accepted by the project history.

The `main` ruleset should include a required status check rule with stale-check
protection enabled. In the GitHub UI, configure:

- Rulesets -> `main` -> Rules -> Require status checks to pass.
- Enable "Require branches to be up to date before merging" if available.
- Add each required check by its exact status-check name.
- Keep pull requests required.
- Keep deletion and non-fast-forward protection enabled.
- Do not grant bypass actors for routine project work.

## Required PR Checks

Configure the `main` ruleset so pull requests cannot merge until the stable
baseline checks complete successfully. `PR Required Baseline` is intentionally
stable: it runs on every CI invocation, depends only on the fast global checks,
and publishes a fixed status before long path-scoped or advisory jobs finish.

Global required checks:

- `CHANGELOG Validation`
- `PR Change Filter`
- `Docs & Policy Checks`
- `PR Required Baseline`
- `Game support matrix sync`
- `track-compat-usage`
- `Moza Focused Checks`

`MSRV Check`, isolation builds, `Schemas & Trybuild`, and
`Workspace Default Build (ubuntu-latest)` are selected by PR surface, path, or
label for ordinary Rust/workspace, dependency, CI, performance, release, and
`full-ci` PRs. Docs-only, Moza-focused, and UI-only PRs can skip broad Rust
workspace gates when their focused checks cover the touched surface.
`Moza Focused Checks` is globally required because it is always present in CI:
it runs for Moza paths and hardware labels, and reports a skipped conclusion for
unrelated surfaces.

The regression and integration workflows (`Smoke Tests`, `User Journey Tests`,
`Acceptance Tests`, `Deprecated Field Detection`, `Trybuild Compile-Fail Tests`,
`JSON Schema Validation`, and `Protobuf Breaking Changes`) remain useful
signals, but should be path-scoped or folded into the stable baseline before
being made globally required for every ordinary PR.

Additional path-scoped or label-scoped checks should be required by path-scoped
rulesets only when the matching PR surface is present:

| PR surface | Required checks |
| --- | --- |
| Docs-only changes | `CHANGELOG Validation`, `PR Change Filter`, `Docs & Policy Checks`, `PR Required Baseline`; Rust workspace, feature, dependency, UI, and performance checks should be skipped unless `full-ci` is requested |
| Moza parser, verifier, or hardware receipt plumbing | `Moza Focused Checks` via the global ruleset; `Moza Receipt Verification` for `ci/hardware/**` and `crates/hid-moza-protocol/fixtures/**` |
| Hardware docs-only changes | Same docs-only lane unless the PR also changes real receipt artifacts or parser/verifier code |
| UI or packaging paths | `UI Isolation Build (ubuntu-22.04)`, `UI Isolation Build (ubuntu-24.04)` |
| Dependency, `Cargo.lock`, workspace feature, or `deny.toml` changes | `Feature Combinations`, `Dependency Governance`, `Security & License Audit`, `Comprehensive Lint Gates & Governance (ubuntu-latest)` |
| CI, workflow, scripts, or policy changes | `Feature Combinations`, `Dependency Governance`, `Comprehensive Lint Gates & Governance (ubuntu-latest)`, `Final Workspace Validation (ubuntu-latest)` |
| Performance-sensitive engine or integration-test paths | `Performance Gate` |
| Release-candidate or `full-ci` labeled PRs | `Feature Combinations`, `Dependency Governance`, `Comprehensive Lint Gates & Governance (ubuntu-latest)`, `Performance Gate`, `Security & License Audit`, `Final Workspace Validation (ubuntu-latest)`, `Stress Tests`, `CI Soak Test`, `Comprehensive Validation` |

For hardware receipt pull requests, require `Moza Receipt Verification` through a
path-scoped ruleset for `ci/hardware/**`, `crates/hid-moza-protocol/fixtures/**`,
and other real receipt/fixture paths. Do not make that check globally required
unless the workflow is guaranteed to run on every pull request. Hardware docs
alone are not receipt evidence and should not force receipt verification unless
they are packaged with receipt artifacts.

## Non-Required Checks

Do not require these checks for ordinary pull requests:

- `Windows Smoke`
- `macOS Smoke`
- `Cross-Platform Tests`
- `Cross-Platform Performance`
- `Windows Platform Smoke`
- `macOS Platform Smoke`
- `Linux UI Packaging Smoke`
- `Feature Combinations` for non-dependency, non-CI, non-release PRs
- `Dependency Governance` for non-dependency, non-CI, non-release PRs
- `Comprehensive Lint Gates & Governance (ubuntu-latest)` for ordinary docs,
  parser, receipt, and narrow CLI PRs where a focused check covers the touched
  surface
- `Performance Gate` for ordinary non-performance PRs
- `Security & License Audit` for ordinary non-dependency PRs
- `Final Workspace Validation (ubuntu-latest)` for ordinary PRs
- bot review checks, including `droid-review`, `CodeRabbit`, and similar advisory signals
- skipped coverage duplicates

Windows and macOS checks still run when a PR is labeled `windows`, `macos`, or
`platform`, when platform-sensitive paths change, or from manual/scheduled
platform-confidence workflows.

## Timing Policy

Hosted-runner timing results are useful telemetry, but they are not release-grade
real-time evidence. The normal PR lane keeps the Linux performance gate as a
merge signal and uses warn-only validation where hosted-runner jitter would make
strict RT assertions noisy. Strict jitter, missed-tick, and latency checks live
in the manual/scheduled `Timing Gates` workflow and should move to a
self-hosted perf-lab runner when one is available.

## Hardware Receipts

Moza hardware validation is evidence verification, not hosted-runner hardware
emulation. The `Hardware Receipt Verification` workflow runs on Linux and checks
dated receipt bundles, parser fixture replay, schemas, and claim boundaries. It
must not open HID devices, send FFB reports, run serial configuration, or issue
firmware/DFU commands.

For hardware PR review, also confirm:

- The PR claim ceiling matches the receipt stage.
- No staged receipt is missing from the lane manifest.
- No hardware validation boolean is promoted without matching receipts.
- No high-torque, serial configuration, firmware, or DFU claim is introduced by
  passive or zero-output receipt PRs.

## Verification Commands

Before merging a PR, use:

```powershell
gh pr checks <pr-number>
gh pr view <pr-number> --json mergeStateStatus,state,isDraft,headRefOid
```

The PR is merge-ready only when required checks are passing and GitHub reports a
mergeable state. Do not use `gh pr merge --auto` as a substitute for enforced
required checks; if the ruleset is incomplete, it can merge immediately.
