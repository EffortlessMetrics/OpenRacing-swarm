#!/usr/bin/env bash
# Exercises the normalized-result contract of the routed Rust workflow.
#
# The result step decides whether required proof exists for a pull request.
# It must accept exactly one successful lane, accept the GitHub-hosted
# fallback when a selected self-hosted lane fails, and reject everything
# else -- in particular a genuine code failure that also fails on hosted.
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/.." && pwd)"
workflow="$repo_root/.github/workflows/em-ci-routed-rust.yml"

tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT

logic="$tmp_dir/result_logic.sh"

extract_status=0
WORKFLOW_PATH="$workflow" LOGIC_PATH="$logic" python3 - <<'PY' || extract_status=$?
import os
import sys

try:
    import yaml
except ModuleNotFoundError:
    sys.exit(97)

with open(os.environ["WORKFLOW_PATH"], encoding="utf-8") as handle:
    workflow = yaml.safe_load(handle)

steps = workflow["jobs"]["rust-small-result"]["steps"]
run_steps = [step for step in steps if "run" in step]
if len(run_steps) != 1:
    print(f"expected exactly one run step, found {len(run_steps)}", file=sys.stderr)
    sys.exit(1)

with open(os.environ["LOGIC_PATH"], "w", encoding="utf-8") as handle:
    handle.write(run_steps[0]["run"])
PY

if [ "$extract_status" -eq 97 ]; then
  echo "routed rust result contract: SKIPPED (PyYAML not installed)"
  exit 0
fi

if [ "$extract_status" -ne 0 ] || [ ! -s "$logic" ]; then
  echo "could not extract the rust-small-result run step from $workflow" >&2
  echo "(extractor exit ${extract_status}); refusing to report a contract result" >&2
  exit 1
fi

failures=0

# case: name|route|target|router_error|cx43|cpx42|cx53|github|fallback|want_exit
run_case() {
  local name route target router_error cx43 cpx42 cx53 github fallback want
  IFS='|' read -r name route target router_error cx43 cpx42 cx53 github fallback want <<< "$1"

  local output status
  set +e
  output="$(
    ROUTE_RESULT="$route" \
    TARGET="$target" \
    ROUTER_REASON="test" \
    ROUTER_ERROR="$router_error" \
    TRUSTED_SELF_HOSTED="true" \
    CX43_RESULT="$cx43" \
    CPX42_RESULT="$cpx42" \
    CX53_RESULT="$cx53" \
    GITHUB_RESULT="$github" \
    FALLBACK_RESULT="$fallback" \
    GITHUB_STEP_SUMMARY=/dev/null \
    bash "$logic" 2>&1
  )"
  status=$?
  set -e

  if [ "$status" -ne "$want" ]; then
    printf 'FAIL  %s (want exit %s, got %s)\n' "$name" "$want" "$status" >&2
    printf '%s\n' "$output" | sed 's/^/        /' >&2
    failures=$((failures + 1))
  fi
}

# Selected lane succeeded: unchanged, pre-existing behavior.
run_case "cx43 success|success|cx43|false|success|skipped|skipped|skipped|skipped|0"
run_case "cpx42 success|success|cpx42|false|skipped|success|skipped|skipped|skipped|0"
run_case "cx53 success|success|cx53|false|skipped|skipped|success|skipped|skipped|0"
run_case "github success|success|github|false|skipped|skipped|skipped|success|skipped|0"

# Selected self-hosted lane failed (for example the disk guard tripped before
# checkout) but the identical proof passed on the GitHub-hosted fallback.
run_case "cx43 fail then fallback|success|cx43|false|failure|skipped|skipped|skipped|success|0"
run_case "cpx42 fail then fallback|success|cpx42|false|skipped|failure|skipped|skipped|success|0"
run_case "cx53 fail then fallback|success|cx53|false|skipped|skipped|failure|skipped|success|0"

# A real defect fails on both lanes and must stay blocked.
run_case "cx43 fail and fallback fail|success|cx43|false|failure|skipped|skipped|skipped|failure|1"
run_case "github fail is not rescued|success|github|false|skipped|skipped|skipped|failure|skipped|1"

# The fallback must never substitute for a lane that never ran.
run_case "skipped lane not rescued|success|cx43|false|skipped|skipped|skipped|skipped|success|1"

# Router integrity checks are preserved.
run_case "router job failed|failure|cx43|false|success|skipped|skipped|skipped|skipped|1"
run_case "router infra error|success|github|true|skipped|skipped|skipped|success|skipped|1"
run_case "unknown target|success|bogus|false|skipped|skipped|skipped|skipped|skipped|1"

# Exactly one implementation lane may run.
run_case "two lanes ran|success|cx43|false|success|skipped|success|skipped|skipped|1"

if [ "$failures" -ne 0 ]; then
  echo "routed rust result contract: $failures case(s) failed" >&2
  exit 1
