# Telemetry Config

Game support matrix, utilities, and configuration writers for OpenRacing telemetry.

## Overview

This crate combines:
- **Game Support Matrix**: Metadata about supported games and their telemetry capabilities
- **Configuration Writers**: Game-specific configuration file writers

## Features

- Load and parse the game support matrix from YAML
- Normalize game IDs (including historical aliases)
- Write game-specific telemetry configuration files
- Validate configuration changes

## Usage

```rust
use openracing_telemetry_config::{
    load_default_matrix, matrix_game_ids,
    config_writer_factories, ConfigWriter, TelemetryConfig,
};

// Load the support matrix
let matrix = load_default_matrix()?;
let game_ids = matrix_game_ids()?;

// Write configuration for a game
let config = TelemetryConfig {
    enabled: true,
    update_rate_hz: 60,
    output_method: "shared_memory".to_string(),
    output_target: "127.0.0.1:12345".to_string(),
    fields: vec!["ffb_scalar".to_string()],
    enable_high_rate_iracing_360hz: false,
};

for (game_id, factory) in config_writer_factories() {
    let writer = factory();
    // Use writer to configure games
}
```

## Supported Games

- iRacing
- Assetto Corsa Competizione (ACC)
- Assetto Corsa Rally
- Automobilista 2 (AMS2)
- rFactor 2
- EA WRC
- F1 series (Codemasters)
- F1 25 (Native UDP)
- Dirt 5
