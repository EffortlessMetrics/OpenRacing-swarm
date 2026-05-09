#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(dirname "$SCRIPT_DIR")"

MODE="full"
ALLOW_DIRTY=0
SKIP_PERFORMANCE=0
FORCE_PERFORMANCE=0
SKIP_COVERAGE=0
SKIP_SECURITY=0
SKIP_MINIMAL_VERSIONS=0
ALLOW_LOCK_UPDATE=0
BUF_AGAINST=""

usage() {
    cat << 'EOF'
OpenRacing CI-equivalent runner (Linux/WSL).

Usage:
  scripts/ci_nix.sh [options]

Modes:
  fast  - isolation builds + workspace default + lint gates + final validation
  full  - all phases including schema validation, feature combinations,
          dependency governance, performance gates, security audit, coverage

Options:
  --mode <full|fast>       Run full (default) or fast subset
  --fast                   Alias for --mode fast
  --allow-dirty            Skip clean-tree checks for generated files
  --skip-performance       Skip performance gate steps
  --force-performance      Run performance gate even on WSL
  --skip-coverage          Skip coverage step
  --skip-security          Skip security audit and license checks
  --skip-minimal-versions  Skip nightly minimal-versions check
  --allow-lock-update      Allow Cargo.lock to change (otherwise fail if it changes)
  --buf-against <ref>      Run buf breaking against git ref
  --help                   Show this help
EOF
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --mode)
            MODE="$2"
            shift 2
            ;;
        --fast)
            MODE="fast"
            shift
            ;;
        --allow-dirty)
            ALLOW_DIRTY=1
            shift
            ;;
        --skip-performance)
            SKIP_PERFORMANCE=1
            shift
            ;;
        --force-performance)
            FORCE_PERFORMANCE=1
            shift
            ;;
        --skip-coverage)
            SKIP_COVERAGE=1
            shift
            ;;
        --skip-security)
            SKIP_SECURITY=1
            shift
            ;;
        --skip-minimal-versions)
            SKIP_MINIMAL_VERSIONS=1
            shift
            ;;
        --allow-lock-update)
            ALLOW_LOCK_UPDATE=1
            shift
            ;;
        --buf-against)
            BUF_AGAINST="$2"
            shift 2
            ;;
        --help)
            usage
            exit 0
            ;;
        *)
            echo "Unknown option: $1"
            usage
            exit 1
            ;;
    esac
done

phase() {
    echo ""
    echo "=== $1 ==="
}

run() {
    echo "+ $*"
    "$@"
}

require_tool() {
    local tool="$1"
    if ! command -v "$tool" >/dev/null 2>&1; then
        echo "Missing required tool: $tool"
        exit 1
    fi
}

ensure_clean_tree() {
    if [[ $ALLOW_DIRTY -eq 1 ]]; then
        return
    fi

    if ! git diff --quiet || ! git diff --cached --quiet; then
        echo "Working tree is dirty. Commit or stash changes, or pass --allow-dirty."
        exit 1
    fi
}

capture_lockfile_hash() {
    LOCK_BEFORE="$(sha256sum Cargo.lock 2>/dev/null || echo "missing")"
}

check_lockfile_unchanged() {
    if [[ $ALLOW_LOCK_UPDATE -eq 1 ]]; then
        return
    fi

    local lock_after
    lock_after="$(sha256sum Cargo.lock 2>/dev/null || echo "missing")"
    if [[ "$LOCK_BEFORE" != "$lock_after" ]]; then
        echo ""
        echo "ERROR: Cargo.lock changed during CI run."
        echo "Re-run with --allow-lock-update if intentional, or revert and regenerate."
        git --no-pager diff -- Cargo.lock 2>/dev/null || true
        exit 1
    fi
}

check_generated_clean() {
    local label="$1"

    if [[ $ALLOW_DIRTY -eq 1 ]]; then
        return
    fi

    if ! git diff --exit-code; then
        echo "$label generated changes. Please regenerate and commit."
        exit 1
    fi
}

ensure_rustup() {
    require_tool rustup

    if ! rustup show >/dev/null 2>&1; then
        echo "rustup is not initialized. Run 'rustup show' to set it up."
        exit 1
    fi
}

ensure_toolchain() {
    local toolchain="$1"
    if ! rustup toolchain list | grep -q "^${toolchain}"; then
        run rustup toolchain install "$toolchain"
    fi
}

ensure_component() {
    local toolchain="$1"
    local component="$2"

    if ! rustup component list --toolchain "$toolchain" | grep -q "^${component} (installed)"; then
        run rustup component add "$component" --toolchain "$toolchain"
    fi
}

setup_env() {
    export CARGO_TERM_COLOR=always
    export RUST_BACKTRACE=1
    export PATH="$HOME/.cargo/bin:$PATH"

    if command -v sccache >/dev/null 2>&1; then
        export SCCACHE_GHA_ENABLED=true
        export RUSTC_WRAPPER=sccache
    fi
}

is_wsl() {
    if [[ -n "${WSL_INTEROP:-}" ]]; then
        return 0
    fi
    if [[ -f /proc/version ]] && grep -qi microsoft /proc/version; then
        return 0
    fi
    return 1
}

run_isolation_builds() {
    phase "Isolation builds"
    run cargo +stable build -p wheelctl --locked
    run cargo +stable run -p wheelctl -- --help

    run cargo +stable build -p racing-wheel-service --locked
    run cargo +stable run -p racing-wheel-service --bin wheeld -- --help

    run cargo +stable build -p racing-wheel-plugins --locked
    run cargo +stable build -p racing-wheel-plugins --features sample-plugins
}

