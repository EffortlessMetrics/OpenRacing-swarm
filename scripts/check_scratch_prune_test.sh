#!/usr/bin/env bash
# Fixture coverage for the "Prune abandoned scratch dirs" step in
# .github/workflows/em-ci-routed-rust.yml.
#
# The step runs on shared self-hosted runners with rm -rf, before the disk
# guard and before checkout, so it cannot live in a repository script: nothing
# is checked out yet. The workflow is therefore the single source of truth, and
# this test extracts the step body straight out of the YAML and exercises it
# against a fixture tree.
#
# What matters is what the step refuses to delete: the running job's own
# directories, anything with recent activity, and anything outside the two run
# directory parents. Each of those has a case below.
#
# Usage: scripts/check_scratch_prune_test.sh

set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

workflow=".github/workflows/em-ci-routed-rust.yml"
tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT

step_body="$tmp_dir/prune.sh"

# Capture the extractor's own status. `if ! cmd; then status=$?` would read
# the negated status, not the exit code, and silently turn the PyYAML skip
# into a failure.
set +e
WORKFLOW_PATH="$workflow" BODY_PATH="$step_body" python3 - <<'PY'
import os
import sys

try:
    import yaml
except ModuleNotFoundError:
    sys.exit(97)

STEP_NAME = "Prune abandoned scratch dirs"
EXPECTED_JOBS = {"rust-small-cx43", "rust-small-cpx42", "rust-small-cx53"}

with open(os.environ["WORKFLOW_PATH"], encoding="utf-8") as handle:
    workflow = yaml.safe_load(handle)

found = {}
for job_id, job in workflow["jobs"].items():
    for step in job.get("steps", []):
        if step.get("name") != STEP_NAME:
            continue
        found[job_id] = step

missing = EXPECTED_JOBS - set(found)
if missing:
    print(f"self-hosted jobs missing the prune step: {sorted(missing)}", file=sys.stderr)
    sys.exit(1)

extra = set(found) - EXPECTED_JOBS
if extra:
    print(f"unexpected jobs carrying the prune step: {sorted(extra)}", file=sys.stderr)
    sys.exit(1)

# Every self-hosted lane must run byte-identical logic, or a fix applied to one
# runner silently misses the others.
bodies = {job_id: step["run"] for job_id, step in found.items()}
if len(set(bodies.values())) != 1:
    print("prune step bodies differ across self-hosted jobs", file=sys.stderr)
    sys.exit(1)

envs = {job_id: step.get("env", {}) for job_id, step in found.items()}
if len({tuple(sorted(env.items())) for env in envs.values()}) != 1:
    print("prune step env blocks differ across self-hosted jobs", file=sys.stderr)
    sys.exit(1)

env = next(iter(envs.values()))
if env.get("SCRATCH_ROOT") != "/mnt/ci-scratch":
    print(f"unexpected SCRATCH_ROOT: {env.get('SCRATCH_ROOT')!r}", file=sys.stderr)
    sys.exit(1)
if "SCRATCH_MAX_AGE" not in env:
    print("prune step does not declare SCRATCH_MAX_AGE", file=sys.stderr)
    sys.exit(1)

# The prune must come before the guard, and must not change it.
for job_id, job in workflow["jobs"].items():
    if job_id not in EXPECTED_JOBS:
        continue
    names = [step.get("name") for step in job["steps"]]
    guards = [n for n in names if n in ("Disk guards", "Disk and memory preflight")]
    if not guards:
        print(f"{job_id} has no disk guard step", file=sys.stderr)
        sys.exit(1)
    if names.index(STEP_NAME) > names.index(guards[0]):
        print(f"{job_id} runs the prune after its disk guard", file=sys.stderr)
        sys.exit(1)

guard_lines = [
    line.strip()
    for job_id in EXPECTED_JOBS
    for step in workflow["jobs"][job_id]["steps"]
    if step.get("name") in ("Disk guards", "Disk and memory preflight")
    for line in step["run"].splitlines()
    if "ci-disk-guard /mnt/ci-scratch" in line
]
if sorted(guard_lines) != sorted(
    ["ci-disk-guard /mnt/ci-scratch 100", "ci-disk-guard /mnt/ci-scratch 80", "ci-disk-guard /mnt/ci-scratch 100"]
):
    print(f"scratch disk guard thresholds changed: {guard_lines}", file=sys.stderr)
    sys.exit(1)

with open(os.environ["BODY_PATH"], "w", encoding="utf-8") as handle:
    handle.write(next(iter(bodies.values())))
PY
extract_status=$?
set -e

if [ "$extract_status" -eq 97 ]; then
  echo "scratch prune contract: SKIPPED (PyYAML not installed)"
  exit 0
fi

