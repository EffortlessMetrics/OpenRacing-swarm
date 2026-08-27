#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT

mkdir "$tmp_dir/good" "$tmp_dir/bad"

cat > "$tmp_dir/good/qualified.yml" <<'YAML'
jobs:
  qualified:
    runs-on:
      - self-hosted
      - linux
      - x64
      - em-ci
      - rust-medium
YAML

cat > "$tmp_dir/bad/unqualified.yml" <<'YAML'
jobs:
  unqualified:
    runs-on:
      - self-hosted
      - linux
      - x64
YAML

cat > "$tmp_dir/bad/inline.yml" <<'YAML'
jobs:
  inline:
    runs-on: [self-hosted, linux, x64]
YAML

WORKFLOW_DIR="$tmp_dir/good" "$script_dir/check_runner_routing.sh"

if WORKFLOW_DIR="$tmp_dir/bad" "$script_dir/check_runner_routing.sh"; then
  echo "expected unqualified runner fixtures to fail" >&2
  exit 1
fi

echo "runner routing fixtures: OK"

# A gate that cannot run must fail rather than pass. Every check in the script
# is an `rg` call, and `rg` exiting 127 reads as "no matches", so without an
# explicit guard the gate reports success on the very fixtures above. The
# GitHub-hosted image this runs on does not ship ripgrep, so that was the live
# behaviour in CI.
#
# Model it the way CI hits it: `rg` absent from PATH entirely. Symlink only the
# externals the script actually uses.
mkdir "$tmp_dir/minimal-path"
for tool in bash sed find; do
  tool_path="$(command -v "$tool")"
  if [ -z "$tool_path" ]; then
    echo "cannot build a minimal PATH: $tool not found" >&2
    exit 1
  fi
  ln -s "$tool_path" "$tmp_dir/minimal-path/$tool"
done

if [ -n "$(PATH="$tmp_dir/minimal-path" command -v rg 2>/dev/null)" ]; then
  echo "minimal PATH still exposes rg; the case would not test anything" >&2
  exit 1
fi

if PATH="$tmp_dir/minimal-path" WORKFLOW_DIR="$tmp_dir/bad" \
     "$script_dir/check_runner_routing.sh" > /dev/null 2>&1; then
  echo "expected a missing rg to fail the gate, not pass it" >&2
  exit 1
fi

echo "OK: runner routing gate fails closed when ripgrep is unavailable."