run_schema_validation() {
    phase "Schemas and trybuild"
    run cargo +stable build -p racing-wheel-schemas --locked
    run cargo +stable test -p racing-wheel-schemas --test trybuild_guards
    run cargo +stable test -p racing-wheel-schemas schema_roundtrip

    if [[ -n "$BUF_AGAINST" ]]; then
        run buf breaking --against "$BUF_AGAINST"
    else
        echo "Skipping buf breaking (no --buf-against provided)"
    fi

    run buf build
}

run_workspace_default() {
    phase "Workspace default build"
    run cargo +stable build --workspace --locked
    run cargo +stable test --workspace --no-run
}

run_feature_combinations() {
    phase "Feature combinations"
    run cargo +stable build --workspace --all-features --locked
    run cargo +stable build --workspace --no-default-features --locked
    run cargo +stable test --workspace --all-features --no-run
}

run_dependency_governance() {
    phase "Dependency governance"

    local duplicates
    duplicates=$(cargo +stable tree --duplicates || true)
    if [[ -n "$duplicates" ]]; then
        echo "Duplicate dependencies found:"
        echo "$duplicates"
        exit 1
    fi

    run cargo +stable hakari generate
    check_generated_clean "cargo hakari"

    run cargo +nightly udeps --workspace --all-targets --exclude workspace-hack

    if [[ $SKIP_MINIMAL_VERSIONS -eq 0 ]]; then
        echo "Running minimal-versions build (this updates Cargo.lock)."
        run cargo +nightly update -Z minimal-versions
        run cargo +nightly build --workspace -Z minimal-versions --locked
    else
        echo "Skipping minimal-versions check"
    fi
}

run_lint_gates() {
    phase "Lint gates"
    run python3 scripts/lint_gates.py

    echo "Running additional governance checks (print statements)"
    PRINT_HITS=$(find crates/ -name "*.rs" \
        -not -path "*/tests/*" \
        -not -path "*/integration-tests/*" \
        -not -path "*/examples/*" \
        -not -path "*/benches/*" \
        -not -path "*/cli/*" \
        -not -path "*/hid-capture/*" \
        -not -path "*/openracing-capture-ids/*" \
        -not -path "*/openracing-test-helpers/*" \
        -not -path "*/plugin-examples/*" \
        -not -name "build.rs" \
        -not -name "main.rs" \
        -not -name "*_tests.rs" \
        -not -name "tests.rs" \
        -not -name "allocation_tracker.rs" \
        -print0 \
    | xargs -0 grep -nE '(println!|print!|dbg!|eprintln!|eprint!)\s*\(' 2>/dev/null \
    | grep -vE ':[0-9]+:\s*//' || true)

    if [ -n "$PRINT_HITS" ]; then
        echo "Print statements found in non-test code. Use tracing macros instead."
        echo "$PRINT_HITS"
        exit 1
    fi
}

run_performance_gate() {
    phase "Performance gate"
    run cargo +stable build --profile rt --bin wheeld --locked
    run cargo +stable bench --bench rt_timing -- --output-format json | tee bench_results.json
    run python3 scripts/validate_performance.py bench_results.json --strict
    run RUST_LOG=debug cargo +stable test --release test_zero_alloc_rt_path
}

run_security_audit() {
    phase "Security and license audit"
    run cargo +stable audit --deny warnings
    run cargo +stable deny check
    run cargo +stable run -p openracing-tools --bin validate-adr -- --verbose
    run cargo +stable run -p openracing-tools --bin generate-docs-index --
}

run_coverage() {
    phase "Coverage"
    run cargo +stable llvm-cov --all-features --workspace --lcov --output-path lcov.info
}

run_final_validation() {
    phase "Final validation"
    run cargo +stable build --workspace --locked
    run cargo +stable test --workspace --no-run --locked

    run cargo +stable build -p wheelctl --locked
    run cargo +stable build -p racing-wheel-service --locked
    run cargo +stable build -p racing-wheel-plugins --locked

    run cargo +stable build --workspace --all-features --locked
    run cargo +stable build --workspace --no-default-features --locked

    run cargo +stable test --workspace --locked
}

main() {
    cd "$ROOT_DIR"

    require_tool git
    require_tool python3
    ensure_rustup

    setup_env

    ensure_toolchain stable
    ensure_component stable rustfmt
    ensure_component stable clippy

    if [[ $SKIP_COVERAGE -eq 0 ]]; then
        ensure_component stable llvm-tools-preview
    fi

    if [[ $SKIP_MINIMAL_VERSIONS -eq 0 ]]; then
        ensure_toolchain nightly
    fi

    ensure_clean_tree
    capture_lockfile_hash

    if is_wsl && [[ $FORCE_PERFORMANCE -eq 0 ]]; then
        SKIP_PERFORMANCE=1
        echo "WSL detected: skipping performance gate (use --force-performance to run)"
    fi

    if [[ "$MODE" == "fast" ]]; then
        run_isolation_builds
        run_workspace_default
        run_lint_gates
        run_final_validation
        check_lockfile_unchanged
        phase "CI run complete (fast mode)"
        echo "All checks passed."
        return
    fi

    run_isolation_builds
    run_schema_validation
    run_workspace_default
    run_feature_combinations
    run_dependency_governance
    run_lint_gates

    if [[ $SKIP_PERFORMANCE -eq 0 ]]; then
        run_performance_gate
    else
        echo "Skipping performance gate"
    fi

    if [[ $SKIP_SECURITY -eq 0 ]]; then
        run_security_audit
    else
        echo "Skipping security audit"
    fi

    if [[ $SKIP_COVERAGE -eq 0 ]]; then
        run_coverage
    else
        echo "Skipping coverage"
    fi

    run_final_validation

    check_lockfile_unchanged

    phase "CI run complete"
    echo "All checks passed."
}

main "$@"
