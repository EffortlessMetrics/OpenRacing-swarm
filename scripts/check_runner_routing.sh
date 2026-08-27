#!/usr/bin/env bash
set -euo pipefail

bad=0
workflow_dir="${WORKFLOW_DIR:-.github/workflows}"

# Every check below is an `rg` invocation. When `rg` is absent each one exits
# 127, which reads as "no matches", so the gate reports success without having
# inspected a single file. A policy gate that cannot run must fail, not pass:
# the GitHub-hosted image this runs on does not ship ripgrep, and the gate was
# silently vacuous there.
if ! command -v rg > /dev/null 2>&1; then
  echo "ripgrep (rg) is required by this gate but was not found on PATH." >&2
  echo "Refusing to report success: nothing would be checked." >&2
  exit 2
fi

echo "Checking for bare self-hosted runner usage..."

if [ ! -d "$workflow_dir" ]; then
  echo "No $workflow_dir directory found; skipping runner routing check."
  exit 0
fi

if rg -n 'runs-on:[[:space:]]*\[[^]]*self-hosted[^]]*linux[^]]*x64[^]]*\]' "$workflow_dir"; then
  echo "Bare inline self-hosted/linux/x64 runs-on is forbidden." >&2
  bad=1
fi

while IFS= read -r -d '' file; do
  while IFS=: read -r line _; do
    window="$(sed -n "${line},$((line+16))p" "$file")"

    if printf '%s\n' "$window" | rg -q '^[[:space:]]*-[[:space:]]*linux[[:space:]]*$' &&
       printf '%s\n' "$window" | rg -q '^[[:space:]]*-[[:space:]]*x64[[:space:]]*$' &&
       ! printf '%s\n' "$window" | rg -q 'group:[[:space:]]*em-ci-' &&
       ! printf '%s\n' "$window" | rg -q '^[[:space:]]*-[[:space:]]*(em-ci|ci-nano|policy-nano|workflow-nano|rust-tiny|rust-medium|rust-large|rust-16gb|cx23|cx33|cx43|cx53|cpx42)[[:space:]]*$'; then
      echo "$file:$line: bare self-hosted block lacks group/capacity labels" >&2
      bad=1
    fi
  done < <(rg -n --no-filename '^[[:space:]]*-[[:space:]]*self-hosted[[:space:]]*$' "$file" || true)
done < <(find "$workflow_dir" -type f -print0)

exit "$bad"
