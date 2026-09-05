# Service API completion proposal

Status: completed
Owner: platform/service
Created: 2026-08-02
Completed: 2026-08-03
Target milestone: service usability
Linked specs:
- docs/specs/OR-SPEC-0006-service-api-completion.md
Linked ADRs: n/a
Linked plan: plans/service-api-completion/implementation-plan.md
Support/status impact: internal service composition only; no new hardware or support claim
Policy impact: n/a

## Why

`WheelService` is the application service owner used by the daemon and service
integration tests, but it exposes only profile, device, and safety services.
Game and plugin services are constructed separately or remain inaccessible,
which makes the application composition difficult to reuse and leaves
service-level tests with ignored placeholder coverage.

## Direction

Own one shared game service and one shared plugin registry service in
`WheelService`, activate the existing plugin-registry modules as part of the
service crate, expose both services through read-only `Arc` accessors, and have
the daemon reuse the game-service instance. This keeps service composition
explicit without changing the IPC contract or adding plugin execution behavior.

## Non-goals

- No new game detection or telemetry-monitoring API.
- No plugin loading, execution, installation, or registry protocol change.
- No IPC, RT, hardware, FFB, or support-tier change.

## Risks and rollback

Service construction gains two fallible initializers and one additional shared
owner. Revert the ownership/accessor and daemon-reuse changes to restore the
current composition.
