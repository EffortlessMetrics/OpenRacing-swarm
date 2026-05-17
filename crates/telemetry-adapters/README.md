# openracing-telemetry-adapters

Game-specific telemetry adapter implementations for OpenRacing.

## Scope

- Protocol adapters:
  - ACC UDP broadcast (`ACCAdapter`)
  - Assetto Corsa Rally (`ACRallyAdapter`)
  - AMS2 shared memory (`AMS2Adapter`)
  - Dirt 5 Codemasters-UDP bridge (`Dirt5Adapter`)
  - EA WRC schema-driven UDP (`EAWRCAdapter`)
  - F1 Codemasters-UDP bridge (`F1Adapter`)
  - iRacing shared memory (`IRacingAdapter`)
  - rFactor 2 shared memory (`RFactor2Adapter`)

- Shared protocol helpers:
  - Codemasters custom UDP decoding (`CustomUdpSpec`, `DecodedCodemastersPacket`)

- Test-facing mock adapter:
  - `MockAdapter`

## Design intent

This crate is a focused microcrate for the adapter layer, decoupling IO/parsing
strategies from service orchestration. The `racing_wheel_service` crate re-exports
this crate under `telemetry::adapters` for compatibility.

## Registry

- `adapter_factories()` returns the canonical `&'static` registry of supported
  game ID -> constructor pairs.
- `AdapterFactory` is the constructor function pointer type for telemetry adapters.

Use this registry whenever you need matrix-driven adapter provisioning.
