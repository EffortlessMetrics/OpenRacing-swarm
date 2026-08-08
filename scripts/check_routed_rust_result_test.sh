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

echo "routed rust result contract: OK"