if [ "$extract_status" -ne 0 ]; then
  echo "scratch prune contract: FAILED (extractor exited $extract_status)" >&2
  exit 1
fi

echo "OK: prune step is present, identical, and ahead of an unchanged disk guard."

failures=0

fail() {
  echo "FAIL: $*" >&2
  failures=$((failures + 1))
}

assert_exists() {
  [ -e "$1" ] || fail "expected $1 to survive: $2"
}

assert_missing() {
  if [ -e "$1" ]; then
    fail "expected $1 to be pruned: $2"
  fi
}

build_fixture() {
  local root="$1"
  rm -rf "$root"
  mkdir -p "$root/target/1000-1/debug/deps" \
           "$root/target/2000-1/debug/deps" \
           "$root/target/3000-1/debug/deps" \
           "$root/tmp/1000-1" \
           "$root/tmp/2000-1" \
           "$root/other/keep-me"

  touch "$root/target/1000-1/debug/deps/a.o" \
        "$root/target/2000-1/debug/deps/a.o" \
        "$root/target/3000-1/debug/deps/a.o" \
        "$root/other/keep-me/file"

  # Age everything depth-first so the parents end up old too. 3000-1 is then
  # given a fresh mtime two levels down: a live job mid-build.
  local aged
  for aged in "$root/target/1000-1" "$root/target/2000-1" "$root/target/3000-1" \
              "$root/tmp/1000-1" "$root/tmp/2000-1" "$root/other/keep-me"; do
    find "$aged" -depth -exec touch -d '3 days ago' {} +
  done
  touch "$root/target/3000-1/debug"
}

echo "== case: removes abandoned run dirs only =="
root="$tmp_dir/scratch"
build_fixture "$root"
SCRATCH_ROOT="$root" \
SCRATCH_MAX_AGE="6 hours ago" \
CARGO_TARGET_DIR="$root/target/2000-1" \
TMPDIR="$root/tmp/2000-1" \
GITHUB_STEP_SUMMARY="$tmp_dir/summary.md" \
  bash "$step_body" > "$tmp_dir/out.txt" 2>&1 || fail "prune step exited non-zero"

assert_missing "$root/target/1000-1" "abandoned target dir"
assert_missing "$root/tmp/1000-1" "abandoned tmp dir"
assert_exists "$root/target/2000-1" "this job's CARGO_TARGET_DIR"
assert_exists "$root/tmp/2000-1" "this job's TMPDIR"
assert_exists "$root/target/3000-1" "recent activity two levels down"
assert_exists "$root/other/keep-me" "tree outside target/ and tmp/"
grep -q "scratch free MB" "$tmp_dir/out.txt" || fail "no free-space line in output"
grep -q "scratch prune" "$tmp_dir/summary.md" || fail "no step summary line written"

echo "== case: an unusable age threshold keeps everything =="
# If the age check cannot run, an empty result must not be read as "stale".
# Failing open here would delete a live job's build tree.
root="$tmp_dir/badage"
build_fixture "$root"
SCRATCH_ROOT="$root" \
SCRATCH_MAX_AGE="not a parseable date" \
CARGO_TARGET_DIR="$root/target/2000-1" \
TMPDIR="$root/tmp/2000-1" \
  bash "$step_body" > /dev/null 2>&1 || fail "an unusable age threshold must not fail the step"
assert_exists "$root/target/1000-1" "age check could not run"
assert_exists "$root/target/3000-1" "age check could not run"
assert_exists "$root/tmp/1000-1" "age check could not run"

echo "== case: a missing scratch root is a no-op, not an error =="
SCRATCH_ROOT="$tmp_dir/absent" \
SCRATCH_MAX_AGE="6 hours ago" \
CARGO_TARGET_DIR="$tmp_dir/absent/target/1-1" \
TMPDIR="$tmp_dir/absent/tmp/1-1" \
  bash "$step_body" > /dev/null 2>&1 || fail "missing scratch root should not fail the step"

echo "== case: an undeletable entry does not fail the step =="
if [ "$(id -u)" -eq 0 ]; then
  echo "   skipped: root ignores the directory permissions this case relies on"
else
root="$tmp_dir/locked"
build_fixture "$root"
chmod a-w "$root/target"
SCRATCH_ROOT="$root" \
SCRATCH_MAX_AGE="6 hours ago" \
CARGO_TARGET_DIR="$root/target/2000-1" \
TMPDIR="$root/tmp/2000-1" \
  bash "$step_body" > /dev/null 2>&1 || fail "an undeletable entry must not fail the step"
chmod u+w "$root/target"
fi

if [ "$failures" -ne 0 ]; then
  echo "$failures assertion(s) failed." >&2
  exit 1
fi

echo "OK: scratch prune fixtures pass."
