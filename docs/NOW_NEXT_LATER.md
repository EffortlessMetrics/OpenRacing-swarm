# NOW / NEXT / LATER

One-screen execution plan for OpenRacing. Updated each sprint.

**Project snapshot:** 85 crates · 30,461+ tests · 509 proptests · 117 fuzz targets · 28 vendors · 61 games

**First hardware target:** [Moza R5 + KS + ES + SR-P + HBP](hardware/moza-r5-validation.md) (receipt-backed lane)

---

## NOW (Active — this sprint)

- **Moza passive lane** - plug in R5, KS, SR-P, HBP; run `wheelctl moza probe`, `hid-capture list`, descriptor capture, and passive input captures; verify with `wheelctl moza verify-bundle --stage passive`
- **Moza parser fixtures** - promote real captures only after aggregate `wheelctl moza validate-captures`; keep raw HID path and serial data out of fixtures
- **Service API completion** — implement `WheelService::game_service()` and `plugin_service()` accessors; re-enable blocked integration tests

## NEXT (Queued — next 2–4 sprints)

- **Moza zero-torque proof** - send only report `0x20` zero payloads, require final zero, watchdog, and disconnect receipts; verify with `wheelctl moza verify-bundle --stage zero`
- **Moza gated low-torque output** - only after a passing real zero proof and `--confirm-low-torque`; cap the first ladder at 2%, keep high torque disabled, and record final zero
- **Moza Pit House coexistence** - test Pit House closed/open/mode-change/update-page cases separately before direct-mode smoke
- **One simulator telemetry path** - validate one real game or SimHub bridge before bounded sim-to-Moza FFB smoke
- **Mutation testing expansion** — extend `cargo-mutants` to protocol encoding and telemetry paths
- **macOS IOKit HID driver** — start actual device I/O on macOS

## LATER (Backlog — future work)

- **Moza real-hardware smoke ready** - only after passive, zero, low-torque, Pit House, watchdog, disconnect, simulator telemetry, and bounded simulator FFB receipts pass
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
