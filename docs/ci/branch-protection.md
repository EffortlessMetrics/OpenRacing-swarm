# Main Branch Protection

`main` should require the Linux correctness lane before merge. Windows and macOS
hosted runners are compatibility signals, not the default merge-safety gate for
ordinary Rust, docs, schema, parser-fixture, or hardware-receipt changes.

## Required PR Checks

Configure the `main` ruleset so pull requests cannot merge until these checks
complete successfully:

- `CHANGELOG Validation`
- `MSRV Check`
- `CLI Isolation Build (ubuntu-latest)`
- `Service Isolation Build (ubuntu-latest)`
- `Plugins Isolation Build (ubuntu-latest)`
- `UI Isolation Build (ubuntu-22.04)`
- `UI Isolation Build (ubuntu-24.04)`
- `Schemas & Trybuild`
- `Workspace Default Build (ubuntu-latest)`
- `Feature Combinations`
- `Dependency Governance`
- `Comprehensive Lint Gates & Governance (ubuntu-latest)`
- `Performance Gate`
- `Security & License Audit`
- `Final Workspace Validation (ubuntu-latest)`
- `Smoke Tests`
- `Performance Gates`
- `User Journey Tests`
- `Stress Tests`
- `CI Soak Test`
- `Acceptance Tests`
- `Deprecated Field Detection`
- `Trybuild Compile-Fail Tests`
- `JSON Schema Validation`
- `Lint Enforcement`
- `Protobuf Breaking Changes`
- `Comprehensive Validation`
- `Game support matrix sync`
- `track-compat-usage`

For hardware receipt pull requests, require `Moza Receipt Verification` through a
path-scoped ruleset for `ci/hardware/**`, `crates/hid-moza-protocol/fixtures/**`,
and `docs/hardware/**`. Do not make that check globally required unless the
workflow is guaranteed to run on every pull request.

## Non-Required Checks

Do not require these checks for ordinary pull requests:

- `Windows Smoke`
- `macOS Smoke`
- `Cross-Platform Tests`
- `Cross-Platform Performance`
- `Windows Platform Smoke`
- `macOS Platform Smoke`
- `Linux UI Packaging Smoke`
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
