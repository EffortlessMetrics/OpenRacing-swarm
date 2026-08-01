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
