#!/usr/bin/env bash
# Run the packaged control-stream lifecycle harness against two tarballs.
#
# The harness is built from the checkout, but wheeld, wheelctl, the contract,
# and the replay fixture are resolved only from the extracted package roots.

set -euo pipefail

usage() {
    cat <<'USAGE'
Usage: control_stream_artifact_smoke.sh \
  --current PACKAGE.tar.gz \
  --previous PACKAGE.tar.gz \
  --harness PATH
USAGE
}

current_tar=""
previous_tar=""
harness=""

while [[ $# -gt 0 ]]; do
    case "$1" in
        --current)
            current_tar="$2"
            shift 2
            ;;
        --previous)
            previous_tar="$2"
            shift 2
            ;;
        --harness)
            harness="$2"
            shift 2
            ;;
        --help)
            usage
            exit 0
            ;;
        *)
            echo "error: unknown argument: $1" >&2
            usage >&2
            exit 2
            ;;
    esac
done

if [[ -z "$current_tar" || -z "$previous_tar" || -z "$harness" ]]; then
    echo "error: --current, --previous, and --harness are required" >&2
    usage >&2
    exit 2
fi
if [[ ! -f "$current_tar" || ! -f "$previous_tar" || ! -x "$harness" ]]; then
    echo "error: package tarballs and executable harness must exist" >&2
    exit 2
fi

work_dir="$(mktemp -d)"
cleanup() {
    rm -rf -- "$work_dir"
}
trap cleanup EXIT

extract_package() {
    local archive="$1"
    local destination="$2"
    mkdir -p "$destination"
    tar -xzf "$archive" -C "$destination"
    local package_root
    package_root="$(find "$destination" -mindepth 1 -maxdepth 1 -type d -print -quit)"
    if [[ -z "$package_root" ]]; then
        echo "error: archive did not contain a package directory: $archive" >&2
        exit 1
    fi
    printf '%s\n' "$package_root"
}

current_root="$(extract_package "$current_tar" "$work_dir/current")"
previous_root="$(extract_package "$previous_tar" "$work_dir/previous")"

"$harness" \
    --current-package "$current_root" \
    --previous-package "$previous_root"
