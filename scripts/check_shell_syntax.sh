#!/usr/bin/env bash
# Parse every tracked shell script.
#
# Packaging and release scripts are not exercised by `cargo test`, so a syntax
# error in one of them stays invisible until a tag is pushed and the release
# workflow fails. This gate catches that at PR time.
#
# Usage: scripts/check_shell_syntax.sh

set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

failed=0
checked=0

while IFS= read -r script; do
    checked=$((checked + 1))
    if ! bash -n "$script"; then
        echo "FAIL: $script does not parse" >&2
        failed=$((failed + 1))
    fi
done < <(git ls-files '*.sh')

if [[ "$failed" -ne 0 ]]; then
    echo "" >&2
    echo "$failed of $checked shell script(s) failed to parse." >&2
    exit 1
fi

echo "OK: all $checked tracked shell scripts parse."
