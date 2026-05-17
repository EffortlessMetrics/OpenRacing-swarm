# Telemetry Crate Consolidation Migration Guide

## Summary

This guide documents the telemetry package transition into four durable
OpenRacing crates, with legacy helper and game-leaf crates kept only as
compatibility wrappers during the migration.

### Before (10 crates)
1. `telemetry-core` - GameTelemetry, GameTelemetryAdapter trait
2. `telemetry-contracts` - NormalizedTelemetry, TelemetryFlags
3. `telemetry-adapters` - Game-specific implementations
4. `telemetry-orchestrator` - Coordination layer
5. `telemetry-integration` - Integration tests as crate
6. `telemetry-recorder` - Recording/playback
7. `telemetry-rate-limiter` - Rate limiting
8. `telemetry-bdd-metrics` - BDD metrics
9. `telemetry-support` - Utilities
10. `telemetry-config-writers` - Config file writers

### After (4 crates)
1. **`openracing-telemetry`** - Core types, adapter trait, contracts, rate limiting, BDD metrics, integration utilities, orchestrator
2. **`openracing-telemetry-adapters`** - Game-specific implementations under `games::*`
3. **`openracing-telemetry-recorder`** - Recording/playback
4. **`openracing-telemetry-config`** - Config writers, game support matrix, utilities

## Migration Paths

### From `telemetry-contracts`

**Old:**
```rust
use racing_wheel_telemetry_contracts::{
    NormalizedTelemetry, TelemetryFlags, TelemetryFrame, TelemetryValue,
};
```

**New:**
```rust
use openracing_telemetry::{
    NormalizedTelemetry, TelemetryFlags, TelemetryFrame, TelemetryValue,
};
```

### From `telemetry-rate-limiter`

**Old:**
```rust
use racing_wheel_telemetry_rate_limiter::{
    RateLimiter, RateLimiterStats, AdaptiveRateLimiter,
};
```

**New:**
```rust
use openracing_telemetry::{
    RateLimiter, RateLimiterStats, AdaptiveRateLimiter,
};
```

### From `telemetry-bdd-metrics`

**Old:**
```rust
use racing_wheel_telemetry_bdd_metrics::{
    BddMatrixMetrics, MatrixParityPolicy, RuntimeBddMatrixMetrics,
};
```

**New:**
```rust
use openracing_telemetry::{
    BddMatrixMetrics, MatrixParityPolicy, RuntimeBddMatrixMetrics,
};
```

### From `telemetry-integration`

**Old:**
```rust
use racing_wheel_telemetry_integration::{
    compare_matrix_and_registry, CoveragePolicy, RegistryCoverage,
    RuntimeCoverageReport,
};
```

**New:**
```rust
use openracing_telemetry::{
    compare_matrix_and_registry, CoveragePolicy, RegistryCoverage,
    RuntimeCoverageReport,
};
```

### From `telemetry-orchestrator`

**Old:**
```rust
use racing_wheel_telemetry_orchestrator::TelemetryService;
```

**New:**
```rust
use openracing_telemetry::TelemetryService;
```

### From `telemetry-support`

**Old:**
```rust
use racing_wheel_telemetry_support::{
    load_default_matrix, matrix_game_ids, normalize_game_id,
    GameSupportMatrix, GameSupport,
};
```

**New:**
```rust
use openracing_telemetry_config::support::{
    load_default_matrix, matrix_game_ids, normalize_game_id,
    GameSupportMatrix, GameSupport,
};
// Or directly:
use openracing_telemetry_config::{
    load_default_matrix, matrix_game_ids, normalize_game_id,
    GameSupportMatrix, GameSupport,
};
```

### From `telemetry-config-writers`

**Old:**
```rust
use racing_wheel_telemetry_config_writers::{
    config_writer_factories, ConfigWriter, TelemetryConfig,
    IRacingConfigWriter, ACCConfigWriter,
};
```

**New:**
```rust
use openracing_telemetry_config::{
    config_writer_factories, ConfigWriter, TelemetryConfig,
    IRacingConfigWriter, ACCConfigWriter,
};
```

### From game leaf crates

Game leaf crates such as `racing-wheel-telemetry-lfs`,
`racing-wheel-telemetry-f1`, and `racing-wheel-telemetry-raceroom` are
compatibility wrappers. New code should import game adapters from
`openracing_telemetry_adapters::games`.

**Old:**
```rust
use racing_wheel_telemetry_lfs::LFSAdapter;
use racing_wheel_telemetry_f1::F1NativeAdapter;
use racing_wheel_telemetry_raceroom::RaceRoomAdapter;
```

**New:**
```rust
use openracing_telemetry_adapters::games::f1::F1NativeAdapter;
use openracing_telemetry_adapters::games::live_for_speed::LFSAdapter;
use openracing_telemetry_adapters::games::raceroom::RaceRoomAdapter;
```

## Cargo.toml Updates

### Old Dependencies
```toml
[dependencies]
racing-wheel-telemetry-contracts = { path = "../telemetry-contracts" }
racing-wheel-telemetry-rate-limiter = { path = "../telemetry-rate-limiter" }
racing-wheel-telemetry-bdd-metrics = { path = "../telemetry-bdd-metrics" }
racing-wheel-telemetry-support = { path = "../telemetry-support" }
racing-wheel-telemetry-config-writers = { path = "../telemetry-config-writers" }
racing-wheel-telemetry-integration = { path = "../telemetry-integration" }
racing-wheel-telemetry-orchestrator = { path = "../telemetry-orchestrator" }
```

### New Dependencies
```toml
[dependencies]
openracing-telemetry = { path = "../telemetry-core" }
openracing-telemetry-config = { path = "../telemetry-config" }
openracing-telemetry-adapters = { path = "../telemetry-adapters" }
openracing-telemetry-recorder = { path = "../telemetry-recorder" }
```

## Deprecated Crates

The following crates are now **deprecated** and will be removed in a future release:

| Old Crate | Replacement |
|-----------|-------------|
| `telemetry-contracts` | `telemetry-core` |
| `telemetry-rate-limiter` | `telemetry-core` |
| `telemetry-bdd-metrics` | `telemetry-core` |
| `telemetry-integration` | `telemetry-core` |
| `telemetry-orchestrator` | `telemetry-core` |
| `telemetry-support` | `telemetry-config` |
| `telemetry-config-writers` | `telemetry-config` |

The game leaf crates are also deprecated as public packages. Their replacement
paths are under `openracing-telemetry-adapters::games::*`, for example
`games::live_for_speed`, `games::f1`, `games::forza`, `games::ams2`,
`games::simhub`, `games::mudrunner`, `games::rennsport`,
`games::wrc_generations`, `games::kartkraft`, and `games::raceroom`.

## Benefits of Consolidation

1. **Simplified Dependencies**: Fewer crates to manage
2. **Clearer Boundaries**: Each crate has a focused purpose
3. **Reduced Build Time**: Fewer crate compilations
4. **Better Organization**: Related functionality grouped together
5. **Easier Maintenance**: Less context switching between crates

## Timeline

- **Phase 1**: New consolidated crates available (current)
- **Phase 2**: Deprecation warnings added to old crates
- **Phase 3**: Old crates removed (future release)
