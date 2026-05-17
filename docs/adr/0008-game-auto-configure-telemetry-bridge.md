# ADR-0008: Game Auto-Configure and Telemetry Bridge

**Status:** Accepted  
**Date:** 2025-01-15  
**Authors:** Architecture Team, Game Integration Team  
**Reviewers:** Engineering Team  
**Related ADRs:** ADR-0002 (IPC Transport), ADR-0005 (Plugin Architecture)

## Context

Prior to this change, connecting OpenRacing to a supported sim title required two manual steps from the user:

1. Run `openracing config apply <game>` to write the UDP/shared-memory telemetry configuration to the game's install directory.
2. Separately start the matching telemetry adapter (via CLI or service management) once the game was launched.

Neither step was triggered automatically when a game process was detected. This violated GI-01 (one-click telemetry) and created friction that prevented a true plug-and-play experience. Users who skipped either step would see missing telemetry data or silent failures with no recovery path.

Two gaps existed in the `AutoProfileSwitcher` pipeline:

- **Config gap**: game install directories were never written unless the user explicitly invoked the CLI.
- **Adapter gap**: telemetry adapters were never started or stopped in response to game lifecycle events.

## Decision

### `GameAutoConfigurer` — first-detection config write

`GameAutoConfigurer` wraps `GameService::configure_telemetry` and adds idempotency:

- On each `on_game_detected(game_id)` call, the configurer checks a persistent store at `~/.openracing/configured_games.json`.
- If the game has not been configured before, it resolves the install directory (see below) and calls `GameService::configure_telemetry`, which writes the UDP/shared-memory config file into the game install directory.
- On success the game ID is inserted into the store and the store is flushed to disk, preventing re-runs on subsequent detections.
- All errors (missing install path, failed config write, failed store flush) are logged as warnings and do not propagate — the game is not marked as configured so the next detection will retry.

**Install path resolution order:**

1. Explicit override via `with_install_path_override(path)` (used in tests).
2. Windows registry keys listed in `auto_detect.install_registry_keys` from the support matrix (Windows only).
3. Relative install paths from `auto_detect.install_paths` checked under platform-appropriate filesystem roots (drive roots on Windows; Steam common paths on Linux/macOS).

### `GameTelemetryBridge` — adapter lifecycle management

`GameTelemetryBridge` implements the `TelemetryAdapterControl` trait, providing two operations:

- `start_for_game(game_id)`: calls `TelemetryService::start_monitoring` and retains the returned `TelemetryReceiver` in an internal map. Keeping the receiver alive keeps the adapter's background task running. Games without a registered adapter produce a non-fatal warning.
- `stop_for_game(game_id)`: removes the receiver from the map (closing the channel and signalling the background task to exit) then calls `TelemetryService::stop_monitoring`.

The `TelemetryAdapterControl` trait is the testability seam — unit tests supply a `MockControl` that records calls without requiring a real `TelemetryService`.

### Wiring into `AutoProfileSwitcher`

Both components are attached via a builder pattern on `AutoProfileSwitcher`:

```rust
let switcher = AutoProfileSwitcher::new(profile_service, process_detection)
    .await?
    .with_game_auto_configurer(Arc::new(GameAutoConfigurer::new(game_service)))
    .with_adapter_control(Arc::new(GameTelemetryBridge::new(telemetry_service)));
```

When a `ProcessEvent::GameStarted` event is received, the switcher:

1. Calls `auto_configurer.on_game_detected(game_id)` (config write, idempotent).
2. Calls `adapter_control.start_for_game(game_id)` (adapter start).
3. Switches the active profile (existing GI-02 behaviour, ≤500 ms).

When a `ProcessEvent::GameStopped` event is received, the switcher calls `adapter_control.stop_for_game(game_id)`.

## Rationale

