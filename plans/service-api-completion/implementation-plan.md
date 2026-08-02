# Service API completion implementation plan

Status: active
Owner: platform/service
Linked proposal: docs/proposals/OR-PROP-0005-service-api-completion.md
Linked specs:
- docs/specs/OR-SPEC-0006-service-api-completion.md
Linked ADRs: n/a
Active goal: .openracing/goals/active.toml

## Work item: expose-application-services

Status: ready
Linked issue: https://github.com/EffortlessMetrics/OpenRacing-swarm/issues/258
Linked spec: docs/specs/OR-SPEC-0006-service-api-completion.md
Target seams: `crates/service/src/service.rs`, `crates/service/src/daemon.rs`, and focused service tests.

### Goal

Make the application service composition reusable by exposing the existing game
and plugin services from `WheelService` and reusing the game service in the
daemon's gRPC layer.

### Production delta

- Store one `Arc<GameService>` and one `Arc<PluginRegistryServiceImpl>` in
  `WheelService`.
- Add `game_service()` and `plugin_service()` accessors.
- Replace the daemon-local `GameService::new()` with the wheel-service-owned
  instance.
- Add focused observable tests for supported-game lookup and an offline plugin
  registry query.

### Acceptance

- Service construction remains fallible with useful context.
- Accessors return stable shared references.
- The daemon does not construct a duplicate game service.
- Focused tests do not require hardware or a live plugin registry.
- No IPC, RT, hardware/output, plugin execution, or support claim changes.

### Proof commands

```text
python scripts/cargo_fmt_workspace.py
cargo test --locked -p racing-wheel-service service::tests -- --nocapture
cargo test --locked -p racing-wheel-service integration_tests::tests::test_game_service_accessor -- --nocapture
cargo test --locked -p racing-wheel-service integration_tests::tests::test_plugin_service_accessor -- --nocapture
cargo clippy --locked -p racing-wheel-service --all-targets --all-features -- -D warnings
cargo run --locked -p openracing-tools --bin package-surface -- --check
python scripts/policy_file.py --strict
git diff --check
```

### Non-goals

- Do not invent or re-enable placeholder APIs such as `detect_games()` or
  `start_telemetry_monitoring()`.
- Do not add plugin execution or registry network behavior.
- Do not modify protobuf, IPC, engine, or hardware code.

### Rollback

Revert the service ownership/accessor and daemon-reuse changes. Existing
separate daemon composition remains the fallback.
