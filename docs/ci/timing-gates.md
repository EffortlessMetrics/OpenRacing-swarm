# Timing Gates

OpenRacing has two kinds of timing checks:

- correctness gates that should run on ordinary shared CI
- timing gates that require real-time-ish scheduling and should run locally or
  in a dedicated performance lab

The distinction is intentional. GitHub-hosted runners can pause, oversubscribe,
throttle, or migrate processes in ways that are unrelated to OpenRacing's
real-time behavior. Hosted-runner jitter should not block unrelated correctness
PRs, but timing guarantees still need a strict lane before release or hardware
promotion.

## Shared CI Policy

Shared CI may set:

```text
OPENRACING_SKIP_TIMING_GUARANTEES=1
```

Coverage jobs also skip timing-sensitive assertions because instrumentation
changes execution cost. Some tests additionally skip when `GITHUB_ACTIONS` is
set, because hosted runners are not a controlled scheduler environment.

This skip policy does not mean the timing behavior is optional. It means the
shared CI lane is not authoritative for sub-millisecond scheduling claims.

## Timing-Sensitive Test Surface

Current timing-sensitive tests include:

| Area | File | Timing Claim |
| --- | --- | --- |
| Engine jitter isolation | `crates/engine/tests/ffb_jitter_isolation_tests.rs` | FFB jitter and isolation timing |
| Engine zero-allocation integration | `crates/engine/tests/zero_alloc_integration.rs` | RT-path allocation and timing coupling |
| Engine virtual device integration | `crates/engine/tests/virtual_device_integration.rs` | virtual HID latency and scheduling behavior |
| Integration workflow gates | `crates/integration-tests/tests/integration_tests.rs` | fault recovery, FFB jitter, HID latency, missed ticks, stress, benchmark timing |
| Plug-and-play iRacing path | `crates/integration-tests/tests/plug_and_play.rs` | strict iRacing normalization latency |
| Performance gate tests | `crates/integration-tests/tests/performance_gate_tests.rs` | jitter variance gate |
| Telemetry protocol deep tests | `crates/telemetry-adapters/tests/protocol_deep_tests.rs` | parser timing guarantees |

The workflows that deliberately skip timing assertions in shared contexts are:

- `.github/workflows/integration-tests.yml`
- `.github/workflows/coverage.yml`
- `.github/workflows/ci.yml` coverage lane

## Local Strict Run

Run strict timing checks on an idle machine with power management disabled or
set to a performance profile.

PowerShell:

```powershell
Remove-Item Env:\OPENRACING_SKIP_TIMING_GUARANTEES -ErrorAction SilentlyContinue
Remove-Item Env:\LLVM_PROFILE_FILE -ErrorAction SilentlyContinue

cargo test -p racing-wheel-engine --test ffb_jitter_isolation_tests -- --nocapture
cargo test -p racing-wheel-engine --test zero_alloc_integration -- --nocapture
cargo test -p racing-wheel-engine --test virtual_device_integration -- --nocapture
cargo test -p racing-wheel-integration-tests --test integration_tests --features ci-gates -- --nocapture
cargo test -p racing-wheel-integration-tests --test performance_gate_tests --features ci-gates -- --nocapture
cargo test -p racing-wheel-integration-tests --test plug_and_play --features ci-gates -- --nocapture
cargo test -p openracing-telemetry-adapters --test protocol_deep_tests -- --nocapture
```

Linux/macOS shell:

```bash
unset OPENRACING_SKIP_TIMING_GUARANTEES
unset LLVM_PROFILE_FILE

cargo test -p racing-wheel-engine --test ffb_jitter_isolation_tests -- --nocapture
cargo test -p racing-wheel-engine --test zero_alloc_integration -- --nocapture
cargo test -p racing-wheel-engine --test virtual_device_integration -- --nocapture
cargo test -p racing-wheel-integration-tests --test integration_tests --features ci-gates -- --nocapture
cargo test -p racing-wheel-integration-tests --test performance_gate_tests --features ci-gates -- --nocapture
cargo test -p racing-wheel-integration-tests --test plug_and_play --features ci-gates -- --nocapture
cargo test -p openracing-telemetry-adapters --test protocol_deep_tests -- --nocapture
```

If the process is running under GitHub-hosted Actions, tests that key off
`GITHUB_ACTIONS` may still skip. Use a local machine, self-hosted runner, or
dedicated perf-lab workflow for strict timing evidence.

## Benchmark Gate

For the RT benchmark gate:

```powershell
$env:BENCHMARK_JSON_OUTPUT = "1"
$env:BENCHMARK_JSON_PATH = "bench_results.json"
cargo bench --bench rt_timing
python scripts/validate_performance.py bench_results.json --strict
```

The benchmark should be collected on an idle performance-configured machine. A
shared hosted runner can provide trend signal, but it is not a release-quality
timing receipt.

## Nightly Or Perf-Lab Workflow

A dedicated timing workflow should:

- run on a self-hosted or otherwise controlled machine
- leave `OPENRACING_SKIP_TIMING_GUARANTEES` unset
- avoid coverage instrumentation
- set CPU governor or Windows power plan to performance
- record OS, CPU model, power profile, and runner identity
- upload raw test logs and `bench_results.json`
- fail on strict timing regressions

Suggested workflow trigger:

```yaml
on:
  workflow_dispatch:
  schedule:
    - cron: "0 7 * * *"
```

The shared PR matrix remains responsible for correctness, parser behavior,
schema compatibility, dependency policy, and build coverage. The timing workflow
is responsible for sub-millisecond scheduling claims.
