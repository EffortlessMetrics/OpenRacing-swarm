# Moza Native Visible Lane Plan

This plan sequences the Moza R5 lane from `native_response_ready` to
`native_visible_ready` without treating source-of-truth docs, dry-runs, passive
sniffing, Pit House, SimHub, simulator telemetry, or simulator FFB as native
motion proof.

Start with [implementation-plan.md](implementation-plan.md).

When the active goal has no ready work item, use [handoff.md](handoff.md) as
the blocked-state handoff instead of inventing new lane work.
