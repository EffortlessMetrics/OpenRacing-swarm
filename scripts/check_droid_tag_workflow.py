#!/usr/bin/env python3
"""Fail-closed structural contract for the secrets-backed Droid Tag workflow."""

from __future__ import annotations

import argparse
import re
from pathlib import Path

SAFE_ACTION = (
    "EffortlessMetrics/droid-action-safe@"
    "7c1377ccbacddc95560d1570547a5baa51de01ec"
)
CHECKOUT_ACTION = (
    "actions/checkout@d23441a48e516b6c34aea4fa41551a30e30af803"
)
TRUSTED_ASSOCIATIONS = '["OWNER","MEMBER","COLLABORATOR"]'


def _block(text: str, heading: str) -> str | None:
    match = re.search(
        rf"(?ms)^  {re.escape(heading)}:\n(?P<body>.*?)(?=^  [A-Za-z0-9_-]+:\n|\Z)",
        text,
    )
    return match.group("body") if match else None


def validate(text: str) -> list[str]:
    """Return every violated workflow invariant."""
    errors: list[str] = []

    on_match = re.search(r"(?ms)^on:\n(?P<body>.*?)(?=^[A-Za-z0-9_-]+:\n)", text)
    if not on_match:
        errors.append("missing top-level on block")
        on_block = ""
    else:
        on_block = on_match.group("body")

    for event in ("issue_comment", "pull_request_review_comment", "pull_request_review"):
        if not re.search(rf"(?m)^  {event}:\s*$", on_block):
            errors.append(f"missing allowed trigger: {event}")
    for event in ("issues", "pull_request", "pull_request_target", "workflow_dispatch"):
        if re.search(rf"(?m)^  {event}:\s*$", on_block):
            errors.append(f"forbidden trigger: {event}")

    authorize = _block(text, "authorize")
    droid = _block(text, "droid")
    if authorize is None:
        errors.append("missing authorize job")
        authorize = ""
    if droid is None:
        errors.append("missing droid job")
        droid = ""

    required_authorize = (
        "github.event.issue.pull_request",
        "author_association",
        TRUSTED_ASSOCIATIONS,
        "review|fill|security",
        "re.MULTILINE | re.IGNORECASE",
        'gh api \\\n            -H "Accept: application/vnd.github+json"',
        '"repos/${REPOSITORY}/pulls/${PR_NUMBER}"',
        "'.head.repo.full_name // empty'",
        "'.head.sha // empty'",
        "authorized_same_repository_command",
        "fork_pull_request",
        "pull_request_not_open",
        'echo "eligible=$eligible"',
    )
    for fragment in required_authorize:
        if fragment not in authorize:
            errors.append(f"authorize job missing contract fragment: {fragment}")
    if "secrets." in authorize:
        errors.append("authorize job must not receive repository secrets")
    if "permissions:\n      contents: read\n      pull-requests: read" not in authorize:
        errors.append("authorize job permissions are not the bounded read-only set")

    required_droid = (
        "needs: authorize",
        "if: needs.authorize.outputs.eligible == 'true'",
        CHECKOUT_ACTION,
        "ref: ${{ needs.authorize.outputs.head_sha }}",
        "persist-credentials: false",
        SAFE_ACTION,
        "factory_api_key: ${{ secrets.FACTORY_API_KEY }}",
        "github_token: ${{ github.token }}",
        "upload_debug_artifacts: false",
        "show_full_output: false",
        "security_block_on_critical: false",
        "security_block_on_high: false",
        "continue-on-error: true",
    )
    for fragment in required_droid:
        if fragment not in droid:
            errors.append(f"droid job missing contract fragment: {fragment}")

    action_refs = re.findall(r"(?m)^\s*uses:\s*([^\s#]+)", text)
    expected_refs = {CHECKOUT_ACTION, SAFE_ACTION}
    unexpected = [ref for ref in action_refs if ref not in expected_refs]
    if unexpected:
        errors.append(f"unexpected or mutable action reference(s): {unexpected}")
    if action_refs.count(SAFE_ACTION) != 1:
        errors.append("safe Droid action must appear exactly once")
    if "Factory-AI/droid-action@" in text:
        errors.append("upstream mutable/raw-upload Droid action is forbidden")

    if "cancel-in-progress: false" not in text:
        errors.append("active explicit reviews must not be cancelled")
    if "droid-tag-${{ github.event.issue.number || github.event.pull_request.number" not in text:
        errors.append("concurrency is not grouped by pull request")

    return errors


def self_test(good: str) -> list[str]:
    """Mutation-check the most important fail-closed invariants."""
    mutations = {
        "mutable Droid action": good.replace(SAFE_ACTION, "Factory-AI/droid-action@main"),
        "debug upload enabled": good.replace(
            "upload_debug_artifacts: false", "upload_debug_artifacts: true"
        ),
        "full output enabled": good.replace("show_full_output: false", "show_full_output: true"),
        "blocking critical review": good.replace(
            "security_block_on_critical: false", "security_block_on_critical: true"
        ),
        "fork guard removed": good.replace("fork_pull_request", "fork_was_not_checked"),
        "exact head removed": good.replace(
            "ref: ${{ needs.authorize.outputs.head_sha }}", "ref: main"
        ),
        "checkout credentials persisted": good.replace(
            "persist-credentials: false", "persist-credentials: true"
        ),
        "secret moved into authorize": good.replace(
            "timeout-minutes: 5", "timeout-minutes: 5\n    env:\n      BAD: ${{ secrets.BAD }}", 1
        ),
        "command grammar removed": good.replace("review|fill|security", "anything", 1),
        "PR-body trigger added": good.replace(
            "  pull_request_review:\n", "  pull_request:\n    types: [opened]\n  pull_request_review:\n",
            1,
        ),
    }

    failures: list[str] = []
    for name, mutated in mutations.items():
        if not validate(mutated):
            failures.append(f"mutation escaped validation: {name}")
    return failures


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--workflow",
        type=Path,
        default=Path(".github/workflows/droid.yml"),
    )
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()

    try:
        text = args.workflow.read_text(encoding="utf-8")
    except (OSError, UnicodeError) as error:
        print(f"droid-tag-policy: could not read {args.workflow}: {error}")
        return 2

    errors = validate(text)
    if errors:
        print("droid-tag-policy: FAILED")
        for error in errors:
            print(f" - {error}")
        return 1

    if args.self_test:
        failures = self_test(text)
        if failures:
            print("droid-tag-policy self-test: FAILED")
            for failure in failures:
                print(f" - {failure}")
            return 1
        print("droid-tag-policy self-test: 10 mutations caught")
    else:
        print("droid-tag-policy: OK")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
