# Droid PR Review

`Droid PR Review` is an advisory pull request review workflow. It can provide
useful automated review comments, but it is backed by an external credit-based
service and is not a deterministic repository gate. It runs for non-draft pull
requests when they are opened, synchronized, reopened, or marked ready for
review. A newer run for the same pull request cancels the older run.

On 2026-05-08, the workflow failed across multiple documentation and tooling PRs
with a service-side `402 Payment Required` / usage-limit response. That failure
did not indicate a repository test failure.

The workflow intentionally lets the action result remain visible: service,
credential, or model failures fail the review check instead of being silently
ignored. Required merge policy should still rely on deterministic project checks
such as CI, schema validation, YAML sync, compatibility tracking, security and
license audit, integration tests, and coverage. Do not add `droid-review` as a
required status check for `main`.

The workflow configures the MiniMax M3 BYOK model through the existing Factory
action and grants only the repository permissions required for review comments,
issue comments, OIDC exchange, and action metadata. Workflow and model tokens
are masked before the third-party action starts because the action may print its
environment while diagnosing a run.

If the team wants Droid review to become blocking later, first ensure the
external service has reliable credits, a stable model path, and an operational
runbook for service outages.
