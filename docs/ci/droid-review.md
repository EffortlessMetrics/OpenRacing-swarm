# Droid Auto Review

`Droid PR Review` is an advisory pull-request review workflow. It can provide
useful automated comments, but an external model or service result is not a
deterministic repository gate.

## Workflow contract

- Runs for `opened`, `synchronize`, `reopened`, and `ready_for_review` events on
  non-draft, same-repository pull requests. Fork pull requests do not receive
  the secrets-backed review job.
- Uses one concurrency group per pull request with `cancel-in-progress: false`.
  The active review finishes and GitHub retains only the newest queued run.
- Checks out the exact pull-request head with immutable action references,
  shallow history, and `persist-credentials: false`.
- Binds `MINIMAX_API_KEY` at job scope and writes the minimal MiniMax M3 BYOK
  model to transient settings under `~/.factory` and `$RUNNER_TEMP`.
- Invokes the pinned organization-safe Droid action with
  `upload_debug_artifacts: false` and `show_full_output: false`. The pinned
  action does not upload raw `~/.factory` or raw prompt files; the caller also
  removes its transient BYOK settings after the action returns.
- Requests automatic code review and automatic security review, but disables
  request-changes submission for both critical and high findings. Findings stay
  advisory rather than becoming an indirect branch-protection gate.
- Grants `contents: read`; workflow comments document the remaining
  `pull-requests`, `issues`, `id-token`, and `actions` permissions used by the
  Droid action.

The checkout and Droid action references are immutable. The Droid pin is
`EffortlessMetrics/droid-action-safe@7c1377ccbacddc95560d1570547a5baa51de01ec`.
At that revision, debug collection and upload are both gated by
`upload_debug_artifacts == 'true'`, and only a separately sanitized directory
is eligible for upload. Any future pin change must re-verify that behavior
before the workflow is updated.

## Claim boundary

The action step uses `continue-on-error: true`; a warning records an advisory
failure. Outages, credit limits, missing secrets, model failures, and findings
do not replace deterministic project checks such as CI, schema validation,
YAML sync, compatibility tracking, security and license audit, integration
tests, and coverage. Do not add this workflow as a required status check for
`main`.

The automatic-review and security-review inputs prove only that both review
paths were requested for the exact pull-request head. They do not prove that
the external service completed successfully or that its findings are complete.

On 2026-05-08, the workflow failed across multiple documentation and tooling
pull requests with a service-side `402 Payment Required` / usage-limit response.
That failure did not indicate a repository test failure and remains one reason
this workflow is advisory.
