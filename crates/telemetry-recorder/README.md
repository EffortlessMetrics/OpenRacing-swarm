# openracing-telemetry-recorder

Small, focused crate for deterministic telemetry recordings and fixture generation.

## Purpose

- Record telemetry frames to JSON fixtures.
- Load and replay recordings.
- Generate synthetic scenarios for testing.

## Usage

This crate is extracted from service telemetry internals and keeps recording,
playback, and scenario-generation concerns isolated so they can evolve independently
from adapter and runtime orchestration logic.
