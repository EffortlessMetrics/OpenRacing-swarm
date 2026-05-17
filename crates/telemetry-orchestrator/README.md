# racing-wheel-telemetry-orchestrator

Runtime orchestration and adapter management for telemetry integrations.

## Purpose

- Owns matrix-driven adapter registration for telemetry sources.
- Resolves game identifier aliases and exposes a stable service façade.
- Coordinates optional recording for telemetry fixture generation.
- Reuses shared telemetry contracts and core domain types.

## Key API

- `TelemetryService` – facade used by service runtime and higher layers.
- Adapter registration is derived from `racing-wheel-telemetry-support` matrix entries.
- The actual constructor registry is sourced from `openracing-telemetry-adapters` via
  `adapter_factories()`.
- `TelemetryService::runtime_coverage_report()` exposes startup matrix/registry parity details.
- `TelemetryService::runtime_bdd_metrics()` exposes policy-aware BDD counters/ratios with `parity_ok`.

## Design notes

- This crate intentionally keeps orchestration concerns out of the main service crate
  to support SRP-compliant incremental extraction.
- Matrix source-of-truth behavior is preserved from existing service behavior.
