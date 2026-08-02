# Service API completion specification

Status: proposed
Owner: platform/service
Created: 2026-08-02
Linked proposal: docs/proposals/OR-PROP-0005-service-api-completion.md
Linked ADRs: n/a
Linked plan: plans/service-api-completion/implementation-plan.md
Linked issues:
- https://github.com/EffortlessMetrics/OpenRacing-swarm/issues/258
Linked PRs: n/a
Support-tier impact: internal service composition only; no public hardware/support promotion
Policy impact: n/a

## Contract

`WheelService` owns exactly one shared `GameService` and one shared
`PluginRegistryServiceImpl` for its lifetime. The accessors return stable
`Arc` references and do not create a second service or perform a network
operation.

The daemon's gRPC composition reuses `WheelService::game_service()`. Existing
IPC behavior remains unchanged.

## Observable requirements

- A virtual `WheelService` can report the supported-game list through
  `game_service()`.
- A virtual `WheelService` can query the plugin registry through
  `plugin_service()` without requiring a registry network request.
- Service construction failures remain fallible and contextual.
- The accessors do not add work to the 1 kHz engine/FFB path.

## Explicit non-claims

This contract does not add game detection, telemetry monitoring, plugin
execution, plugin installation, network availability, hardware compatibility,
FFB behavior, or support-tier evidence.

## Proof

Focused service tests, service Clippy, package-surface validation, policy
validation, and `git diff --check` are required for the implementation PR.
