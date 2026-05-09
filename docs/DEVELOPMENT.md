# Development Guide

This document provides an overview of the development process, tooling, and standards for the Racing Wheel Software Suite.

## Architecture Decision Records (ADRs)

All significant architectural decisions are documented in ADRs located in `docs/adr/`. See [ADR README](adr/README.md) for the complete process.

### Current ADRs
- [ADR-0001: Force Feedback Mode Matrix](adr/0001-ffb-mode-matrix.md)
- [ADR-0002: IPC Transport Layer](adr/0002-ipc-transport.md) 
- [ADR-0003: OWP-1 Protocol Specification](adr/0003-owp1-protocol.md)
- [ADR-0004: Real-Time Scheduling Architecture](adr/0004-rt-scheduling-architecture.md)
- [ADR-0005: Plugin Architecture](adr/0005-plugin-architecture.md)
- [ADR-0006: Safety Interlocks and Fault Management](adr/0006-safety-interlocks.md)
- [ADR-0007: Multi-Vendor HID Protocol Architecture](adr/0007-multi-vendor-hid-protocol-architecture.md)
- [ADR-0008: Game Auto-Configure and Telemetry Bridge](adr/0008-game-auto-configure-telemetry-bridge.md)

## Continuous Integration

The CI pipeline enforces code quality, performance, and security standards:

### Test Matrix
- **Platforms**: Ubuntu, Windows, macOS
- **Rust Versions**: Stable, Beta (Ubuntu only)
- **Test Types**: Unit, integration, doc tests

### Performance Gates
- **P99 Jitter**: ≤ 0.25ms at 1kHz (NFR-01)
- **Missed Ticks**: ≤ 0.001% rate
- **Processing Time**: ≤ 50μs median, ≤ 200μs p99
- **Memory**: Zero heap allocations in RT path

### Security & Compliance
- **Vulnerability Scanning**: `cargo audit` with deny warnings
- **License Compliance**: `cargo deny` with approved license list
- **Dependency Tracking**: Third-party license report generation
- **ADR Validation**: Format and requirement reference checking

## Development Workflow

### 1. Code Standards
```bash
# Format code
cargo fmt --all

# Run lints
cargo clippy --all-targets --all-features -- -D warnings

# Run tests
cargo test --all-features --workspace

# Faster test runner (requires cargo-nextest)
cargo nextest run --all-features --workspace

# Build/test without Tauri/GTK deps (the ui crate needs them)
cargo test --all-features --workspace --exclude racing-wheel-ui
```

#### Memory Safety Rules
- **No static mut**: Use `std::sync::OnceLock` instead of `static mut` for thread-safe initialization
- **Lint Guard**: All non-test crates must include `#![deny(static_mut_refs)]` to prevent regression
- **Safe Alternatives**: Prefer `AtomicBool`, `OnceLock`, or `LazyLock` over unsafe static patterns

