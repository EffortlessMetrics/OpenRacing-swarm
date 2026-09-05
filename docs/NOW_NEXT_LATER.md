# NOW / NEXT / LATER

One-screen execution plan for OpenRacing. Updated each sprint.

**Project snapshot:** 85 crates · 30,461+ tests · 509 proptests · 117 fuzz targets · 28 vendors · 61 games

**First hardware target:** [Moza R5 + KS + ES + SR-P + HBP](hardware/moza-r5-validation.md) (receipt-backed lane)

---

## NOW (Queue convergence — this sprint)

- **Existing PR queue reconciliation** - the service API lane is complete in merged PR #294; no new plan-backed product lane is active while the existing CI, test, generated-artifact, and workflow PR queue is reconciled serially. Runner-token and capacity debt remain tracked separately by issues #302 and #236.

## NEXT (Queued — next 2–4 sprints)

- **Moza native-visible frontier** - the archived R5 lane remains `native_response_ready` and hardware-blocked; its closeout and evidence handoff preserve `hardware_output_authorized=false` and do not authorize another attempt
- **Moza no-output operator navigation** - use `wheelctl moza artifact-index`, `wheelctl moza bench-wizard`, and `wheelctl moza verify-bundle --stage native-visible-ready` to inspect the blocked frontier; closed-loop artifacts are diagnostic only and create no authorization, output permission, or readiness claim
- **External control-stream lane** - completed through packaged artifact lifecycle proof in PR #210; its closeout preserves the virtual/package claim boundary
- **Moza vendor-specific control investigation** - six no-output sniff plans are recorded for Pit House, SimHub, and simulator sessions; Pit House open-idle, full-controls, and the repeat `pit-house-setting-change` capture have non-claiming receipts/summaries, the first setting-change capture remains low-yield historical evidence, and artifact-index/bench-wizard now surface the highest-frequency unknown commanded `0x7E`/`0x80` traffic plus bounded sample frames, packet groups, repeated motifs, empty/zero-filled payload-shape hints, low-confidence semantic hypotheses, a semantic correlation plan that now routes to SimHub/simulator gaps, and residual payload export gap locators without making them semantic or sendable
- **Moza Pit House coexistence** - external compatibility only; test closed/open/mode-change/update-page cases separately and do not make Pit House a native-control prerequisite
- **Moza passive USB sniff support evidence** - optional protocol research for Pit House, SimHub, and simulator traffic; three Pit House summaries are recorded, remaining captures are summary-only by default, no raw pcapng unless reviewed, and never a native or smoke-ready gate
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
