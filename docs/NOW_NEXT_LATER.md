# NOW / NEXT / LATER

One-screen execution plan for OpenRacing. Updated each sprint.

**Project snapshot:** 85 crates · 30,461+ tests · 509 proptests · 117 fuzz targets · 28 vendors · 61 games

**First hardware target:** [Moza R5 + KS + ES + SR-P + HBP](hardware/moza-r5-validation.md) (receipt-backed lane)

---

## NOW (Active — this sprint)

- **Moza native-visible frontier** - the R5 lane is `native_response_ready`; three 1 degree controlled-angle attempts, including `bounded-pidff-effect-lifecycle-v1` attempt 03, failed safely in the same ~0.181277 degree response band, and `native-pidff-standard-path-diagnosis.json` records that the standard PIDFF path is ineffective in the current R5 mode
- **Moza source-of-truth activation** - use `.openracing/goals/active.toml`, `docs/proposals/OR-PROP-0001-moza-native-visible-lane.md`, `docs/specs/OR-SPEC-0001-moza-native-visible-lane.md`, and `plans/moza-native-visible-lane/implementation-plan.md` as the current lane rail
- **Moza no-output operator navigation** - use `wheelctl moza artifact-index`, `wheelctl moza bench-wizard`, and `wheelctl moza verify-bundle --stage native-visible-ready` to inspect the blocked frontier; the wizard now points from the completed standard-PIDFF diagnosis to command-bound passive sniff receipt/summary handoff for `pit-house-open-idle`, but still creates no authorization, output permission, or readiness claim
- **Service API completion** — implement `WheelService::game_service()` and `plugin_service()` accessors; re-enable blocked integration tests

## NEXT (Queued — next 2–4 sprints)

- **Moza vendor-specific control investigation** - no-output sniff plans are recorded for Pit House, SimHub, and simulator sessions; next capture those scenarios, generate non-claiming receipts/summaries, decode vendor reports, and only then design a reviewed vendor-control plan
- **Moza Pit House coexistence** - external compatibility only; test closed/open/mode-change/update-page cases separately and do not make Pit House a native-control prerequisite
- **Moza passive USB sniff support evidence** - optional protocol research for Pit House, SimHub, and simulator traffic; summary-only by default, no raw pcapng unless reviewed, and never a native or smoke-ready gate
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