#### Error Handling Defaults
- **No `unwrap()`/`expect()`**: The workspace enforces a strict policy against `unwrap()` and `expect()`, **especially in tests**.
- **Result-returning tests**: Tests should map errors by returning `Result<(), ErrorType>` (or `Result<(), Box<dyn std::error::Error>>`).
- **Property-based tests**: In `proptest!` blocks (where `?` isn't always viable), use `let-else` combined with `prop_assert!()` and `unreachable!()` (e.g. `let Ok(val) = fallible() else { prop_assert!(false, "failed"); unreachable!() };`).

### 2. Performance Validation
```bash
# Build RT profile
cargo build --profile rt --bin wheeld

# Run benchmarks and generate JSON results for validation
BENCHMARK_JSON_OUTPUT=1 BENCHMARK_JSON_PATH=bench_results.json cargo bench --bench rt_timing

# Validate performance gates
python scripts/validate_performance.py bench_results.json --strict
```

### 3. Documentation
```bash
# Validate ADRs
cargo run -p openracing-tools --bin validate-adr -- --verbose

# Generate documentation index
cargo run -p openracing-tools --bin generate-docs-index --

# Build docs
cargo doc --all-features --workspace

# Regenerate workspace-hack after dependency changes
cargo hakari generate
```

## Code Coverage

The project uses [`cargo-llvm-cov`](https://github.com/taiki-e/cargo-llvm-cov) for source-based code coverage via LLVM instrumentation.

### Prerequisites

```bash
# Install the llvm-tools component
rustup component add llvm-tools-preview

# Install cargo-llvm-cov
cargo install cargo-llvm-cov
```

### Running Coverage Locally

Use the helper script:
```bash
# Text summary (printed to terminal)
./scripts/coverage.sh

# HTML report (opens in browser)
./scripts/coverage.sh --html

# JSON report (codecov format, writes codecov.json)
./scripts/coverage.sh --json

# LCOV report (writes lcov.info, for IDE integration)
./scripts/coverage.sh --lcov
```

Or run `cargo llvm-cov` directly:
```bash
cargo llvm-cov --workspace --all-features \
  --exclude racing-wheel-ui \
  --exclude racing-wheel-integration-tests \
  --ignore-filename-regex '(\.pb\.rs$|/tests/|/benches/|/fuzz/|/build\.rs$|_test\.rs$|/target/)'
```

### What Is Excluded from Coverage

| Pattern | Reason |
|---------|--------|
| `*.pb.rs` | Generated protobuf code |
| `/tests/` | Test code itself |
| `/benches/` | Benchmark harnesses |
| `/fuzz/` | Fuzz targets |
| `/build.rs` | Build scripts |
| `*_test.rs` | Test modules |
| `racing-wheel-ui` | Requires Tauri/GTK — not testable on CI |
| `racing-wheel-integration-tests` | Integration tests are not coverage subjects |

### CI Integration

The [coverage workflow](../.github/workflows/coverage.yml) runs on every push to `main` and on PRs:
1. Generates an LLVM-based coverage report
2. Uploads results to [Codecov](https://codecov.io/gh/EffortlessMetrics/OpenRacing)
3. Posts a coverage summary comment on PRs

## Real-Time Development Guidelines

### Critical Path Rules
1. **No Heap Allocations**: RT thread must not allocate after initialization
2. **No Blocking Operations**: No syscalls, locks, or I/O in RT path
3. **Bounded Execution**: All RT operations must have deterministic timing
4. **Error Handling**: Use `Result<(), RTError>` with pre-allocated error codes

### Performance Budgets
- **Total RT Budget**: 1000μs @ 1kHz
- **Input Processing**: 50μs
- **Filter Pipeline**: 200μs median, 800μs p99
- **Output Formatting**: 50μs
- **HID Write**: 100μs median, 300μs p99
- **Safety Checks**: 50μs

### Testing RT Code
```rust
#[test]
fn test_zero_alloc_rt_path() -> Result<(), Box<dyn std::error::Error>> {
    // Use allocation tracking to ensure no heap usage
    let _guard = allocation_tracker::track();
    
    let mut engine = Engine::new();
    let mut frame = Frame::default();
    
    // This must not allocate
    engine.process_frame(&mut frame)?;
    
    assert_eq!(allocation_tracker::allocations(), 0);
    Ok(())
}
```

## Single Responsibility Principle (SRP) Micro-Crates

Use micro-crates when a component has one clear reason to change and can be reused
across runtime layers.

### When to extract

- A module mixes pure protocol/data logic with runtime concerns (HID I/O, env/config, logging).
- The same parsing/normalization/encoding logic is needed in multiple crates.
- Test coverage is easier to maintain when logic is isolated from device plumbing.

### Micro-crate rules

- Keep scope narrow: one domain concern per crate (for example, one hardware protocol parser).
- Prefer pure functions and small value types over stateful services.
- Keep hot-path APIs allocation-free and deterministic.
- Avoid blocking, syscalls, and runtime I/O inside parsing/encoding functions.
- Expose stable, minimal APIs and re-export from higher-level crates only where needed.

### Non-goals

- Do not split crates only for naming or directory symmetry.
- Do not extract code that is tightly coupled to a single runtime implementation.

### Verification checklist for extractions

- Unit tests move with the extracted logic crate.
- Integration paths in consuming crates continue to pass.
- RT safety expectations remain explicit (no allocations, no blocking, bounded execution).
- Public API and dependency changes are documented in `docs/` and ADRs when architectural impact is significant.

## Safety Requirements

All safety-critical code must follow these guidelines:

### Fault Response
- **Detection Time**: ≤ 10ms
- **Response Time**: ≤ 50ms total (fault to safe state)
- **Recovery**: Automatic where safe, manual confirmation for critical faults

### Testing Requirements
- **Fault Injection**: All defined failure modes must be tested
- **Timing Validation**: Oscilloscope measurement for critical timing
- **Soak Testing**: 48+ hour continuous operation validation

## Plugin Development

### Safe Plugins (WASM)
- **Update Rate**: 60-200Hz
- **Sandboxing**: Capability-based permissions
- **Memory Limit**: Configurable per plugin
- **Crash Isolation**: Automatic restart with backoff

### Fast Plugins (Native)
- **Update Rate**: 1kHz (RT path)
- **Timing Budget**: Microsecond-level enforcement
- **ABI Versioning**: Semantic compatibility checking
- **Code Signing**: Ed25519 signatures required

## Troubleshooting

### Performance Issues
1. Check RT thread priority and affinity
2. Validate timing with `cargo bench --bench rt_timing`
3. Use system tracing (ETW/tracepoints) for detailed analysis
4. Review blackbox recordings for timing anomalies

### Build Issues
1. Ensure system dependencies are installed (libudev-dev, pkg-config)
2. Check Rust toolchain version compatibility
3. Validate cargo deny configuration for new dependencies
4. Review CI logs for platform-specific issues

### Safety System Issues
1. Check device capability negotiation
2. Validate safety state machine transitions
3. Review fault detection thresholds
4. Test physical interlock mechanisms

## Mutation Testing

Mutation testing verifies that the test suite actually catches logic errors — not just that code runs, but that tests *fail* when the code is wrong. This is especially important for safety-critical paths where an undetected mutation could allow unsafe torque or miss a fault.

### Install

```bash
cargo install cargo-mutants
```

### Run (engine safety code)

```bash
# Full mutation run for the safety-critical engine crate (slow — allow 30–60 min)
cargo mutants --package racing-wheel-engine --test-timeout 60 --jobs 4

# Convenience scripts (also exit 1 if any mutants survive)
./scripts/run_mutation_tests.sh                          # Linux / macOS
.\scripts\run_mutation_tests.ps1                         # Windows PowerShell

# Narrower run — only the safety module
cargo mutants --package racing-wheel-engine --file 'crates/engine/src/safety.rs'
```

### What to focus on

| File | Why it matters |
|------|---------------|
| `crates/engine/src/safety.rs` | `clamp_torque_nm`, `report_fault`, `max_torque_nm`, state machine transitions |
| `crates/engine/src/safety/hardware_watchdog.rs` | Watchdog feed/timeout, `SafetyInterlockSystem` |
| `crates/engine/src/safety/fault_injection.rs` | Fault trigger conditions and recovery logic |
| `crates/engine/src/policies.rs` | `SafetyPolicy` torque limits and hands-off timeout |

### CI integration

A scheduled workflow (`.github/workflows/mutation-tests.yml`) runs mutation tests weekly (Monday 03:00 UTC) and on manual dispatch. It is intentionally **not** triggered on every push because mutation tests are too slow for that.

### Interpreting results

- **Caught**: good — the test suite detected the mutation.
- **Survived**: bad — a mutation was not caught; add a test that would fail for that change.
- **Timeout**: usually means a test hung; investigate or increase `--test-timeout`.

The configuration in `mutants.toml` already excludes test-only code, serde boilerplate, and Debug/Display impls so results focus on production logic.

## Contributing

1. **Create ADR**: For architectural changes, create an ADR first
2. **Follow Standards**: Use rustfmt, clippy, and pass all CI checks
3. **Test Thoroughly**: Include unit, integration, and performance tests
4. **Document Changes**: Update relevant documentation and ADRs
5. **Performance Impact**: Validate that changes don't regress performance gates

### Renaming public constants or symbols (F-007)

When renaming a public constant, variant, or function in a protocol crate:

1. Add the `#[deprecated]` attribute to the old name **one release cycle before removal**:
   ```rust
   #[deprecated(since = "1.1.0", note = "use `frame_seq` instead")]
   pub const sequence: u64 = frame_seq;
   ```
2. Update all call sites in the same PR where possible.
3. Remove the deprecated alias in the following release.

This converts silent breakage into a clear compiler warning for downstream crates.

### Snapshot tests and cross-validation (F-006)

Snapshot tests (`insta`) confirm output stability but cannot detect "wrong but consistent" values — the first `--force-update-snapshots` bakes in whatever the code produces.

Mitigation:
- For device ID constants, add assertions in a separate `*_id_verification.rs` test file (see `crates/hid-moza-protocol/tests/id_verification.rs`) that cross-check against the golden values in `docs/protocols/SOURCES.md`.
- Add `// source: <URL>` comments inside snapshot files so reviewers can verify values against community documentation.
- When adding a new device crate, add at least one "known-good byte string" unit test (e.g. a hard-coded raw HID report and the expected decoded struct) in addition to snapshots.

## Tools and Scripts

- `scripts/validate_performance.py`: Performance gate validation
- `cargo run -p openracing-tools --bin validate-adr --`: ADR format and reference validation
- `cargo run -p openracing-tools --bin generate-docs-index --`: Documentation index generation
- `cargo run -p openracing-tools --bin yaml-sync-check --`: Game support matrix YAML sync tool (see below)
- `scripts/run_mutation_tests.sh`: Mutation testing runner (Linux/macOS)
- `scripts/run_mutation_tests.ps1`: Mutation testing runner (Windows)
- `benches/rt_timing.rs`: Real-time performance benchmarks
- `deny.toml`: Dependency and license configuration
- `clippy.toml`: Linting configuration
- `rustfmt.toml`: Code formatting configuration

### Keeping game support matrix files in sync

Two YAML files must always be identical:

- `crates/telemetry-config/src/game_support_matrix.yaml` (canonical — runtime)
- `crates/telemetry-support/src/game_support_matrix.yaml` (mirror — tests)

**Whenever you edit `crates/telemetry-config/src/game_support_matrix.yaml`, run:**

```bash
cargo run -p openracing-tools --bin yaml-sync-check -- --fix
```

This copies the canonical file to the mirror. To check without writing:

```bash
cargo run -p openracing-tools --bin yaml-sync-check -- --check   # exits 1 if files differ
```

The CI workflow (`.github/workflows/yaml-sync-check.yml`) enforces this on every push and PR.

## WSL + Nix CI Runner (Windows)

If you want to run the Linux CI-equivalent checks from Windows without moving the
repo into WSL, use the WSL wrapper script. It maps the Windows path into WSL and
executes the Nix dev shell before running the CI script.

### Prerequisites
- WSL2 with a Linux distro (e.g., Ubuntu)
- Nix installed in WSL (recommended: [Determinate Systems installer](https://install.determinate.systems/nix))
- Nix flakes enabled (for `flake.nix`)

### Usage

Run from PowerShell in the repo root:
```powershell
.\scripts\ci_wsl.ps1 -- --mode fast
.\scripts\ci_wsl.ps1 -- --mode full
```

The `--` delimiter separates PowerShell args from Linux script args.

### CI Modes

| Mode | Description |
|------|-------------|
| `fast` | Isolation builds, workspace default, lint gates, final validation |
| `full` | All phases including schema validation, feature combinations, dependency governance, performance gates, security audit, coverage |

### Common Flags

| Flag | Description |
|------|-------------|
| `--allow-dirty` | Skip clean-tree checks (useful during iteration) |
| `--skip-performance` | Skip performance gate steps |
| `--force-performance` | Run performance gates even on WSL |
| `--skip-coverage` | Skip coverage collection |
| `--skip-security` | Skip security audit and license checks |
| `--skip-minimal-versions` | Skip nightly minimal-versions check |
| `--allow-lock-update` | Allow Cargo.lock to change (otherwise fails if lockfile changes) |
| `--buf-against <ref>` | Run buf breaking checks against a git ref |

### Examples

Quick iteration (dirty tree, skip perf):
```powershell
.\scripts\ci_wsl.ps1 -- --mode fast --allow-dirty
```

Full CI with lockfile changes allowed:
```powershell
.\scripts\ci_wsl.ps1 -- --mode full --allow-lock-update
```

Select a specific WSL distro:
```powershell
$env:OPENRACING_WSL_DISTRO = "Ubuntu-22.04"
.\scripts\ci_wsl.ps1 -- --mode fast
```

Skip Nix (if already in a nix shell or debugging):
```powershell
.\scripts\ci_wsl.ps1 -NoNix -- --mode fast
```

### On Linux (or inside WSL)

Run the CI script directly:
```bash
nix develop --command bash scripts/ci_nix.sh --mode fast
# or without nix:
scripts/ci_nix.sh --mode fast
```

### Troubleshooting

- **Nix not found**: Install Nix in your WSL distro
- **Cargo.lock changed**: The CI run modified the lockfile. Re-run with `--allow-lock-update` if intentional, or revert and regenerate
- **Performance gate failures on WSL**: WSL timing is unreliable; use `--skip-performance` or `--force-performance` to run anyway
- **Path mapping failed**: Ensure the repo is accessible from both Windows and WSL
