# Explicit Droid commands

`Droid Tag` is the bounded, secrets-backed path for a trusted maintainer to ask
Droid to act on an open pull request. It is advisory. It does not replace the
repository's deterministic checks or authorize a merge.

## Accepted surfaces

The workflow listens only to newly created pull-request conversation comments,
inline review comments, and submitted review bodies. It does not execute from
issue bodies, issue assignments, pull-request titles/descriptions, edited
content, forks, or non-PR issue comments.

The command must begin a line, ignoring leading whitespace, and use one of:

```text
@droid review
@droid fill
@droid security
```

Matching is case-insensitive. Text that merely mentions `@droid` or quotes a
command after another character does not authorize the secrets-backed job.
The actor must have GitHub author association `OWNER`, `MEMBER`, or
`COLLABORATOR`; the pinned action retains its own authorization checks.

## Two-stage control

`authorize` receives no repository secret. It parses the command, resolves the
pull request through the GitHub API, requires the PR to remain open, and verifies
that its head repository is exactly `github.repository`. It then publishes the
captured head SHA and PR number as job outputs.

Only an eligible result starts `droid`. That job checks out the captured head
with persisted checkout credentials disabled and invokes
`EffortlessMetrics/droid-action-safe@7c1377ccbacddc95560d1570547a5baa51de01ec`.
Raw debug artifacts and full output are disabled. Security findings cannot
submit a blocking request-changes review. An external-service failure remains
visible but non-blocking.

A head can move after authorization. The recorded SHA identifies the source tree
checked out for the invocation; it does not prove that an external comment was
published against an unchanged later PR head. A new command should be issued
when a review is needed after subsequent commits.

## Policy proof

```text
python scripts/check_droid_tag_workflow.py
python scripts/check_droid_tag_workflow.py --self-test
```

The self-test mutation-checks the safe action pin, artifact/output controls,
non-blocking security posture, fork boundary, exact-head checkout, checkout
credential handling, no-secret authorization job, command grammar, and trigger
surface. These checks prove repository configuration only, not the availability
or completeness of an external review.
