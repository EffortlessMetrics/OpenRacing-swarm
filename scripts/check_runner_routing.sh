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
import sys

try:
    import yaml
except ImportError:
    print("Runner routing check requires PyYAML; no workflows checked.", file=sys.stderr)
    sys.exit(2)

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
        group = selector.get("group", "")
        if not isinstance(group, str):
            fail(f"{context}: runs-on.group must be a string")
        selector = selector.get("labels", [])
    if isinstance(selector, str):
        selector = [selector]
    if not isinstance(selector, list) or any(not isinstance(label, str) for label in selector):
        fail(f"{context}: runs-on must contain string labels")
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
            workflow = yaml.safe_load(handle)
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