fi

# The cases above cover the result step, which decides whether proof exists.
# They cannot cover whether the fallback job is *launched*, because that is a
# workflow-level `if:` expression rather than shell. Evaluate that expression
# directly so the acceptance path is proven without needing a self-hosted
# runner to be online.
gate_status=0
WORKFLOW_PATH="$workflow" python3 - <<'PY' || gate_status=$?
import os
import re
import sys

try:
    import yaml
except ModuleNotFoundError:
    sys.exit(97)

with open(os.environ["WORKFLOW_PATH"], encoding="utf-8") as handle:
    workflow = yaml.safe_load(handle)

jobs = workflow["jobs"]
gate = jobs["rust-small-github-fallback"]["if"]

needs = jobs["rust-small-github-fallback"]["needs"]
for required in (
    "route-rust-small",
    "rust-small-cx43",
    "rust-small-cpx42",
    "rust-small-cx53",
):
    if required not in needs:
        print(f"fallback job does not depend on {required}", file=sys.stderr)
        sys.exit(1)

# The gate asks whether *any* self-hosted lane failed, not whether the
# *selected* one did. That is only safe because exactly one lane can ever run:
# each lane is pinned to its own target, so the others are skipped rather than
# failed. Assert that invariant here -- if a lane were ever allowed to run for
# a target other than its own, a non-selected failure could launch a fallback
# while the selected lane succeeded.
for lane, target in (
    ("rust-small-cx43", "cx43"),
    ("rust-small-cpx42", "cpx42"),
    ("rust-small-cx53", "cx53"),
):
    condition = " ".join(jobs[lane]["if"].split())
    if f"needs.route-rust-small.outputs.target == '{target}'" not in condition:
        print(f"{lane} is not pinned to target '{target}': {condition!r}", file=sys.stderr)
        sys.exit(1)

# The expression uses only always(), equality against string literals, &&, ||
# and parentheses. Translate that subset faithfully rather than approximating
# GitHub Actions semantics in general.
ALLOWED = re.compile(
    r"^(?:\s|\(|\)|&&|\|\||==|'[a-z-]*'|always\(\)|needs\.[a-z0-9-]+\.(?:result|outputs\.[a-z_]+))+$"
)
if not ALLOWED.match(gate):
    print(f"fallback gate uses unsupported syntax: {gate!r}", file=sys.stderr)
    sys.exit(1)


def evaluate(expression, results, outputs):
    def lookup(match):
        job, field = match.group(1), match.group(2)
        if field == "result":
            value = results[job]
        else:
            value = outputs[job][field.split(".", 1)[1]]
        return repr(value)

    python_expr = re.sub(
        r"needs\.([a-z0-9-]+)\.(result|outputs\.[a-z_]+)", lookup, expression
    )
    python_expr = python_expr.replace("always()", "True")
    python_expr = python_expr.replace("&&", " and ").replace("||", " or ")
    return bool(eval(python_expr, {"__builtins__": {}}, {}))  # noqa: S307


def scenario(route_result, error, cx43, cpx42, cx53):
    return (
        {
            "route-rust-small": route_result,
            "rust-small-cx43": cx43,
            "rust-small-cpx42": cpx42,
            "rust-small-cx53": cx53,
        },
        {"route-rust-small": {"error": error, "target": "unused"}},
    )


gate_cases = [
    # A selected self-hosted lane failed: the fallback must run.
    ("cx43 failed", scenario("success", "false", "failure", "skipped", "skipped"), True),
    ("cpx42 failed", scenario("success", "false", "skipped", "failure", "skipped"), True),
    ("cx53 failed", scenario("success", "false", "skipped", "skipped", "failure"), True),
    # Nothing failed, or nothing self-hosted ran: the fallback must stay out.
    ("cx43 succeeded", scenario("success", "false", "success", "skipped", "skipped"), False),
    ("all skipped", scenario("success", "false", "skipped", "skipped", "skipped"), False),
    # Router problems are still owned by the result step, not papered over here.
    ("router errored", scenario("success", "true", "failure", "skipped", "skipped"), False),
    ("router job failed", scenario("failure", "false", "failure", "skipped", "skipped"), False),
]

gate_failures = 0
for name, (results, outputs), expected in gate_cases:
    actual = evaluate(gate, results, outputs)
    if actual != expected:
        print(
            f"FAIL  fallback gate '{name}': expected {expected}, got {actual}",
            file=sys.stderr,
        )
        gate_failures += 1

if gate_failures:
    sys.exit(1)
PY

if [ "$gate_status" -eq 97 ]; then
  echo "routed rust fallback gate: SKIPPED (PyYAML not installed)"
elif [ "$gate_status" -ne 0 ]; then
  echo "routed rust fallback gate: FAILED" >&2
  exit 1
else
  echo "routed rust fallback gate: OK"
fi

echo "routed rust result contract: OK"
