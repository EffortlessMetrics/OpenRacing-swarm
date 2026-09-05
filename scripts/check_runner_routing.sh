#!/usr/bin/env bash
set -euo pipefail

# Validate each job's own YAML selector. A line window can borrow labels from
# another job, and a failed text-search process is not proof of no violations.
python="${PYTHON:-python3}"
if ! command -v "$python" >/dev/null 2>&1; then
  echo "Runner routing check requires Python: $python" >&2
  exit 2
fi

exec "$python" - "${WORKFLOW_DIR:-.github/workflows}" <<'PY'
import os
from pathlib import Path
import re
import sys

try:
    import yaml
except ImportError:
    print("Runner routing check requires PyYAML; no workflows checked.", file=sys.stderr)
    sys.exit(2)

# PyYAML defaults to YAML 1.1, where unquoted yes/no/on/off become booleans.
# GitHub Actions follows YAML 1.2-style boolean semantics for workflow scalars,
# so preserve those runner labels as strings while keeping true/false boolean.
_BOOL_TAG = "tag:yaml.org,2002:bool"


class GitHubActionsLoader(yaml.SafeLoader):
    pass


GitHubActionsLoader.yaml_implicit_resolvers = {
    prefix: [
        (tag, pattern)
        for tag, pattern in resolvers
        if tag != _BOOL_TAG
    ]
    for prefix, resolvers in yaml.SafeLoader.yaml_implicit_resolvers.items()
}
GitHubActionsLoader.add_implicit_resolver(
    _BOOL_TAG,
    re.compile(r"^(?:true|false)$", re.IGNORECASE),
    list("tTfF"),
)

# Preserve the existing guard's qualifier set. This is a static bare-runner
# policy, not a replacement for the router's complete capacity/label matrix.
QUALIFIERS = {
    "em-ci", "ci-nano", "policy-nano", "workflow-nano", "rust-tiny",
    "rust-medium", "rust-large", "rust-16gb", "cx23", "cx33", "cx43",
    "cx53", "cpx42",
}
REQUIRED = {"self-hosted", "linux", "x64"}


def fail(message):
    print(f"Runner routing check could not complete: {message}", file=sys.stderr)
    sys.exit(2)


def walk_error(error):
    raise error


def selector_labels(selector, context):
    group = ""
    if isinstance(selector, dict):
        unknown_keys = [key for key in selector if key not in {"group", "labels"}]
        if unknown_keys:
            rendered = ", ".join(sorted(repr(key) for key in unknown_keys))
            fail(f"{context}: runs-on mapping contains unsupported key(s): {rendered}")
        if not selector:
            fail(f"{context}: runs-on mapping must contain group or labels")
        if "group" in selector:
            group = selector["group"]
            if not isinstance(group, str) or not group.strip():
                fail(f"{context}: runs-on.group must be a non-empty string")
        selector = selector.get("labels", [])
    if isinstance(selector, str):
        selector = [selector]
    if not isinstance(selector, list) or any(
        not isinstance(label, str) or not label.strip() for label in selector
    ):
        fail(f"{context}: runs-on must contain non-empty string labels")
    if not selector and not group:
        fail(f"{context}: runs-on must contain group or labels")
    labels = {label.casefold() for label in selector}
    dynamic = any("${{" in label for label in selector) or "${{" in group
    return labels, group, dynamic


root = Path(sys.argv[1])
if not root.is_dir():
    fail(f"workflow directory does not exist: {root}")

violations = []
files_checked = static_checked = dynamic_selectors = 0
try:
    paths = sorted(
        Path(directory) / name
        for directory, _, names in os.walk(root, onerror=walk_error)
        for name in names
        if name.endswith((".yml", ".yaml"))
    )
    if not paths:
        fail(f"no workflow YAML files found in {root}")
    for path in paths:
        with path.open(encoding="utf-8") as handle:
            workflow = yaml.load(handle, Loader=GitHubActionsLoader)
        if not isinstance(workflow, dict) or not isinstance(workflow.get("jobs"), dict) or not workflow["jobs"]:
            fail(f"{path}: expected a non-empty jobs mapping")
        for job_id, job in workflow["jobs"].items():
            context = f"{path}: jobs.{job_id}"
            if not isinstance(job, dict):
                fail(f"{context}: expected a job mapping")
            if "runs-on" not in job:
                uses = job.get("uses")
                if isinstance(uses, str) and uses.strip():
                    # Reusable workflows choose their own runners.
                    continue
                fail(f"{context}: missing runs-on or reusable-workflow uses")
            labels, group, dynamic = selector_labels(job["runs-on"], context)
            dynamic_selectors += int(dynamic)
            if not REQUIRED.issubset(labels):
                continue
            static_checked += 1
            # An expression is not evidence of an em-ci group. Concrete labels
            # can still independently qualify a selector containing expressions.
            qualified_group = group.startswith("em-ci-") and "${{" not in group
            if not qualified_group and not labels.intersection(QUALIFIERS):
                violations.append(f"{context}: bare self-hosted/linux/x64 selector lacks group/capacity labels")
        files_checked += 1
except (OSError, UnicodeError, yaml.YAMLError) as error:
    fail(str(error))

for violation in violations:
    print(violation, file=sys.stderr)
print(
    f"Checked {files_checked} workflow(s), {static_checked} static Linux/x64 self-hosted selector(s); "
    f"{dynamic_selectors} selector(s) contain expressions not evaluated by this static gate."
)
sys.exit(1 if violations else 0)
PY