- **Minimal user action**: GI-01 requires automatic telemetry config on game detection; `GameAutoConfigurer` fulfils this without any CLI invocation.
- **Idempotency over correctness guarantees**: persisting a marker file is simpler and cheaper than computing a content hash of the written files. A re-write would be safe but unnecessary; the marker prevents redundant filesystem operations on every game start.
- **Trait seam for testability**: `TelemetryAdapterControl` decouples `AutoProfileSwitcher` from `TelemetryService`, enabling unit tests with a lightweight mock.
- **Builder opt-in**: keeping both components optional in the builder preserves backward compatibility for callers that do not need auto-configuration or adapter management.

## Consequences

### Positive

- True plug-and-play: game detection triggers both the config write and telemetry adapter start with no user intervention.
- Idempotent config write prevents redundant file I/O and avoids overwriting user edits on every game launch.
- Telemetry adapters are cleanly stopped when the game process exits, avoiding leaked background tasks.
- `TelemetryAdapterControl` trait enables unit testing of adapter lifecycle without a real telemetry service.

### Negative

- Windows registry lookup (`winreg` crate) adds a platform-specific code path that must be kept in sync with game installer behaviour and has no automated test coverage against real registries.
- Auto-configuration requires an async runtime; callers that previously used `AutoProfileSwitcher` in a sync context must provide one.
- The `configured_games.json` marker file is a write-once record; if the config written to disk is later corrupted or deleted, the user must manually delete the marker file to trigger a re-configuration.

### Neutral

- Config write failures are silent warnings rather than hard errors, which trades observability for resilience.
- Games without a registered telemetry adapter produce a log warning on every start event.

## Alternatives Considered

1. **Manual-only setup (prior state)**: Rejected because it violates GI-01 and creates friction that prevents plug-and-play operation. Users regularly missed one or both setup steps.
2. **Always re-write config on every game start**: Rejected because it unconditionally overwrites the config file on each launch, discarding any user edits made after the initial write. The idempotent marker approach avoids this at the cost of a second deletion step if re-configuration is needed.
3. **Hash-based idempotency**: Re-write only when the on-disk content differs from the generated config. Rejected as over-engineered for the current requirement; the marker-file approach is sufficient and easier to reason about.

## Implementation Notes

**`ConfiguredGamesStore` schema** (`~/.openracing/configured_games.json`):

```json
{
  "configured": ["iracing", "assetto_corsa_competizione"]
}
```

**Re-triggering auto-configuration** (user-facing recovery):

Remove the game entry from `~/.openracing/configured_games.json` (or delete the file) and restart the game. The next detection will treat it as a first-time configuration.

**Locking discipline**: `GameAutoConfigurer` holds a `tokio::sync::Mutex` over `ConfiguredGamesStore`. The store lock is always released before calling `GameService::configure_telemetry` to avoid holding it across async I/O.

**`GameTelemetryBridge` receiver map**: the map is keyed by game ID. If a game starts twice without a stop event in between (e.g. crash restart), the old receiver is replaced, which drops the previous channel and signals the old background task to exit before the new one is inserted.

## Compliance & Verification

- `game_auto_configure` unit tests: first-detection writes config and persists marker; subsequent detection skips re-write; missing install path returns gracefully without marking game as configured.
- `game_telemetry_bridge` unit tests: `MockControl` verifies start/stop call sequences via recorded game ID lists.
- Integration: `AutoProfileSwitcher` tests attach a `MockControl` via `with_adapter_control` and verify that `start_for_game` / `stop_for_game` are called in response to process events.
- CI gate: `cargo test --all-features --workspace` must pass with all above tests included.

## References

- Requirements: GI-01 (one-click telemetry), GI-02 (auto profile switch), GI-03 (normalized telemetry), GI-04 (loss handling), XPLAT-01 (I/O stacks), XPLAT-03 (install and perms)
- Implementation: `crates/service/src/game_auto_configure.rs`
- Implementation: `crates/service/src/game_telemetry_bridge.rs`
- Integration point: `crates/service/src/auto_profile_switching.rs`
- Support matrix: `openracing-telemetry-config` crate (`load_default_matrix`, `normalize_game_id`)
- Related ADRs: ADR-0002 (IPC Transport), ADR-0005 (Plugin Architecture)
