# Friction Log

Running record of pain points, blockers, and technical debt encountered during development. Reviewed periodically to drive improvements to tooling, process, and architecture.

Each entry has: **date**, **severity** (Low/Medium/High), **status** (Open/Resolved/Won't Fix), and a description + proposed remedy.

**Summary (71 items):** 12 Open · 2 In Progress · 52 Resolved · 1 Partially Resolved · 1 Investigating · 2 Noted · 1 Won't Fix

---

## Active Issues

### F-001 · Dual game support matrix sync (High · Partially Resolved)

**Encountered:** RC sprint (multi-vendor/game push)

Two YAML files must always contain identical game entries:
- `crates/telemetry-config/src/game_support_matrix.yaml` (runtime)
- `crates/telemetry-support/src/game_support_matrix.yaml` (tests)

Every time a game is added, both files must be updated manually. When they diverge, `GameService::new()` panics with an opaque "Missing config writer" error rather than a clear sync error.

**Remedy:** Generate one file from the other at build time, or introduce a CI check that diffs the two files and fails if they differ. Long term: merge into a single source of truth consumed by both crates.

**Update (CI check):** `.github/workflows/yaml-sync-check.yml` + `cargo run -p openracing-tools --bin yaml-sync-check -- --check` confirmed present. The workflow runs on every push/PR and fails with a clear diff message if the files diverge.

**Update (sync tool):** `cargo run -p openracing-tools --bin yaml-sync-check -- --fix` copies the canonical file after editing to keep both files in sync. See F-013 (Resolved). The single-source-of-truth refactor remains a long-term goal.

**Current state:** Files are now identical. The `dirt_rally_2` content divergence (6 lines of `supported_fields`) and `raceroom` omission were resolved. See F-013.

---

### F-013 · No developer-facing sync tool for game support matrix (Medium · Resolved)

**Encountered:** RC sprint / feat/r5-test-coverage-and-integration — F-001 kept recurring because developers had no easy command to sync the two YAML files after editing the canonical one.

The CI check catches divergence at PR time and the Rust sync tool now offers a fix path. Before that, developers had to manually copy the file or hunt down the right diff. This caused repeated F-001/F-013 CI failures.

**Fix applied:** `cargo run -p openracing-tools --bin yaml-sync-check -- --fix` copies `crates/telemetry-config/src/game_support_matrix.yaml` to the mirror. Use `--check` in CI or pre-commit hooks to verify without writing. Documented in `docs/DEVELOPMENT.md` under "Keeping game support matrix files in sync".

---

### F-002 · Duplicate config writer registration (High · **Resolved**)

**Encountered:** RC sprint

Every game's config writer must be registered in **two** separate files:
- `crates/telemetry-config/src/writers.rs` (used by `GameService` at runtime)
- `crates/telemetry-config-writers/src/lib.rs` (parallel crate)

Missing one silently causes tests to pass while runtime silently skips the writer. Caused several hard-to-debug failures.

**Fix applied:** `crates/telemetry-config-writers/src/lib.rs` is now the single source of truth for all ConfigWriter implementations. The four writers that existed only in the duplicate (`GranTurismo7ConfigWriter`, `AssettoCorsaConfigWriter`, `ForzaMotorsportConfigWriter`, `BeamNGDriveConfigWriter`) were migrated there. `crates/telemetry-config/src/writers.rs` was replaced with a single re-export line: `pub use racing_wheel_telemetry_config_writers::*;`. The `telemetry-config` crate now lists `racing-wheel-telemetry-config-writers` as a workspace dependency.

---

### F-012 · Manual telemetry configuration required per game (Low · Resolved)

**Encountered:** RC sprint — users had to manually enable UDP telemetry in each game's settings menu and enter the correct port/IP. Easy to miss; caused "no telemetry" support tickets.

**Fix applied:** `crates/service/src/game_auto_configure.rs` writes the required telemetry config file on first game detection; `crates/service/src/game_telemetry_bridge.rs` auto-starts/stops the matching adapter when the game process starts/exits. All 29 supported games are now plug-and-play with zero user setup steps.

---

### F-003 · Race condition: agents editing files during compilation (High · **Resolved**)

**Encountered:** RC sprint — agent-26 modifying `windows.rs` while `cargo check` was running

Concurrent file edits during active builds cause cascading compilation errors (references to constants that briefly exist/don't exist). This is especially bad for agents that progressively refine a large file.

**Fix applied:** `AGENTS.md` updated with a "Multi-agent / worktree rules" section that mandates `git worktree add` per agent, isolation to the agent's own worktree, and the cherry-pick rebase pattern after squash merges. See feat/r7-quirks-cleanup-v2.

---

### F-004 · Windows linker PDB limit in integration tests (Medium · **Resolved**)

**Encountered:** RC sprint — `racing-wheel-integration-tests` fails with LNK1318 / LNK1180 on Windows

The Windows linker hits its PDB symbol table size limit when the integration test crate is built in debug mode with all features, because it transitively pulls in every crate in the workspace.

**Fix applied:** Added `[profile.test.package.racing-wheel-integration-tests] debug = false` to workspace `Cargo.toml`. Disabling debug info for this specific package avoids generating a PDB file that exceeds the linker's symbol table limit, while leaving debug symbols enabled for all other packages. Note: `.cargo/config.toml` cannot be used because `.cargo/` is gitignored (machine-specific). (feat/r7-quirks-cleanup-v2)

---

### F-005 · Wrong protocol values in initial implementations (Medium · Resolved)

**Encountered:** RC sprint — multiple vendor protocol crates had wrong VIDs/PIDs on creation, requiring a full web-verification pass (agent-26) to fix:
- Cammus VID `0x3285 → 0x3416`
- Simucube VID `0x2D6A → 0x16D0`
- Asetek VID `0x2E5A → 0x2433`
- Simagic VID `0x2D5C → 0x3670`
- Logitech G923 PS PID `0xC266 → 0xC267`
- Thrustmaster TMX PID `0xB66D → 0xB67F`

Protocol values sourced from memory/guesses rather than verified sources.

**Remedy:** Add a `docs/protocols/SOURCES.md` that records the authoritative source (USB descriptor dump, community wiki URL, official SDK) for every VID/PID. Require a source citation when adding a new device. Add a unit test that cross-references the IDs against a checked-in golden file so a stale value causes a test failure.

**Fix applied:** `docs/protocols/SOURCES.md` added — tables every VID/PID for all 12 vendor protocol crates, with per-entry status (Verified / Community / Estimated) and source URLs. Unit tests added at `crates/hid-moza-protocol/tests/id_verification.rs` that assert all Moza VID/PID constants against the golden values in SOURCES.md, so any future stale constant causes a test failure.

---

### F-006 · Snapshot tests silently encoding wrong values (Medium · **Resolved**)

**Encountered:** RC sprint — Simagic snapshot tests were accepted with wrong legacy PIDs (`0x0101–0x0301`) and had to be regenerated after web-verification corrected the PIDs to `0x0500–0x0502`.

Snapshot tests provide no protection against "wrong but consistent" values: the first `--force-update-snapshots` run permanently bakes in whatever the code produces, even if the code is wrong.

**Fix applied (full):** Every HID vendor crate now has a `tests/id_verification.rs` test suite (15 crates total: Moza, Simagic, Cammus, VRS, Asetek, AccuForce, Fanatec, Heusinkveld, Leo Bodnar, Logitech, Simucube, Thrustmaster, OpenFFBoard, FFBeast, button-box) that asserts each VID/PID constant against the golden values in `docs/protocols/SOURCES.md`. Guidance for annotating snapshot files added to `docs/DEVELOPMENT.md` under "Snapshot tests and cross-validation". (feat/r7-quirks-cleanup-v2)

---

### F-013 · YAML sync requires manual update of two identical files (Medium · **Resolved**)

**Encountered:** R5 test coverage sprint

Both `crates/telemetry-config/src/game_support_matrix.yaml` and `crates/telemetry-support/src/game_support_matrix.yaml` must be kept identical. Every game addition requires two manual edits. The files have already diverged (see F-001); the CI diff check is the only safety net.

**Fix applied:** `cargo run -p openracing-tools --bin yaml-sync-check -- --fix` copies the canonical file to the mirror after editing. The long-term single-source-of-truth refactor remains tracked under F-001. (feat/r5-test-coverage-and-integration)

---

### F-014 · Agent race conditions on shared branch (High · **Resolved**)

**Encountered:** R5 test coverage sprint — multiple agents operating on `feat/r5-test-coverage-and-integration`

Multiple agents running concurrently on the same branch can cause YAML divergence, merge conflicts, and `workspace-hack` drift. The risk compounds when agents edit overlapping files without coordination.

**Fix applied:** `AGENTS.md` updated with a "Multi-agent / worktree rules" section (same fix as F-003): `git worktree` per agent, isolated directories, cherry-pick rebase pattern, and `cargo hakari generate` reminder. See feat/r7-quirks-cleanup-v2.

---

### F-015 · Workspace-hack requires manual regeneration (Low · **Resolved**)

**Encountered:** R5 test coverage sprint — adding new crates caused `workspace-hack` drift detected by CI

After adding new crates or changing feature flags, `cargo hakari generate` must be re-run manually to keep `workspace-hack/` in sync. CI catches the drift but the fix always requires a manual step.

**Fix applied:** Created `.githooks/pre-commit` — a versioned hook that runs `cargo hakari verify` and diffs the two `game_support_matrix.yaml` files before every commit. Also added `scripts/pre-commit/check-hakari.sh` as a standalone helper. `AGENTS.md` updated with hook setup instructions (`git config core.hooksPath .githooks`) and reminder to run `cargo hakari generate` when adding crates. (feat/r7-quirks-cleanup-v2)

---

### F-016 · `bench_results.json` generation undocumented (Low · **Resolved**)

**Encountered:** R5 maintenance review — `scripts/validate_performance.py bench_results.json --strict` referenced in `CLAUDE.md` but the file is never present in the repo

`bench_results.json` must be generated by running `cargo bench --bench rt_timing` with the env vars `BENCHMARK_JSON_OUTPUT=1` and `BENCHMARK_JSON_PATH=bench_results.json`. This is documented only inside `benches/rt_timing.rs` itself, not in `CLAUDE.md`, `README.md`, or any CI workflow.

**Fix applied:** `CLAUDE.md` updated — "Benchmarks and performance" section now shows the full two-step command: generate with `BENCHMARK_JSON_OUTPUT=1 BENCHMARK_JSON_PATH=bench_results.json cargo bench --bench rt_timing`, then validate. (feat/r7-quirks-cleanup-f007)

---

### F-007 · Symbol renames cascade across many test files (Medium · **Resolved**)

**Encountered:** RC sprint — `ProRacing → GPro`, `PRO_RACING → G_PRO` renames required manual fixes across:
- `logitech_e2e.rs`, `logitech_tests.rs`, `windows_property_tests.rs`, `windows.rs` (tests), etc.

No compile-time help distinguishes "this is a renamed constant" from "this constant was removed."

**Fix applied (feat/r7-quirks-cleanup-v2):** The `sequence → frame_seq` rename in the telemetry adapter crates is now complete — all 20 remaining adapter files (`acc.rs`, `ac_rally.rs`, `ams2.rs`, `automobilista.rs`, `dirt_rally_2.rs`, `dirt_showdown.rs`, `dirt3.rs`, `dirt5.rs`, `f1_25.rs`, `f1_native.rs`, `f1.rs`, `grid_2019.rs`, `grid_autosport.rs`, `grid_legends.rs`, `iracing.rs`, `kartkraft.rs`, `lib.rs`, `race_driver_grid.rs`, `rfactor2.rs`, `wtcr.rs`) updated.

**Remaining (structural):** The process issue — that renames cascade silently rather than with a deprecation warning — is still open. Use the `#[deprecated(since = "...", note = "use X instead")]` attribute on renamed constants/variants for at least one release cycle before removal.

---

### F-011 · Linux `emit_rt_event` borrow error hidden on Windows (Medium · Resolved)

**Encountered:** PR #15 CI — `UI Isolation Build (ubuntu-24.04)` failed with E0596:
`cannot borrow 'file' as mutable, as it is not declared as mutable` in
`crates/openracing-tracing/src/platform/linux.rs`.

`LinuxTracepointsProvider::trace_file` was `Option<File>` but `TracingProvider::emit_rt_event` takes `&self`, so `write_all` couldn't borrow it mutably. This compiled fine on Windows (only the Windows provider is compiled on that platform).

**Fix applied:** Wrapped in `Option<Mutex<File>>`; `emit_rt_event` uses `try_lock()` — contended writes increment `events_dropped` instead of blocking the RT thread. Commit `1c3fea5`.

**Lesson:** Platform-specific code must be CI-checked on all platforms. A Linux-only compile error was invisible during all local Windows development. See also F-003: the CI platform matrix is the only safety net for cross-platform bugs.

---

### F-008 · BeamNG gear value overflow (Resolved)

**Encountered:** RC sprint — gear field stored as `i8`, underflowed at `0x80` (reverse/neutral boundary in the game's UDP packet)

**Fix applied:** `crates/telemetry-adapters/src/beamng.rs` — cast via `u8` first before `as i8`.

**Lesson:** Game UDP telemetry fields that encode "neutral/reverse" as values near 127/128 should be parsed as `u8` and then mapped to a domain type, never directly to `i8`.

---

### F-009 · `static_mut_refs` denial missing from several crates (Resolved)

**Encountered:** RC sprint — CI caught 4 crates missing `#![deny(static_mut_refs)]`

**Fix applied:** agent-22 added the attribute to `openracing-watchdog` and related crates.

**Lesson:** The attribute should be added by a workspace-level `[lints]` table in `Cargo.toml` so new crates inherit it automatically. Track this as a follow-up cleanup.

---

### F-010 · Integration test function named after old API (Resolved)

**Encountered:** RC sprint — `scenario_pro_racing_uses_1080_degree_range` persisted in `logitech_e2e.rs` long after the `ProRacing → GPro` rename and the `1080° → 900°` correction.

**Fix applied:** agent-30 renamed it to `scenario_g_pro_uses_900_degree_range`.

**Lesson:** Function names in integration tests are documentation; they should be reviewed when the underlying protocol constant is renamed.

---

### F-020 · `cargo tree --duplicates` CI gate overly strict (Medium · Resolved)

**Encountered:** RC sprint (feat/r7-quirks-cleanup-v2) — the `dependency-governance` CI job ran `cargo tree --duplicates` and exited with code 1 on any output. Large workspaces using Tauri, tokio, and wasmtime inevitably have parallel major versions (`syn v1/v2`, `windows-sys`, `zip v2/v7`, etc.).

**Impact:** CI blocked on every PR, including branches that introduced zero new duplicate deps.

**Fix applied:** Changed `exit 1` to `::warning::` GitHub Actions annotation in `.github/workflows/ci.yml`. Duplicate version policy remains enforced by `cargo deny check` (in the `lint-gates` job) which respects `multiple-versions = "warn"` in `deny.toml`. Commit `b9ed332`.

---

### F-021 · Fuzz targets silently unlinked from crate dependencies (Low · Resolved)

**Encountered:** `fuzz_simplemotion.rs` imported `racing_wheel_simplemotion_v2::parse_feedback_report` but `racing-wheel-simplemotion-v2` was absent from `fuzz/Cargo.toml`. The file compiled with a broken import and was never caught by a workspace-level `cargo check`.

**Root cause:** The `fuzz/` directory is an isolated workspace (`[workspace]` in `fuzz/Cargo.toml`). It does not inherit workspace deps and is not checked by the main `cargo check --workspace`.

**Fix applied:** Added `racing-wheel-simplemotion-v2 = { path = "../crates/simplemotion-v2" }` to `fuzz/Cargo.toml`. Commit `4a250f3`.

**Lesson:** When adding a new fuzz target, always add its crate dep to `fuzz/Cargo.toml` and verify with `cargo check --bin fuzz_<name>` from the `fuzz/` directory.

---

### F-022 · ACC2 / AC EVO telemetry — no public protocol docs (Low · Open)

**Encountered:** RC sprint — checked for ACC2 (2025) and AC EVO (2026) telemetry protocols. Neither game has published UDP telemetry documentation.

**Impact:** Cannot implement adapters without community reverse-engineering. These are high-demand titles as they gain adoption.

**Proposed remedy:** Monitor Kunos community forums and GitHub issues; implement once protocol is documented. `seb_loeb_rally.rs` and `f1_manager.rs` are maintained as intentional stubs (no telemetry protocol) for similar reasons.

**Research update (2026-03):** Thorough web and GitHub research confirmed no public telemetry protocols exist:

- **ACC2:** Kunos Simulazioni has not announced a game called "ACC2." The existing ACC (1.x) uses the UDP broadcasting protocol v4 (port 9000). There is no separate ACC2 product, SDK, or shared-memory struct layout.
- **AC EVO:** In Early Access (v0.5.2 as of Feb 2026) on Steam (app ID 2492500). Built on a new engine, distinct from AC1 and ACC. No telemetry API, shared-memory interface, or UDP protocol documentation has been published. The Kunos official forum has no SDK/telemetry subforum for AC EVO. SimHub does not list AC EVO support. Zero GitHub repositories exist for AC EVO telemetry integration.
- **Historical context:** AC1 used Windows shared memory (`acpmf_physics`, `acpmf_graphics`, `acpmf_static`) and ACC uses UDP broadcast v4. AC EVO may adopt a similar approach once it exits Early Access, but nothing is documented yet.

**Action taken:** Added stub adapters (`acc2.rs`, `ac_evo.rs`) and game support matrix entries following the `f1_manager`/`seb_loeb_rally` stub pattern. These register the games for auto-detection visibility without claiming telemetry support. Adapters return `NormalizedTelemetry::default()` and emit no frames.

---

### F-023 · PXN HID report ID byte skipped — all input field offsets off by 1 (High · Resolved)

**Encountered:** PR #18 review (Qodo comment) — `feat/r6-pxn-v2`

`crates/hid-pxn-protocol/src/input.rs` `parse()` was reading steering from `data[0..2]`,
throttle from `data[2..4]`, etc. — treating the raw HID buffer as if byte 0 were the first
data field. But by convention (consistent with every other vendor protocol crate in the
repo), byte 0 of the HID buffer is the HID report ID (`0x01`). All fields were shifted
by one, so the steering angle was actually reading the throttle, the throttle was reading
the brake, etc. Zero-byte buffers appeared to parse to "center/all-zeros" because the
report ID and all data happened to be 0 in most tests.

**Root cause:** New crate authored without cross-checking the Logitech/Fanatec/Moza parsers
for offset convention. No integration test compared parsed steering against an actual
captured HID frame, so the bug was invisible.

**Fix applied:** commit `f8f46a4` on `feat/r6-pxn-v2`:
- `NEED` constant: 10 → 11 (report ID byte + 10 data bytes)
- Added `ParseError::WrongReportId { got: u8 }` variant
- `parse()` validates `data[0] == REPORT_ID` before extracting fields
- All field reads shifted +1 (steering: `data[1..3]`, throttle: `data[3..5]`, etc.)
- All inline tests, property tests, and snapshot tests updated to prepend `REPORT_ID`

**Lesson:** Every HID protocol crate must have at least one test that constructs a minimal
known-good frame (with report ID byte) and asserts the parsed result against expected
normalised values. A golden-frame test would have caught this immediately.

---

### F-024 · `insta` snapshot tests cannot auto-create files in non-interactive CI (Medium · Open)

**Encountered:** RC sprint (feat/r7-quirks-cleanup-v2) — adding new telemetry adapter snapshot tests for `f1_25` and `wrc_10`

When a new snapshot test is added and no `.snap` file exists, `insta` in non-interactive CI mode (`INSTA_UPDATE=no`, the default) fails with "snapshot assertion failed" instead of creating the file. `INSTA_UPDATE=new` is only useful in local interactive runs; CI environments exit without writing the new snapshot to disk.

**Impact:** Every new snapshot test requires manual pre-computation of the expected YAML output and committing a `.snap` file before the test can pass in CI. This is error-prone and time-consuming for complex normalisation output.

**Remedy:**
1. Add a CI job that runs with `INSTA_UPDATE=always` and uploads the generated `.snap` files as artifacts, allowing developers to download and commit them.
2. Alternatively, add a developer script (`scripts/update_snapshots.sh`) that runs `INSTA_UPDATE=always cargo test` and commits the result.
3. Long term: use `insta`'s `force-update-snapshots` feature combined with a dedicated "snapshot refresh" CI workflow that opens a PR with the updated files.

---

### F-025 · Windows PowerShell shell sessions die immediately in agent environment (High · **Won't Fix**)

**Encountered:** RC sprint (feat/r7-quirks-cleanup-v2) — all new PowerShell sessions (sync and async modes) exited with no output

All new PowerShell sessions created via the agent tooling die immediately after creation — `list_powershell` returns "invalid shell ID" for any session created in the current agent turn, even for simple one-liners like `echo test`. Pre-existing sessions from prior turns complete but cannot be re-used. The root cause is unknown (possible Windows credential/profile issue, terminal initialisation error, or environmental state).

**Impact:** Cannot run any shell commands locally — `cargo build`, `git status`, `cargo fmt`, etc. are all unavailable. Must work around by using task sub-agents (which have working shells in a subprocess), or by using file-reading tools only. Significantly degrades agent productivity.

**Remedy:**
1. Investigate Windows PowerShell profile or credential issue — check `$PROFILE` and Windows Event Log.
2. Try resetting the agent environment or restarting VS Code.
3. As a process improvement: document that task sub-agents provide a working shell fallback when the main session shell is broken.

**Won't Fix:** This is an external agent-environment issue, not an OpenRacing project defect. Shell sessions work reliably in most agent sessions; intermittent failures are not actionable from the project side.

---

### F-026 · Codemasters Mode 1 UDP adapters had systematically wrong byte offsets (High · Resolved)

**Encountered:** RC sprint — telemetry adapter review / byte-offset audit

Seven adapters sharing the Codemasters legacy 66-float UDP packet (DiRT Rally 2, DiRT 3, DiRT 4, Dirt Showdown, GRID 2019, GRID Legends, Race Driver GRID) all had incorrect byte offsets. The offsets were shifted — e.g., throttle was read at offset 108 (actually `wheel_speed_fl`), RPM at 140 (actually `g_force_lon`), gear at offset 124 (actually `brakes`). The `fuel_percent` calculation also multiplied by 100, but the `NormalizedTelemetry` builder clamps to `[0, 1]`, causing fuel to always show 100%. `MIN_PACKET_SIZE` was 252 instead of the correct 264 bytes (66 × f32 LE = 264 bytes).

**Root cause:** The original offsets were likely transcribed from an unverified or mislabelled source and then copy-pasted across all seven files. No golden-packet test existed to catch the mismatch.

**Fix applied:** All seven adapter files corrected to match the community-verified layout documented in community tools like `dr2_logger`. `MIN_PACKET_SIZE` updated to 264. Fuel calculation changed to pass the raw `[0, 1]` value. Affected files: `dirt_rally_2.rs`, `dirt3.rs`, `dirt4.rs`, `dirt_showdown.rs`, `grid_2019.rs`, `grid_legends.rs`, `race_driver_grid.rs`.

**Lesson:** Adapters that share a common packet format should extract the shared parsing logic into a single helper (e.g., `codemasters_mode1_parse()`) so offset definitions exist in exactly one place. Any adapter sharing a format via copy-paste should be flagged in code review.

**Update (shared parsing extracted):** `codemasters_shared.rs` now contains the single shared Mode 1 parsing implementation. All seven adapters delegate to it, removing ~890 lines of duplicated offset logic. This friction point is fully resolved.

---

### F-027 · Forza tire temperatures assumed Kelvin, actually Fahrenheit (Medium · Resolved)

**Encountered:** RC sprint — telemetry adapter protocol verification audit

The Forza adapter comments said "Kelvin" and converted with `k - 273.15`, but Forza Motorsport/Horizon actually sends tire temps in Fahrenheit. The correct conversion is `(f - 32) * 5/9`.

**Evidence:** The `stelmanjones/fmtel` Go library explicitly documents `// Tire temperatures in fahrenheit.` and converts with `(temp - 32) * 5 / 9`. The `mplutka/tm-bt-led` JS implementation does the same. The official Forza "Data Out" format documentation does not annotate units.

**Fix applied:** Updated `forza.rs` comments and conversion. Default fallback changed from `293.15` (20°C in Kelvin) to `68.0` (20°C in Fahrenheit).

---

### F-028 · fuel_percent × 100 bug in LFS, AMS1; f32/f64 mismatch in RaceRoom (Medium · Resolved)

**Encountered:** RC sprint — full adapter fuel_percent audit

Three adapters had fuel bugs found during the systematic audit triggered by F-026:

1. **LFS** (`lfs.rs`): `fuel * 100.0` passed to `.fuel_percent()`, but the builder clamps to [0,1] — fuel always showed 100%.
2. **Automobilista** (`automobilista.rs`): Same `* 100.0` pattern.
3. **RaceRoom** (`raceroom.rs`): FuelLeft/FuelCapacity read as `f32` (4 bytes), but R3E SDK uses `f64` (8 bytes). Reading 4 bytes of a double produces garbage. Added `read_f64_le()` helper.

**Fix applied:** Removed `* 100.0` from LFS/AMS1. Changed RaceRoom to read f64 and cast to f32. Updated snapshot files.

---

### F-029 · cargo-udeps false positives in CI Dependency Governance job (Medium · **Resolved**)

**Encountered:** Cleanup sprint — CI `dependency-governance` job

`cargo-udeps` flags many workspace dependencies as unused when they are actually consumed transitively, in doc-tests, or in build scripts. Examples include shared utility crates pulled in via `workspace-hack`, `cfg`-gated platform dependencies, and crates used only in `#[doc = include_str!(...)]` or `build.rs`. The false-positive rate is high enough that the job output is noisy and real unused deps are easy to miss.

**Impact:** Developers ignore the CI output because most flagged crates are legitimate. Genuinely unused dependencies accumulate without notice.

**Proposed remedy:**
1. Pin a known-good `cargo-udeps` version and re-evaluate after upstream fixes for transitive/doctest detection.
2. Add an allow-list (`udeps.toml` or inline `#[cfg_attr]` annotations) for confirmed false positives so the CI signal is actionable.
3. Consider supplementing with `cargo machete` which uses a different heuristic and may have fewer false positives for workspace setups.

**Fix applied (Wave 16):** Missing ignore entries added for 8 crates. CI dependency governance check made non-blocking (warning-only) to avoid false-positive pipeline failures. Policy enforcement remains in `cargo deny check` via `deny.toml`.

---

### F-030 · Assetto Corsa adapter used wrong protocol entirely (High · Resolved)

**Encountered:** Protocol verification wave — web research against vpicon/acudp, lmirel/mfc, Kunos C# SDK

AC adapter was parsing 76-byte OutGauge packets (used by LFS, BeamNG) but Assetto Corsa actually uses its own Remote Telemetry UDP protocol with a 3-step handshake and 328-byte RTCarInfo packets. Every field offset was wrong. All AC telemetry was garbage data.

**Fix applied:** Complete rewrite to Remote Telemetry UDP — sends handshake, subscribes to updates, parses RTCarInfo. Integration test updated to mock AC server. Committed as `9365e99`.

---

### F-031 · Simagic M10 PID collision with Simucube 1 (High · Resolved)

**Encountered:** Engine device sync — both Simagic M10 and Simucube SC1 listed at VID 0x16D0, PID 0x0D5A

Agent-20's device sync added "Simagic M10" at PID 0x0D5A, but that PID on VID 0x16D0 belongs to Simucube 1 (confirmed via official Simucube developer docs, gro-ove/actools). The Simagic M10 actually uses VID 0x0483 PID 0x0522 (shared with Alpha family via STM32 bootloader). Also had "Simagic FX" at 0x0D5B — also wrong.

**Fix applied:** Removed ghost M10/FX entries from windows.rs and linux.rs. Added correct Simucube 1 entry. Committed in `54c8b22`.

---

### F-032 · Estimated PIDs for unreleased Simagic devices (Low · Resolved)

**Encountered:** Protocol verification wave

Simagic Alpha EVO (0x0600), Neo (0x0700), and Neo Mini (0x0701) PIDs are estimates based on sequential numbering convention. No community source confirms these values. These devices may not have shipped yet.

**Impact:** If wrong, these devices won't be detected. Low risk — devices will still get some FFB from the Simagic family fallback.

**Remedy:** Acquire hardware captures or wait for community reverse engineering (JacKeTUs/simagic-ff driver updates).

**Merged to main:** PR #19 merge (d6fba74).

---

### F-033 · Simucube Wireless Wheel PID unconfirmed (Low · Resolved)

**Encountered:** Protocol verification wave

Simucube Wireless Wheel (PID 0x0D63) is listed in engine tables but not confirmed in any public source. It's a receiver, not a force feedback device, so we set torque to 0 Nm. If the PID is wrong it won't cause harm (no FFB commands sent to it).

**Merged to main:** PR #19 merge (d6fba74).

---

### F-034 · Shared USB VIDs require PID-based runtime disambiguation (Medium · Resolved)

**Encountered:** Protocol verification wave (Heusinkveld + VRS)

Two USB Vendor IDs are each shared by **three or more** sim racing hardware vendors:

**VID `0x16D0` (MCS Electronics / OpenMoko):**
- Simucube 2 wheelbases (Granite Devices) — PIDs `0x0D5A`–`0x0D66`
- Legacy Simagic — PID `0x0D5A`

**VID `0x04D8` (Microchip Technology):**
- Heusinkveld pedals — PIDs `0xF6D0`–`0xF6D3` (moved from VID `0x16D0`; see OpenFlight cross-ref)

**VID `0x0483` (STMicroelectronics):**
- VRS DirectForce Pro — PIDs `0xA355`–`0xA35A`
- Legacy Simagic (Alpha family) — PIDs `0x0522`–`0x0524`
- Cube Controls (PROVISIONAL) — PIDs `0x0C73`–`0x0C75`
- Hundreds of non-sim STM32 devices

None of these vendors own their VID; they sub-license or reuse a chip vendor's default. VID-only matching will mis-identify devices. The engine's `get_vendor_protocol()` already dispatches by PID within each shared VID, but this is fragile: any new vendor shipping on `0x0483` or `0x16D0` with a PID inside an existing range would collide silently.

Additionally, most individual PIDs for these vendors (Heusinkveld, VRS, Cube Controls) are **unverified** in external USB databases (USB-IF, linux-hardware.org, devicehunt.com). They were likely derived from hardware captures or firmware dumps rather than public registries.

**Impact:** Medium — mis-routing a wheelbase as pedals (or vice versa) could send FFB torque commands to a non-actuated device or fail to initialize FFB. Existing PID-range dispatch in `crates/engine/src/hid/vendor/mod.rs` mitigates this today.

**Remedy:** (1) Acquire USB captures from actual hardware for all unverified PIDs. (2) Consider adding a secondary check (e.g. HID usage page, product string) when VID is `0x0483` or `0x16D0` to reduce the risk of PID-only mismatches. (3) Document vendor-specific PID ranges as reserved in a shared constants file so new vendors don't accidentally overlap.

**Merged to main:** PR #19 merge (d6fba74).

---

### F-035 · PCars2 adapter rewritten from fabricated offsets to correct SMS UDP v2 format (High · Resolved)

**Encountered:** RC telemetry adapter audit (2025-06)

The Project CARS 2 telemetry adapter used entirely fabricated byte offsets that did not correspond to the actual SMS UDP v2 protocol. Field positions were wrong for speed, RPM, gear, and throttle/brake inputs, resulting in garbage telemetry data at runtime.

**Fix applied:** Complete rewrite of the PCars2 adapter to use correct SMS UDP v2 packet format with verified struct offsets from the official Slightly Mad Studios documentation. Snapshot tests added and passing.

---

### F-036 · Leo Bodnar PID 0xBEEF confirmed as placeholder — no real hardware match found (Low · Resolved)

**Encountered:** RC device verification audit (2025-06)

The SLI-M entry in `hid-leo-bodnar-protocol` uses PID `0xBEEF`, which is a common development placeholder value. Checked: devicehunt.com, linux-hardware.org USB database, the-sz.com VID registry, GitHub code search, and JacKeTUs/linux-steering-wheels — no match found for VID `0x1DD2` + PID `0xBEEF`.

**Remedy:** Acquire a USB device capture from real Leo Bodnar SLI-M hardware to determine the actual PID. Until then, `0xBEEF` is flagged as provisional in code and documentation.

**Merged to main:** PR #19 merge (d6fba74).

---

### F-037 · OpenFFBoard PID 0xFFB1 absent from all sources — likely doesn't exist (Low · Resolved)

**Encountered:** RC device verification audit (2025-06)

The OpenFFBoard alt PID `0xFFB1` is listed in the protocol crate but cannot be found in: pid.codes registry (only `0xFFB0` registered), OpenFFBoard firmware source (Ultrawipf/OpenFFBoard), JacKeTUs/linux-steering-wheels, Linux kernel hid-ids.h, or any USB capture database. It may have been speculatively added for a planned firmware variant that was never released.

**Remedy:** Review OpenFFBoard firmware release history and changelogs to determine if `0xFFB1` was ever shipped. If not, consider removing or marking as deprecated.

**Merged to main:** PR #19 merge (d6fba74).

---

### F-038 · Cube Controls PIDs 0x0C73–0x0C75 unverifiable — product pages return 404 (Medium · Resolved)

**Encountered:** RC device verification audit (2025-06)

Cube Controls GT Pro, Formula CSX-3, and F-CORE PIDs `0x0C73`–`0x0C75` cannot be verified. Product pages for several models return HTTP 404. No entries found in: JacKeTUs/linux-steering-wheels, devicehunt.com VID `0x0483` database, Linux kernel hid-ids.h, or GitHub code search for USB captures. These are button boxes (input-only, non-FFB), not wheelbases.

**Fix applied (partial):** Devices reclassified as input-only in code and docs. PIDs kept as provisional placeholders with doc comments.

**Remedy:** Acquire a USB device tree capture (`lsusb -v` or USBTreeView) from real Cube Controls hardware to confirm or correct VID/PIDs.

**Merged to main:** PR #19 merge (d6fba74).

---

### F-039 · VRS DirectForce Pro PID 0xA355 confirmed via linux-steering-wheels (Low · Resolved)

**Encountered:** RC device verification audit (2025-06)

VRS DirectForce Pro PID `0xA355` was previously listed as community-reported without a specific source. Confirmed via JacKeTUs/linux-steering-wheels database and cross-referenced with linux-hardware.org USB captures under VID `0x0483`.

**No code change needed** — PID was already correct. Status upgraded from community-reported to verified in documentation.

---

### F-040 · 100% telemetry adapter snapshot test coverage achieved (Medium · Resolved)

**Encountered:** RC test coverage audit (2025-06)

Prior to this sprint, many telemetry adapters lacked snapshot tests, meaning protocol regressions could slip through undetected. A systematic audit identified all untested adapters.

**Fix applied:** Snapshot tests added for all 56 telemetry adapters (100% coverage). Each test verifies that a representative packet produces the expected `TelemetryData` output, catching field mapping regressions and byte offset errors.

---

### F-041 · 126 additional unwrap/expect calls eliminated from 8 test files (Medium · Resolved)

**Encountered:** RC test quality audit (2025-06)

Per the project's testing rules (no `unwrap()`/`expect()` in tests), a sweep of test files found 126 remaining instances across 8 files. These could mask errors by panicking instead of producing clear test failure messages.

**Fix applied:** All 126 calls replaced with `Result`-returning test functions, explicit assertions, or `?` propagation. Zero `unwrap()`/`expect()` calls remain in test code.

---

### F-042 · Asetek Tony Kanaan torque corrected 18→27 Nm, added 8 proptest properties (Medium · Resolved)

**Encountered:** RC device verification audit (2025-06)

The Asetek Tony Kanaan Edition wheelbase was listed at 18 Nm torque, but the official Asetek spec sheet and JacKeTUs/universal-pidff both list 27 Nm. Additionally, the Asetek protocol crate lacked property-based tests for edge cases.

**Fix applied:** Tony Kanaan `max_torque_nm()` corrected from 18.0 to 27.0. Eight proptest property tests added covering torque scaling, command serialization, and round-trip invariants.

---

### F-051 · Leo Bodnar PID 0xBEEF is a placeholder, needs real USB PID (Low · Resolved)

**Encountered:** Wave 15 RC hardening (2025-06)

The SLI-M entry in `hid-leo-bodnar-protocol` used PID `0xBEEF`, which is a common development placeholder. No public USB database lists this PID for VID `0x1DD2`. The placeholder has been replaced with community-estimated PID `0x1301` (source: OpenFlight compat DB, sim racing community reports). The product name was corrected from "SLI-M" (non-existent) to "SLI-Pro" (actual Leo Bodnar product). PID still needs hardware capture to fully confirm. See also F-036.

**Remedy:** PID updated to `0x1301` (community estimate). Full confirmation still requires a real USB device capture from SLI-Pro hardware.

---

### F-052 · OpenFFBoard PID 0xFFB1 unverified (Low · **Resolved**)

**Encountered:** Wave 15 RC hardening (2025-06)

The OpenFFBoard alt PID `0xFFB1` is listed in the protocol crate but cannot be found in pid.codes (only `0xFFB0` registered), OpenFFBoard firmware source, or any USB capture database. It may have been speculatively added for a firmware variant that was never released. See also F-037.

**Web verification (2025-07):** Re-checked against 5 independent sources:
- pid.codes `1209/FFB1` → HTTP 404 (not registered)
- OpenFFBoard firmware `usb_descriptors.cpp` → only `USBD_PID 0xFFB0`
- OpenFFBoard-configurator `serial_ui.py` → `OFFICIAL_VID_PID = [(0x1209, 0xFFB0)]`
- GitHub code search on `Ultrawipf/OpenFFBoard` for "FFB1" → zero results
- JacKeTUs/linux-steering-wheels → only VID `1209` / PID `ffb0` listed (Platinum, hid-pidff)

**Resolved (Wave 34):** Confirmed SPECULATIVE. Zero evidence across all 5 sources; PID `0xFFB1` has never appeared in firmware, configurator, pid.codes, or any community database. Flagged as speculative in protocol crate.

---

### F-053 · macOS not in CI matrix (Medium · **In Progress**)

**Encountered:** Wave 15 RC hardening (2025-06)

The CI workflow matrix covers Linux and Windows but does not include macOS. macOS is a supported platform (macOS 10.15+) with platform-specific code paths (e.g., `thread_policy_set` for RT scheduling). Platform-specific compile errors and behavioral differences can go undetected until manual testing.

**Remedy:** Add a macOS runner (`macos-latest`) to the CI matrix for at least the build and test jobs. Consider using `macos-13` for x86_64 and `macos-14` for ARM64 coverage.

**Update:** PR #84 adds macOS (`macos-latest`) to the CI matrix for CLI, Service, and Workspace builds.

---

### F-054 · No MSRV check job in CI (Low · **Resolved**)

**Encountered:** Wave 15 RC hardening (2025-06)

**Resolved (PR #24, 2025-07):** MSRV Check job exists in CI workflow (`ci.yml`) and passes. The job verifies compilation against the minimum supported Rust version specified in the workspace.

---

### F-055 · 44 unwrap/expect remaining in test files (convention violation) (Medium · Resolved)

**Encountered:** Wave 15 RC hardening (2025-06)

Despite the F-041 cleanup (126 calls removed), 44 `unwrap()`/`expect()` calls remained across test files. Per project convention (no `unwrap()`/`expect()` in tests), these should be replaced with `Result`-returning test functions, explicit assertions, or `?` propagation.

**Resolved:** Wave 16 — all remaining `unwrap()`/`expect()` calls eliminated. 0 instances across all test files. CI lint recommended to prevent regression.

---

### F-056 · VRS Pedals V1 PID migration: old 0xA357 → new 0xA3BE (Medium · **Resolved**)

**Encountered:** Wave 16 protocol verification (2025-06)

VRS Pedals V1 has undergone a PID change from `0xA357` to `0xA3BE`. The protocol crate and engine dispatch tables need updating. Users on older firmware may still present as `0xA357`, so both PIDs should be recognized during a transition period.

**Resolved:** Both PIDs are already recognized everywhere:
- `hid-vrs-protocol/src/ids.rs`: `PEDALS = 0xA3BE` (primary), `PEDALS_V1 = 0xA357` (legacy alias)
- `engine/src/hid/windows.rs`: Both PIDs in SupportedDevices list
- `engine/src/hid/linux.rs`: Both PIDs in device table
- `engine/src/hid/vendor/vrs.rs`: `is_vrs_product()` matches `0xA355..=0xA35A | 0xA3BE`
- `determine_device_capabilities()`: Matches both PIDs for non-FFB peripheral classification
- All test suites verify both PIDs. No user-facing functionality gap.

---

### F-057 · VRS DFP V2 PID 0xA356 unverified (Low · Open)

**Encountered:** Wave 16 protocol verification (2025-06)

The VRS DirectForce Pro V2 PID `0xA356` is present in the protocol crate but has not been independently verified via USB captures, linux-steering-wheels, or VRS official documentation. It may be an internal engineering PID or a community estimate.

**Web verification (2025-07):** Re-checked against 4 independent sources:
- Linux kernel `hid-ids.h` → only `USB_DEVICE_ID_VRS_DFP 0xa355` and `USB_DEVICE_ID_VRS_R295 0xa44c` (no 0xa356)
- JacKeTUs/linux-steering-wheels → only VID `0483` / PID `a355` listed (Platinum, "Turtle Beach VRS")
- JacKeTUs/simracing-hwdb `90-vrs.hwdb` → only `v0483pA355` (DFP) and `v0483pA3BE` (Pedals)
- VRS website (virtualracingschool.com) → no USB identifiers published

VRS is now branded "Turtle Beach VRS" in linux-steering-wheels (Turtle Beach acquired VRS).

**Remedy:** Confirm PID via hardware capture or VRS/Turtle Beach support. Flag as provisional in protocol crate until verified.

**Update (Wave 15 RC, 2025-07):** Re-verified — still no independent confirmation across 4 sources. No status change.

---

### F-058 · Heusinkveld PIDs updated to VID 0x04D8 (Microchip) (Low · Resolved)

**Encountered:** Wave 16 protocol verification (2025-06)

All three Heusinkveld PIDs were originally under VID `0x16D0` with no external verification. Cross-referencing with the OpenFlight sister project (`EffortlessMetrics/OpenFlight`) revealed Heusinkveld uses VID `0x04D8` (Microchip Technology) with PIDs in the `0xF6Dx` range. Updated VID to `0x04D8` and PIDs to `0xF6D0` (Sprint), `0xF6D2` (Ultimate+), `0xF6D3` (Pro, estimated). Pro PID is estimated from sequential pattern; a USB capture would confirm.

**Remedy:** USB captures from Heusinkveld hardware owners would fully confirm the OpenFlight-sourced PIDs.

---

### F-059 · Cube Controls PIDs (all 3) provisional, no external evidence (Low · **Resolved**)

**Encountered:** Wave 16 protocol verification (2025-06)

Cube Controls PIDs `0x0C73`, `0x0C74`, `0x0C75` were confirmed FABRICATED with zero external evidence across 8 sources.

**Resolved (PR #24, 2025-07):** Fabricated PIDs removed from FFB dispatch. Cube Controls reclassified as input-only button boxes — the protocol crate retains the PIDs for input identification but they are no longer used for force feedback dispatch. See F-038, F-073.

---

### F-060 · Cammus new pedal PIDs need wiring into engine/dispatch (Medium · **Resolved**)

**Encountered:** Wave 16 protocol verification (2025-06)

New Cammus pedal PIDs have been identified but are not yet wired into the engine dispatch table or device capability matrix. The protocol crate may define the PIDs, but the engine cannot recognize or route HID reports for these devices until dispatch entries are added.

**Fix applied:** Cammus CP5 (0x1018) and LC100 (0x1019) pedals are now fully wired:
- SupportedDevices table in `windows.rs:506-507`
- Linux device list in `linux.rs:494-495`
- `determine_device_capabilities()` in `windows.rs:1635-1640` (non-FFB, input-only)
- Property test exclusion list in `windows_property_tests.rs:190`
- Vendor module `cammus.rs` with `is_cammus_product()` returning true for both PIDs

---

### F-061 · Simucube protocol crate uses speculative wire format (High · Partially Resolved)

**Encountered:** Wave 17 kernel protocol verification

The `hid-simucube-protocol` crate's `input.rs` and `output.rs` modules use a custom binary layout (22-bit angle sensor, torque-streaming output, centi-Nm encoding) that **does not match the actual device protocol**. Research confirmed Simucube uses standard **USB HID PID** (Physical Interface Device) protocol:
- Input: standard HID joystick report with 16-bit unsigned axes + 128 buttons
- Output: effect-based PID descriptors (Constant, Spring, Damper, etc.) — not torque streaming
- The 22-bit encoder is internal and NOT exposed over USB (16-bit X axis instead)
- Rotation range is configured via True Drive software, not USB commands

PIDs, VID, torque specs, and model classification are verified correct.

**Source:** `github.com/Simucube/simucube-docs.github.io`, `granitedevices.com/wiki/`

**Partial resolution (2025-07):** `SimucubeHidReport` now accurately models the
documented HID joystick layout (u16 X/Y + 6 axes + 128 buttons = 32 bytes).
Field set verified against official Simucube developer docs and Granite Devices
wiki. `SimucubeInputReport` (speculative extended diagnostics) and `output.rs`
(placeholder torque-streaming format) remain unverified.

**Remaining remedy:** Rewrite output module to use HID PID protocol. This is a significant refactor that requires understanding the USB HID PID usage page (0x0F). Consider sharing a common `hid-pid` crate across Simucube and any other HID PID devices. Low priority since Simucube works via DirectInput on Windows regardless.

---

### F-062 · Fanatec sign-fix was inverted — CSR Elite is the exception, not the target (Medium · **Resolved**)

**Encountered:** Wave 17 kernel protocol verification

The `needs_sign_fix()` method on `FanatecModel` originally returned `true` only for CSR Elite, but kernel driver analysis shows the opposite: `fix_values()` in `hid-ftecff.c:send_report_request_to_device()` applies sign correction for **all** wheelbases **except** CSR Elite.

**Fix applied:** Inverted the method to return `true` for all models except CSR Elite and Unknown.

---

### F-063 · Fanatec range command uses different encoding than kernel driver (Low · Open)

**Encountered:** Wave 17 kernel protocol verification

Our `build_rotation_range_report()` uses `[0x01, 0x12, range_lo, range_hi, ...]` (report ID + command byte), but the kernel driver (`ftec_set_range`) sends a 3-step sequence: `[0xF5, ...]` → `[0xF8, 0x09, ...]` → `[0xF8, 0x81, range_lo, range_hi, ...]`. Added `build_kernel_range_sequence()` as the kernel-verified alternative.

**Remedy:** Determine which encoding is correct for Windows. The kernel sequence may be Linux-specific. Both are now available; integration code should use the kernel sequence when talking to raw HID.

---

### F-064 · GT7 extended packet types (316/344 bytes) not supported (Low · **Resolved**)

**Encountered:** Wave 18 telemetry protocol verification (2025-07)

GT7 v1.42+ (2023) added two new heartbeat types that return larger packets with additional telemetry fields. Our adapter only supports the original PacketType1 (heartbeat `"A"`, 296 bytes, XOR key `0xDEADBEAF`). The newer types are:
- PacketType2 (heartbeat `"B"`, 316 bytes, XOR `0xDEADBEEF`): adds WheelRotation (radians), Sway, Heave, Surge — useful for motion platforms.
- PacketType3 (heartbeat `"~"`, 344 bytes, XOR `0x55FABB4F`): adds energy recovery, filtered throttle/brake, car-type indicator.

**Source:** [`Nenkai/PDTools`](https://github.com/Nenkai/PDTools) `SimulatorInterfaceClient.cs` (commit 5bb714c) and `SimulatorInterfaceCryptorGT7.cs`.

**Remedy:** Add `PacketType` configuration to `GranTurismo7Adapter` (default to PacketType3 for maximum data). Requires parameterising the heartbeat byte, XOR key, and expected packet size. Low priority — all core telemetry fields (RPM, speed, gear, throttle, brake, tyre temps) are available in PacketType1.

**Update (Wave 15 RC, 2025-07):** Still not implemented. Remains low priority — core telemetry works with PacketType1.

**Resolved (Wave 31-32):** Extended packet support (316/344 bytes) implemented in `gran_turismo_7.rs`. All three packet types now supported with configurable heartbeat byte, XOR key, and expected packet size.

---

### F-065 · GT Sport ports were swapped (recv 33739→33340, send 33740→33339) (High · **Resolved**)

**Encountered:** Wave 15 RC hardening (2025-07)

GT Sport telemetry adapter had receive and send ports swapped: recv was `33739` (should be `33340`) and send was `33740` (should be `33339`). GT7 uses recv `33740` / send `33739`; GT Sport uses the lower pair (`33340` / `33339`). Cross-referenced against Nenkai/PDTools `SimulatorInterfaceClient.cs` (`BindPortDefault=33340`, `ReceivePortDefault=33339`) and SimHub wiki (GT Sport: UDP ports 33339 and 33340).

**Fix applied:** `gran_turismo_sport.rs` corrected: `GTS_RECV_PORT = 33340`, `GTS_SEND_PORT = 33339`. `telemetry-config-writers/src/lib.rs` updated with `GTS_DEFAULT_PORT = 33340`. Port verification tests added.

---

### F-066 · Heusinkveld Pro PID 0xF6D3 has zero external evidence (Low · Open)

**Encountered:** Wave 15 RC hardening (2025-07)

Heusinkveld Pro PID `0xF6D3` is estimated from the sequential pattern after Sprint (`0xF6D0`) and Ultimate+ (`0xF6D2`). The Pro pedal set is discontinued. No USB capture, vendor documentation, or community database confirms this PID. The only source is the sequential-numbering assumption from the OpenFlight cross-reference.

**Remedy:** Obtain a USB capture from Heusinkveld Pro hardware. If unavailable (discontinued product), mark as estimated/provisional in protocol crate with a comment. See also F-058.

---

### F-067 · Heusinkveld Sprint/Ultimate+ PIDs have single-source evidence only (Low · Open)

**Encountered:** Wave 15 RC hardening (2025-07)

Heusinkveld Sprint PID `0xF6D0` and Ultimate+ PID `0xF6D2` (VID `0x04D8`, Microchip Technology) come from a single source: the OpenFlight project (`EffortlessMetrics/OpenFlight`). No independent confirmation from USB captures, Linux kernel `hid-ids.h`, JacKeTUs/linux-steering-wheels, or Heusinkveld official documentation. Single-source PIDs carry higher risk of being incorrect.

**Remedy:** Seek independent USB captures. Until a second source confirms, flag as single-source in protocol crate comments.

---

### F-068 · Fanatec GT DD Pro and ClubSport DD PID findings (Low · **Investigating**)

**Encountered:** Wave 15 RC hardening (2025-07)

Fanatec GT DD Pro PID `0x0024` and ClubSport DD PID `0x01E9` (VID `0x0EB7`) were present in engine dispatch tables and protocol crate but had no external confirmation. Comments in `windows.rs` and `linux.rs` note "from USB captures; not yet in community drivers." The Linux kernel `hid-fanatec.c` driver does not include these PIDs.

**Wave 34 findings:** GT DD Pro and ClubSport DD confirmed to share PID `0x0020` with CSL DD in PC mode. The previously listed PIDs (`0x0024`, `0x01E9`) may represent console-mode or firmware-variant PIDs. PID `0x0020` is the confirmed PC-mode PID used by all three devices (CSL DD, GT DD Pro, ClubSport DD).

**Remedy:** Update engine dispatch tables to use PID `0x0020` for GT DD Pro and ClubSport DD in PC mode. Investigate whether `0x0024` and `0x01E9` represent console-mode PIDs and document accordingly.

---

### F-069 · deny.toml broken with cargo-deny 0.19+ (Medium · **Resolved**)

**Encountered:** Wave 15 RC hardening (2025-07)

`deny.toml` used configuration syntax incompatible with `cargo-deny` 0.19+, causing CI failures in the dependency governance job. The schema version and field names needed updating for the newer cargo-deny release.

**Fix applied:** Updated `deny.toml` to cargo-deny 0.19-compatible syntax. CI dependency governance job passes cleanly.

---

### F-070 · TelemetryBuffer mutex unwrap panics (Medium · **Resolved**)

**Encountered:** Wave 15 RC hardening (2025-07)

`TelemetryBuffer` in `openracing-telemetry-streams` used `mutex.lock().unwrap()` which panics on poisoned mutexes. In a real-time context, a panic in the telemetry path could crash the entire service. This violates the project convention against `unwrap()`/`expect()`.

**Fix applied:** Replaced `unwrap()` calls with proper error handling. `TelemetryBuffer` now handles poisoned mutexes gracefully without panicking.

---

### F-071 · CI workflows lacked timeout-minutes (Medium · **Resolved**)

**Encountered:** Wave 15 RC hardening (2025-07)

Multiple CI workflow jobs had no `timeout-minutes` set, meaning a hung build or test could consume GitHub Actions runner minutes indefinitely. This is a CI cost and reliability risk.

**Fix applied:** All CI workflow jobs now have explicit `timeout-minutes` values (5–180 min depending on job type). Covers `ci.yml`, `docs.yml`, `compat-tracking.yml`, `governance-automation.yml`, `integration-tests.yml`, `mutation-tests.yml`, `nightly-soak.yml`, `regression-prevention.yml`, `release.yml`, `yaml-sync-check.yml`, and `schema-validation.yml`.

---

### F-072 · PXN V10/V12/GT987 protocol crate added (Low · **Resolved**)

**Encountered:** Wave 31-32 (2025-07)

PXN racing wheel support was previously only on `feat/r6-pxn-v2` and not merged into the main RC hardening branch. A dedicated `hid-pxn-protocol` crate has been added with VID/PIDs web-verified against the Linux kernel `hid-ids.h` (VID `0x11FF` / Lite Star), covering V10 (`0x3245`), V12 (`0x1212`), and GT987 models.

**Resolved:** Protocol crate created with full proptest/snapshot coverage and web-verified VID/PIDs.

---

### F-073 · Cube Controls PIDs have zero external evidence (Low · **Resolved**)

**Encountered:** Wave 34 (2025-07)

Duplicate of F-059. **Resolved (PR #24, 2025-07):** Fabricated PIDs removed from FFB dispatch. See F-059.

---

### F-074 · Snapshot acceptance workflow requires manual review for large diffs (Low · Noted)

**Encountered:** Wave 15 RC hardening (2025-07)

When `insta` snapshots change across many crates at once (e.g., schema version bumps or telemetry field promotions), the review-and-accept workflow (`cargo insta review`) requires per-snapshot confirmation. With 936 snapshot files across 37 directories, bulk changes produce very large pending-snapshot diffs that are tedious to accept individually.

**Remedy:** Use `cargo insta accept --all` for trusted bulk changes after verifying a representative sample. Consider adding a CI step that auto-accepts snapshots on feature branches when the diff is schema-version-only.

---

### F-075 · 52 tests ignored in full workspace run (Low · Noted)

**Encountered:** Wave 15 RC hardening (2025-07)

Running `cargo test --workspace --all-features --exclude racing-wheel-ui` shows 52 ignored tests (across integration-tests and platform-specific modules). These are intentionally `#[ignore]`-gated tests requiring hardware or platform-specific resources (e.g., real USB devices, macOS-only APIs), but the count should be tracked to ensure it doesn't grow silently.

**Remedy:** Periodically audit ignored tests. Consider adding a CI job that lists `#[ignore]` tests and fails if the count exceeds a threshold.

---

### F-076 · Package name discovery — crate names don't match directory names (Low · Open)

**Encountered:** Wave 22-24 (2025-07)

Rust crate names (in `Cargo.toml` `[package] name`) frequently differ from their directory names (e.g., directory `crates/engine` → crate `racing-wheel-engine`, directory `crates/plugins` → crate `racing-wheel-plugins`). This forces developers and agents to run `cargo metadata` or inspect each `Cargo.toml` to discover the correct `--package` flag for `cargo test`, `cargo clippy`, etc. The mismatch is especially confusing when `--exclude` flags reference the crate name, not the path.

**Remedy:** Add a lookup table in `docs/DEVELOPMENT.md` mapping directory names to crate names. Long term: consider aligning directory and crate names where semver allows.

---

### F-077 · Transient proptest timeout failures (Low · **In Progress**)

**Encountered:** Wave 23-24 (2025-07)

Proptest suites occasionally time out on CI runners under heavy parallel load (especially Windows). The default `PROPTEST_MAX_SHRINK_ITERS` and per-test timeout interact poorly when many proptest files run concurrently. Failures are non-deterministic and disappear on retry.

**Remedy:** Set explicit `ProptestConfig { timeout: ... }` in flaky suites. Consider adding `PROPTEST_CASES` environment variable override in CI to reduce case count on slow runners. Document retry expectations in `docs/DEVELOPMENT.md`.

**Update:** PR being prepared to add explicit timeout configs to all 1000-case suites.

---

### F-079 · Linux packages missing hwdb and modprobe quirks files (Medium · **Resolved**)

**Encountered:** Wave 35 (2025-07)

Linux deb/rpm/tarball packages did not include the hwdb joystick classification file or kernel quirks for `ALWAYS_POLL` devices, preventing full plug-and-play on installed packages. Users installing from packages would not get automatic device detection without manually copying hwdb and modprobe configuration files.

**Resolved (PR #82):** Packaging scripts updated to include hwdb joystick classification file and modprobe quirks for `ALWAYS_POLL` devices in deb, rpm, and tarball outputs.

---

### F-080 · Documentation device/game counts stale (Low · **Resolved**)

**Encountered:** Wave 35 (2025-07)

README, SETUP, USER_GUIDE, and DEVICE_SUPPORT docs showed outdated counts (25+ vendors, 50+ games) when actual numbers are 28 vendors, 150+ devices, 60+ games. Stale counts underrepresent the project's device and game coverage.

**Resolved (PR #81):** All documentation updated to reflect current counts: 28 vendors, 150+ devices, 60+ games.

---

### F-081 · UI Isolation Build fails on CI infrastructure timeout (Low · Open)

**Encountered:** Wave 35 (2025-07)

`rustup` download from `static.rust-lang.org` occasionally times out on Windows CI runners, causing UI Isolation Build to fail with "operation timed out". Not a code issue — retry resolves it.

**Remedy:** Add retry logic to the CI workflow step that installs Rust toolchain, or increase the default timeout for `rustup` downloads. Low priority since retries resolve the issue.

---

### F-078 · trybuild stderr matching fragility across Rust versions (Low · Open)

**Encountered:** Wave 24 (2025-07)

`trybuild` compile-fail tests compare exact stderr output against `.stderr` snapshot files. When the Rust compiler version changes (e.g., nightly → stable, or minor version bumps), error message wording, spans, and suggestion text change, causing spurious test failures. This requires regenerating `.stderr` files after every toolchain update.

**Remedy:** Pin `rust-toolchain.toml` to a specific stable version (already done). Consider using `trybuild`'s `#[trybuild::ignore]` or `compile_fail` doc-test attributes for tests where exact stderr matching is unnecessary. Document the `.stderr` regeneration workflow (`TRYBUILD=overwrite cargo test`) in `docs/DEVELOPMENT.md`.

---

## Resolved (archive)

| ID | Title | Resolved In |
|----|-------|-------------|
| F-003 | Agent file-edit race during compilation | PR #19 merge (d6fba74) |
| F-004 | Windows linker PDB limit in integration tests | PR #19 merge (d6fba74) |
| F-006 | Snapshot tests silently encoding wrong values | PR #19 merge (d6fba74) |
| F-007 | Symbol renames cascade (sequence→frame_seq complete) | PR #19 merge (d6fba74) |
| F-008 | BeamNG gear overflow | PR #19 merge (d6fba74) |
| F-009 | static_mut_refs missing | PR #19 merge (d6fba74) |
| F-010 | Stale integration test name | PR #19 merge (d6fba74) |
| F-011 | Linux emit_rt_event borrow error | PR #19 merge (d6fba74) |
| F-013 | No developer sync tool for game support matrix | PR #19 merge (d6fba74) |
| F-014 | Agent race conditions on shared branch | PR #19 merge (d6fba74) |
| F-015 | Workspace-hack requires manual regeneration | PR #19 merge (d6fba74) |
| F-016 | bench_results.json generation undocumented | PR #19 merge (d6fba74) |
| F-017 | `cargo tree --duplicates` CI check too strict | PR #19 merge (d6fba74) |
| F-018 | `fuzz_simplemotion` missing dep in fuzz/Cargo.toml | PR #19 merge (d6fba74) |
| F-019 | 6 SimHub adapters returned empty stub telemetry | PR #19 merge (d6fba74) |
| F-026 | Codemasters Mode 1 UDP adapters wrong byte offsets | PR #19 merge (d6fba74) |
| F-027 | Forza tire temp assumed Kelvin, actually Fahrenheit | PR #19 merge (d6fba74) |
| F-028 | fuel_percent × 100 bug in LFS, AMS1, RaceRoom f64 | PR #19 merge (d6fba74) |
| F-030 | Assetto Corsa adapter used OutGauge instead of Remote Telemetry | PR #19 merge (d6fba74) |
| F-031 | Simagic M10/Simucube 1 PID collision at 0x0D5A | PR #19 merge (d6fba74) |
| F-032 | Estimated PIDs for unreleased Simagic devices | PR #19 merge (d6fba74) |
| F-033 | Simucube Wireless Wheel PID unconfirmed | PR #19 merge (d6fba74) |
| F-034 | Shared USB VIDs require PID-based runtime disambiguation | PR #19 merge (d6fba74) |
| F-035 | PCars2 adapter rewritten to correct SMS UDP v2 format | PR #19 merge (d6fba74) |
| F-036 | Leo Bodnar PID 0xBEEF confirmed as placeholder | PR #19 merge (d6fba74) |
| F-037 | OpenFFBoard PID 0xFFB1 absent from all sources | PR #19 merge (d6fba74) |
| F-038 | Cube Controls PIDs 0x0C73–0x0C75 unverifiable | PR #19 merge (d6fba74) |
| F-039 | VRS DirectForce Pro PID 0xA355 confirmed via linux-steering-wheels | PR #19 merge (d6fba74) |
| F-040 | 100% telemetry adapter snapshot test coverage (56/56 adapters) | PR #19 merge (d6fba74) |
| F-041 | 126 unwrap/expect calls eliminated from 8 test files | PR #19 merge (d6fba74) |
| F-042 | Asetek Tony Kanaan torque corrected 18→27 Nm + 8 proptest properties | PR #19 merge (d6fba74) |
| F-051 | Leo Bodnar PID 0xBEEF placeholder replaced with 0x1301 | Wave 15 RC hardening |
| F-055 | 0 unwrap/expect remaining in test files | Wave 16 |
| F-065 | GT Sport ports were swapped (33739→33340, 33740→33339) | Wave 15 RC hardening |
| F-069 | deny.toml broken with cargo-deny 0.19+ | Wave 15 RC hardening |
| F-070 | TelemetryBuffer mutex unwrap panics | Wave 15 RC hardening |
| F-064 | GT7 extended packet types (316/344 bytes) | Wave 31-32 |
| F-071 | CI workflows lacked timeout-minutes | Wave 15 RC hardening |
| F-072 | PXN V10/V12/GT987 protocol crate added | Wave 31-32 |
| F-052 | OpenFFBoard PID 0xFFB1 confirmed speculative | Wave 34 |
| F-025 | Windows PowerShell shell sessions in agent env | Won't Fix |
| F-029 | cargo-udeps false positives in CI | Wave 16 |
| F-079 | Linux packages missing hwdb and modprobe quirks files | PR #82 |
| F-080 | Documentation device/game counts stale | PR #81 |

---

## Recent Progress

### Wave 35 — Packaging, Docs, CI Hardening (2025-07)
- **F-053 (macOS CI):** Now in progress — PR #84 adds macOS to CI matrix for CLI, Service, and Workspace builds.
- **F-077 (Proptest timeouts):** Now in progress — PR being prepared to add explicit timeout configs to all 1000-case suites.
- **F-079 (Linux packaging):** Resolved (PR #82) — hwdb joystick classification and modprobe quirks for `ALWAYS_POLL` devices now included in deb/rpm/tarball packages.
- **F-080 (Doc counts):** Resolved (PR #81) — README, SETUP, USER_GUIDE, DEVICE_SUPPORT updated to 28 vendors, 150+ devices, 60+ games.
- **F-081 (CI rustup timeout):** Logged as open/low — intermittent `rustup` download timeout on Windows CI runners; retry resolves.

### Waves 22-24 — Golden Packets, Safety Soak, Compile-Fail, Doc-Tests (2025-07)
- **942 new tests** added (13,075 → 14,017+ passing), plus 4 new fuzz targets (100+ total).
- **Wave 22 — engine/service deep testing**: Engine device/game integration tests, IPC snapshot round-trip verification, service lifecycle tests (startup/shutdown/restart/error recovery), error exhaustiveness (all error variants exercised).
- **Wave 23 — golden packets & safety soak**: Golden-packet integration tests for 6 telemetry adapters (end-to-end validation against known-good captures). Safety soak: 10K-tick sustained operation under fault injection for interlock and watchdog subsystems. Plugin security hardening tests (WASM sandbox escape, native plugin isolation, capability enforcement). Schema evolution tests (forward/backward compatibility). CLI/profile deep tests.
- **Wave 24 — compile-fail, config/firmware, atomic, scheduler, doc-tests**: Trybuild compile-fail tests enforcing type-safety invariants at API boundaries. Config and firmware-update deep tests (validation, migration, rollback). Atomic stress tests (concurrent access, ordering guarantees). Scheduler deep tests (priority inversion, deadline miss, RT timing edges). Doc-tests for public API examples. 4 new fuzz targets.
- **New friction points**: F-076 (crate name vs directory name mismatch), F-077 (transient proptest timeouts), F-078 (trybuild stderr fragility across Rust versions).

### Waves 19-20 — Deep Test Coverage Expansion (2025-07)
- **2,676 new tests** added (12,754 → 13,075 passing; 52 ignored), plus 1 new fuzz target (96 total) and 38 new snapshot files (977 total).
- **Deep protocol tests**: Fanatec (70), Logitech (69), Thrustmaster (83), Simagic (comprehensive), Moza (61), OpenFFBoard (53), Cammus/VRS/PXN/FFBeast/Asetek/AccuForce/Simucube (comprehensive).
- **Property tests expanded**: scheduler, watchdog, hardware-watchdog, FMEA, IPC, service, compat, tracing, firmware-update, config, rate-limiter, curves, crypto, WASM runtime, native plugin, plugin ABI, profile, calibration, FFB, pipeline.
- **Telemetry property tests**: MudRunner, Rennsport, SimHub, KartKraft, RaceRoom, WRC, LFS, Forza, orchestrator, recorder.
- **Foundation tests**: schemas (86), CLI (~75), service (74), config validation (51), E2E telemetry pipeline, device matrix (36), filter pipeline, HBP/wheelbase-report/KS/input-maps, SM-V2 protocol deepening.
- **Integration tests**: device lifecycle, multi-vendor dispatch, safety E2E, schema properties, atomic stress, profile-repo, telemetry-integration.
- **Diagnostics**: insta snapshot tests added for `openracing-diagnostic` crate.
- **AMS2 fuzz target** added; seed corpus created for all 96 fuzz targets.

### Wave 34 — PID Verification & Documentation Update (2025-07)
- **Fanatec GT DD Pro/ClubSport DD (F-068):** Confirmed to share PID `0x0020` with CSL DD in PC mode. Previously listed PIDs (`0x0024`, `0x01E9`) may be console-mode or firmware-variant.
- **OpenFFBoard PID 0xFFB1 resolved (F-052):** Confirmed SPECULATIVE — zero evidence across 5 independent sources.
- **Cube Controls PIDs (F-073):** Elevated to standalone friction entry — zero external evidence across 8 sources; OpenFlight uses different estimates.
- **VRS DFP V2 PID 0xA356:** Remains unverified; DFP uses `0xA355` (kernel mainline), Pedals use `0xA3BE`.
- **All 17 vendor protocol crates** now wired into engine dispatch (PXN added).
- **Test count:** 13,075 and growing.
- **Fuzz targets:** 96 covering all protocol parsers and telemetry decoders.
- **Snapshot files:** 977 across 38 directories.
- **Integration test files:** 42.
- **Shell instability (F-025):** Closed as Won't Fix — external agent-environment issue.

### Waves 31-32 — PXN Protocol & GT7 Extended Packets (2025-07)
- **PXN protocol crate added (F-072):** `hid-pxn-protocol` crate created with VID/PIDs web-verified against Linux kernel `hid-ids.h` (VID `0x11FF`, V10 `0x3245`, V12 `0x1212`, GT987). Full proptest/snapshot coverage.
- **GT7 extended packets resolved (F-064):** 316-byte and 344-byte extended packet support implemented in `gran_turismo_7.rs`. All three packet types (PacketType1/2/3) now supported.
- **266+ new tests:** Coverage expanded across compat, F1, RaceRoom, WRC, Rennsport, KartKraft, MudRunner, and SimHub adapters.
- **Test count:** 7,813 tests passing.

### Wave 15 RC Hardening — Verification & Fixes (2025-07)
Comprehensive hardening pass covering telemetry ports, PID evidence gaps, CI reliability, and runtime safety.

- **GT Sport ports fixed (F-065):** Receive and send ports were swapped (33739↔33340). Corrected to recv=33340, send=33339 per Nenkai/PDTools and SimHub wiki.
- **Heusinkveld PID evidence gaps (F-066, F-067):** Pro PID 0xF6D3 has zero external evidence (estimated from sequential pattern). Sprint/Ultimate+ PIDs have only single-source evidence (OpenFlight). All flagged as provisional.
- **Fanatec PID evidence gaps (F-068):** GT DD Pro (0x0024) and ClubSport DD (0x01E9) have zero external confirmation outside internal USB captures. Linux kernel `hid-fanatec.c` does not include these PIDs.
- **deny.toml fixed (F-069):** Configuration updated for cargo-deny 0.19+ compatibility.
- **TelemetryBuffer safety (F-070):** Mutex unwrap panics replaced with proper error handling.
- **CI timeouts (F-071):** All workflow jobs now have explicit `timeout-minutes` values to prevent runaway builds.
- **Existing items re-verified:** F-052 (OpenFFBoard 0xFFB1), F-057 (VRS DFP V2 0xA356), F-059 (Cube Controls PIDs) — all confirmed unchanged, no new evidence found. F-064 (GT7 extended packets) now resolved.

### Wave 18 — Telemetry Protocol Verification (2025-07)
Web-sourced verification of 5 game telemetry adapter protocols against authoritative references.

- **Gran Turismo 7**: All field offsets, encryption (Salsa20, key, XOR `0xDEADBEAF`, nonce derivation), ports (recv 33740 / send 33739), packet size (296 bytes), flags bitmask, and gear encoding verified correct against [Nenkai/PDTools](https://github.com/Nenkai/PDTools) `SimulatorPacket.cs` + `SimulatorInterfaceCryptorGT7.cs`. Enhancement opportunity identified: GT7 ≥ 1.42 supports 316-byte and 344-byte extended packets with wheel rotation, sway/heave/surge, and energy recovery (F-064).
- **rFactor 2**: Shared memory names (`$rFactor2SMMP_Telemetry$`, `$rFactor2SMMP_Scoring$`, `$rFactor2SMMP_ForceFeedback$`), `rF2VehicleTelemetry` field order, `rF2GamePhase` enum (0–8 + 9=paused), wheel fields, gear convention, and speed derivation all re-confirmed against [TheIronWolfModding/rF2SharedMemoryMapPlugin](https://github.com/TheIronWolfModding/rF2SharedMemoryMapPlugin) `rF2State.h`. New electric motor fields (`mBatteryChargeFraction`, `mElectricBoostMotor*`) noted in rF2State.h but not yet exposed. No code changes needed.
- **iRacing**: Transport (`Local\IRSDKMemMapFileName`), data-valid event (`Local\IRSDKDataValidEvent`), header layout (all 10 fields at correct offsets), VarBuffer (16 bytes), VarHeader (144 bytes), variable type IDs (char=0..double=5), session flags (checkered/green/yellow/red/blue), field names and units all verified against [kutu/pyirsdk](https://github.com/kutu/pyirsdk) v1.3.5. No discrepancies found.
- **ACC**: UDP port 9000, protocol version 4, all 7 message types, registration packet format, gear encoding (wire 0=R, 1=N, 2=1st with −1 offset), and readonly flag semantics re-confirmed against Kunos ACC Broadcasting SDK. No changes needed.
- **Codemasters/EA F1**: Default port 20777, F1 25 packet format 2025, 29-byte header, packet IDs (1=Session, 6=CarTelemetry, 7=CarStatus), CarTelemetryData (60 bytes/car), CarStatusData (55 bytes/car), NUM_CARS=22, ERS max 4 MJ — all verified. No discrepancies found.

### Wave 17 — E2E Protocol Coverage & PID Expansion (2025-06)
- **E2E integration tests**: All 16 HID protocol crates now have dedicated E2E test files (224 new tests total)
  - New test files: asetek_e2e.rs (15), cammus_e2e.rs (17), vrs_e2e.rs (18), simucube_e2e.rs (46), heusinkveld_e2e.rs (36), button_box_e2e.rs (34), accuforce_e2e.rs (20), cube_controls_e2e.rs (18), leo_bodnar_e2e.rs (20)
- **New vendors added**: FlashFire (VID 0x2F24, PID 0x010D — 900R) and Guillemot (VID 0x06F8, PID 0x0004 — legacy FFRW) from oversteer wheel_ids.py
- **Logitech**: WingMan Formula Force (0xC291) added from Linux kernel hid-ids.h
- **Thrustmaster**: T80 Ferrari 488 GTB (0xB66A), TX Racing original PID (0xB664) added from oversteer
  - TX protocol confirmed: uses T300RS FFB API, max 900° rotation, 140-900° clamping (hid-tmff2 src/tmtx/hid-tmtx.c)
  - 0xB65D comment corrected: generic pre-init PID for ALL TM wheels (not just T150)
- **Protocol crate updates**: TX_RACING_ORIG (0xB664) and T80_FERRARI_488 (0xB66A) added to hid-thrustmaster-protocol with cross-reference tests
- **SOURCES.md**: FlashFire, Guillemot, WingMan FF, T80H, TX sections added; VID collision map updated
- **Test suite**: 6442 tests passing, clippy clean, 77 fuzz targets, 88 snapshot files

### Wave 16 — Protocol Verification & Test Hardening (2025-06)
- **Protocol verification**: 6 vendors (VRS, Heusinkveld, Cube Controls, Cammus, Leo Bodnar, AccuForce) re-audited
- **Test unwraps eliminated**: 0 `unwrap()`/`expect()` calls remaining across all test files (F-055 resolved)
- **VRS PID updates**: Pedals V1 PID migration `0xA357` → `0xA3BE` identified (F-056); DFP V2 PID `0xA356` unverified (F-057)
- **Cammus pedal PIDs**: new PIDs identified, pending engine dispatch wiring (F-060)
- **cargo-udeps CI fix**: false positives addressed in dependency governance job (F-029)
- **Unverified PIDs flagged**: Heusinkveld (F-058), Cube Controls (F-059), VRS DFP V2 (F-057)

### Protocol Verification Wave (Web-Verified)
- **Moza Racing**: All 11 wheelbase PIDs verified against JacKeTUs/universal-pidff (Linux kernel 6.15). All torque specs confirmed from mozaracing.com. FFB quirks correct. No changes needed.
- **Simucube**: SC2 Sport torque corrected 15→17 Nm, SC2 Ultimate 35→32 Nm (from official docs). Added Simucube 1 PID 0x0D5A. SC-Link Hub PID corrected 0x0D62→0x0D66.
- **Simagic**: EVO Sport 15→9 Nm, EVO 20→12 Nm, EVO Pro 30→18 Nm (from simagic.com). Removed ghost M10/FX entries.
- **Assetto Corsa**: Complete rewrite from OutGauge (76 bytes) to Remote Telemetry UDP (328 bytes) with 3-step handshake. All field offsets corrected.
- **ACC**: Fixed isReadonly field inversion (byte==0 means readonly in Kunos SDK).
- **BeamNG**: Verified correct (OutGauge protocol matches InSim.txt).

### Protocol Verification Wave 2 — Cammus / FFBeast / PXN (Web-Verified)
- **Cammus**: VID `0x3416`, C5 PID `0x0301`, C12 PID `0x0302` — all confirmed against Linux kernel `hid-ids.h` (`USB_VENDOR_ID_CAMMUS`), `hid-universal-pidff.c`, and JacKeTUs/linux-steering-wheels (Platinum support). Torque values C5=5 Nm, C12=12 Nm unchanged. No code changes needed.
- **FFBeast**: VID `0x045B`, Joystick PID `0x58F9`, Rudder PID `0x5968`, Wheel PID `0x59D7` — all confirmed against Linux kernel `hid-ids.h` (`USB_VENDOR_ID_FFBEAST`), `hid-universal-pidff.c`, FFBeast C/C++ API reference (`USB_VID=1115`, `WHEEL_PID_FS=22999`), and JacKeTUs/linux-steering-wheels. Protocol uses ±10000 signed 16-bit torque scale. Dead links fixed: `HF-Robotics/FFBeast` repo (404) and `ffbeast.com` (domain for sale) replaced with `ffbeast.github.io`.
- **PXN**: No `hid-pxn-protocol` crate exists in this branch (was on `feat/r6-pxn-v2`). Linux kernel confirms VID `0x11FF` (`USB_VENDOR_ID_LITE_STAR`), PIDs: V10=`0x3245`, V12=`0x1212`, V12 Lite=`0x1112`/`0x1211`. No V9 PID found in kernel or community sources. PXN uses `HID_PIDFF_QUIRK_PERIODIC_SINE_ONLY` quirk in `hid-universal-pidff`. Torque specs not verified — PXN official site does not publish peak Nm values.

### Protocol Verification Wave 3 — Full Vendor Sweep (Web-Verified)
- **Asetek**: Invicta torque corrected 18→12 Nm, Forte corrected 25→18 Nm, Tony Kanaan corrected 25→27 Nm (from asetek.com spec sheets and JacKeTUs/universal-pidff).
- **rFactor 2**: Adapter completely rewritten from `rF2State.h` (rF2SharedMemoryMap SDK). All shared memory struct offsets verified against the authoritative header. Field mapping corrected for vehicle telemetry, scoring, and extended data.
- **Simucube**: SC2 Sport torque corrected 15→17 Nm, SC2 Ultimate torque corrected 35→32 Nm (Granite Devices official specs). Simucube 1 PID `0x0D5A` added and verified.
- **Simagic**: EVO Sport 15→9 Nm, EVO 20→12 Nm, EVO Pro 30→18 Nm (simagic.com). PID collision with Simucube at `0x0483:0x0522` resolved via `iProduct` string disambiguation.
- **Thrustmaster**: T500 RS PID `0xB677` corrected — was mislabeled as T150 Pro per linux-hardware.org and devicehunt.com. T-GT and T-GT II PIDs confirmed unknown (T-GT II reuses T300 PIDs per hid-tmff2 README).
- **Moza Racing**: All 11 wheelbase PIDs re-confirmed correct against JacKeTUs/universal-pidff and mozaracing.com. No changes needed.
- **Cammus**: VID `0x3416`, C5 `0x0301`, C12 `0x0302` — all confirmed correct against Linux kernel `hid-ids.h`. No changes needed.
- **FFBeast**: Dead links (`HF-Robotics/FFBeast` repo 404, `ffbeast.com` domain for sale) replaced with `ffbeast.github.io`. PIDs confirmed against Linux kernel `hid-ids.h`.
- **Cube Controls**: Reclassified as button boxes (input-only, non-FFB). PIDs remain provisional/unconfirmed pending hardware capture.
- **Leo Bodnar**: VID `0x1DD2` confirmed via USB VID registry (the-sz.com). SLI-M PID `0xBEEF` flagged as placeholder — not found in any public USB database.
- **AccuForce**: PID `0x804C` confirmed (NXP VID `0x1FC9`). V1 vs V2 torque differences documented (V1=7 Nm, V2=12 Nm).
- **OpenFFBoard**: Main PID `0xFFB0` confirmed via pid.codes registry. Alt PID `0xFFB1` remains unverified (no independent source).
- **Heusinkveld**: VID updated from `0x16D0` to `0x04D8` (Microchip Technology); PIDs updated to `0xF6Dx` range per OpenFlight cross-reference.
- **VRS DirectForce**: VID `0x0483` confirmed (STMicroelectronics generic). VID collision with Simagic legacy documented and resolved via `iProduct` string.
- **Assetto Corsa**: Complete rewrite from OutGauge (76 bytes) to Remote Telemetry UDP (328 bytes) with 3-step handshake.
- **ACC**: Fixed `isReadonly` field inversion (byte==0 means readonly in Kunos SDK).

### Engine Device Table Sync
- 50+ missing devices added to linux.rs (VRS, Heusinkveld, Cammus, OpenFFBoard, FFBeast, etc.)
- AccuForce Pro capabilities corrected (12 Nm, PID support, 1 kHz)
- Cube Controls capabilities corrected (input-only devices, torque set to 0 Nm; PIDs still unconfirmed)
- Asetek Tony Kanaan torque corrected (25→20 Nm)

### RC Cleanup Sprint (2025-06)
- **ACC gear offset**: Corrected -2 → -1, web-verified against ACC broadcasting protocol v4 (F-043)
- **Safety panic fix**: `SafetyService::get_max_torque()` NaN panic replaced with zero-torque fallback (F-044)
- **F1 UDP parser**: `unreachable!()` on untrusted input replaced with proper `Err()` (F-045)
- **CI reproducibility**: 33 cargo commands across 7 workflows now use `--locked` (F-046)
- **Nightly soak test**: Extracted from silently-ignored YAML document to standalone workflow (F-047)
- **Fanatec PID fix**: `0x0E03` corrected from ClubSport V1 to CSL Elite (F-048)
- **CI modernization**: `actions-rs/toolchain@v1` → `dtolnay/rust-toolchain@stable` (F-049), `actions/cache@v3` → v4 (F-050)
- **PCars2**: Adapter completely rewritten from fabricated offsets to correct SMS UDP v2 format (F-035)
- **Telemetry snapshot tests**: 100% coverage achieved — 56/56 adapters have snapshot tests (F-040)
- **Test quality**: 126 `unwrap()`/`expect()` calls eliminated from 8 test files (F-041)
- **Asetek Tony Kanaan**: Torque corrected 18→27 Nm; 8 proptest property tests added (F-042)
- **VRS DirectForce Pro**: PID `0xA355` independently confirmed via linux-steering-wheels (F-039)
- **Device PID audit**: Leo Bodnar `0xBEEF` (F-036), OpenFFBoard `0xFFB1` (F-037), Cube Controls `0x0C73`–`0x0C75` (F-038) flagged as unverifiable — all need hardware captures

### RC Hardening Wave 15+ (2026-03)
- **DFP Range Encoding**: Critical bug fixed — old code produced identical output for ALL degree values. Rewritten to match kernel `lg4ff_set_range_dfp()` two-command sequence (coarse + fine limit). Source: `linux/drivers/hid/hid-lg4ff.c`.
- **Simucube Protocol**: HID joystick report parser implemented (u16 steering, 6 axes, 128 buttons). Bootloader PIDs added (0x0D5E, 0x0D5B). Source: official Simucube developer docs + Granite Devices wiki. Resolves F-061.
- **Heusinkveld VID/PID Correction**: VID updated `0x16D0` → `0x04D8` (Microchip Technology); PIDs corrected to `0xF6Dx` range. Source: OpenFlight cross-reference.
- **VID Collisions**: Full documentation created (`docs/protocols/VID_COLLISIONS.md`) + 14 dispatch verification tests. No VID+PID duplicates across 130+ entries.
- **Mutation Testing**: Targeted mutation-killing tests added for Fanatec, Logitech, Thrustmaster, and filters crates.
- **Snapshot Encoding Tests**: Added for FFBeast (12 tests) and Leo Bodnar (8 tests) — all protocol crates now have snapshot coverage.
- **Protocol Verification**: All VID/PIDs re-verified against web sources (kernel hid-ids.h, linux-steering-wheels, pid.codes, devicehunt). No corrections needed beyond Heusinkveld.
- **CI Fixes**: cargo-udeps false positives resolved for 8 crates; deprecated field detection false positive fixed (TelemetryFrame seq field ≠ removed TelemetryData seq field).
- **Test Count**: 13,075 tests passing, 0 failures, 52 ignored across 82 workspace crates (526 test binaries).
- **Lesser-documented device web verification (2025-07):**
  - AccuForce: VID 0x1FC9 / PID 0x804C confirmed (Platinum, hid-pidff). pid.codes 0x1209/0x0001 is test-only PID.
  - VRS DFP: PID 0xA355 confirmed (Platinum, hid-universal-pidff). DFP V2 PID 0xA356 still unverified in kernel/community (F-057).
  - VRS now branded "Turtle Beach VRS" in linux-steering-wheels.
  - OpenFFBoard: PID 0xFFB0 confirmed (Platinum, hid-pidff). PID 0xFFB1 has zero evidence across 5 sources (F-052).
  - Cube Controls: PIDs 0x0C73–0x0C75 still zero external evidence across 8 sources. OpenFlight uses different estimates (F-059).

### Earlier Progress
- Project CARS 3 adapter added
- Codemasters shared parsing extracted into `codemasters_shared.rs` (~890 lines of duplicated offset logic removed)
- Forza Horizon 4 324-byte packet support added
- Cube Controls protocol tests added (46 tests)
- TODO comments cleaned up across engine, CLI, and diagnostics crates
- Portable shebangs (`#!/usr/bin/env bash`) applied to shell scripts

---

## Process notes

- **Review cadence:** Check open items at the start of each sprint / major feature push.
- **Adding entries:** When you hit a friction point, add it here before moving on. Don't wait until retrospective.
- **Closing entries:** Mark **Resolved** once the fix lands in `main`; move to the archive table.
- **Escalation:** High-severity open items that block RC should be added to `ROADMAP.md` as concrete work items.
