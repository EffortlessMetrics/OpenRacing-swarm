# Droid Auto Review

`Droid Auto Review` is an advisory pull request review workflow. It can provide
useful automated review comments, but it is backed by an external credit-based
service and is not a deterministic repository gate.

## Workflow contract

- Runs for `opened`, `synchronize`, `reopened`, and `ready_for_review` pull-request events.
- Uses one concurrency group per pull request. An active review is not cancelled;
  GitHub retains only the newest queued run.
- Checks out the exact pull-request head with immutable action references and
  `persist-credentials: false`.
- Binds `MINIMAX_API_KEY` at job scope, masks only non-empty bound credentials,
  and writes the minimal MiniMax M3 BYOK model to
  `~/.factory/settings.local.json`. The same file is passed to the Droid action
  through its supported `settings` input.
- Enables both automatic code review and automatic security review.
- Grants `contents: read`; the workflow comments document the remaining
  `pull-requests`, `issues`, `id-token`, and `actions` permissions required by
  the Factory action.

The checkout and Droid action SHAs are immutable. If either dependency is
upgraded, verify the replacement commit against the intended upstream ref and
record the new verification in the pull request.

## Claim boundary

The Droid action uses `continue-on-error: true`. Its step and logs remain
visible, and a follow-up warning records an advisory failure, but an outage,
credit limit, missing secret, or external review finding does not become a
repository correctness gate. Required merge policy must rely on deterministic
project checks such as CI, schema validation, YAML sync, compatibility
tracking, security and license audit, integration tests, and coverage. Do not
add `droid-review` as a required status check for `main`.

The automatic-review and security-review inputs are proof that both review
paths were requested for the exact pull-request head. They do not prove that
the external service completed successfully or that its findings are complete.

On 2026-05-08, the workflow failed across multiple documentation and tooling
PRs with a service-side `402 Payment Required` / usage-limit response. That
failure did not indicate a repository test failure and remains the reason this
workflow is advisory.
