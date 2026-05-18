# NOW / NEXT / LATER

One-screen execution plan for OpenRacing. Updated each sprint.

**Project snapshot:** 85 crates · 30,461+ tests · 509 proptests · 117 fuzz targets · 28 vendors · 61 games

**First hardware target:** [Moza R5 + KS + ES + SR-P + HBP](hardware/moza-r5-validation.md) (receipt-backed lane)

---

## NOW (Active — this sprint)

- **Moza native-visible frontier** - the R5 lane is `native_response_ready`; two 1 degree controlled-angle attempts failed safely in the same ~0.181277 degree response band, `native-controlled-angle-attempt-03-preflight.json` records a no-output `bounded-pidff-effect-lifecycle-v1` dry-run, and no output is authorized
- **Moza source-of-truth activation** - use `.openracing/goals/active.toml`, `docs/proposals/OR-PROP-0001-moza-native-visible-lane.md`, `docs/specs/OR-SPEC-0001-moza-native-visible-lane.md`, and `plans/moza-native-visible-lane/implementation-plan.md` as the current lane rail
- **Moza no-output operator navigation** - use `wheelctl moza artifact-index`, `wheelctl moza bench-wizard`, and `wheelctl moza verify-bundle --stage native-visible-ready` to inspect the blocked frontier; do not treat dry-runs, sniffing, Pit House, SimHub, or simulator evidence as native-visible proof
- **Service API completion** — implement `WheelService::game_service()` and `plugin_service()` accessors; re-enable blocked integration tests

## NEXT (Queued — next 2–4 sprints)

- **Moza attempt-03 authorization** - only after fresh command-bound bench-clear, create one exact authorization for target 1 degree, max 5%, timeout 2000 ms, strategy `pidff-bounded-effect`, profile `bounded-pidff-effect-lifecycle-v1`, preserving all prior receipts
- **Moza attempt-03 output** - run exactly one authorized controlled-angle attempt, preserve the receipt, and either promote `native_visible_ready` if the verifier passes or stop and analyze if it fails
- **Moza Pit House coexistence** - external compatibility only; test closed/open/mode-change/update-page cases separately and do not make Pit House a native-control prerequisite
- **One simulator telemetry path** - telemetry-only first, no FFB writes, before bounded sim-to-Moza FFB smoke
- **Mutation testing expansion** — extend `cargo-mutants` to protocol encoding and telemetry paths
- **macOS IOKit HID driver** — start actual device I/O on macOS

## LATER (Backlog — future work)

- **Moza controlled movement ladder** - after `native_visible_ready`, continue one authorized rung at a time: 1 degree repeat, 3, 5, 10, 30, 90 right, and 90 return
- **Moza real-hardware smoke ready** - only after native-visible, controlled movement confidence, Pit House coexistence, simulator telemetry, bounded simulator FFB, support bundle, manifest promotion, and lane audit receipts pass
- **Extended Validation & Soak** — 1hr continuous bounded FFB, disconnect/reconnect stress, V1 vs V2 firmware, Standard vs Direct FFB comparison
- **Phase 12: Multi-Vendor Verification** — Fanatec, Logitech, Thrustmaster HIL; protocol research; 48hr soak; community capture program
- **Cloud integration** — profile sharing and cross-machine sync
- **Telemetry dashboard** — browser-based replay visualization and session comparison
- **AI/ML integration** — adaptive FFB tuning from driving style analysis
- **Plugin marketplace** — searchable catalog with community submissions
- **VR / motion rig integration** — haptic feedback via OpenXR
- **Mobile companion app** (iOS/Android)
- **Accessibility** — screen reader support, high-contrast mode
- **Localization** — multi-language UI and docs

---

*Source: [ROADMAP.md](../ROADMAP.md) · [FRICTION_LOG.md](FRICTION_LOG.md) · [RC_READINESS.md](RC_READINESS.md)*
