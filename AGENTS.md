# AGENTS.md

This file guides automated agents working in this repository. Follow it alongside `docs/DEVELOPMENT.md`.

## Project summary
- OpenRacing is a Rust workspace for safety-critical, real-time force feedback.
- The real-time (RT) path runs at 1kHz with strict latency and allocation rules.
- Plugins support both WASM (safe, sandboxed) and native (fast, RT) implementations.

## Key locations
- `crates/engine`: core RT pipeline, safety, diagnostics
- `crates/plugins`: plugin runtime (WASM + native)
- `crates/cli`: CLI tooling
- `crates/service`: background service and integration
- `crates/integration-tests`: integration + acceptance tests
- `docs/`: development, ADRs, and system design

## Must-follow engineering rules
- **No RT allocations** after initialization. Avoid heap usage in RT code paths.
- **No blocking in RT**: no I/O, locks, or syscalls in RT hot paths.
- **No `static mut`**: use `OnceLock`, `LazyLock`, atomics, or other safe patterns.
- Keep execution **bounded and deterministic** in RT code.
- Respect safety interlocks and fault response guarantees.

## Architecture changes
- Significant architectural changes require an ADR. See `docs/adr/README.md`.

## Code style and linting
- Format: `python scripts/cargo_fmt_workspace.py` (avoid `cargo fmt --all` on Windows due to path length limit os error 206)
- Lints: `cargo clippy --all-targets --all-features -- -D warnings`
- Prefer small, readable diffs; keep APIs consistent across crates.

## Testing and validation
- Unit + integration tests: `cargo test --all-features --workspace`
- RT performance profile: `cargo build --profile rt --bin wheeld`
- Benchmarks: `BENCHMARK_JSON_OUTPUT=1 BENCHMARK_JSON_PATH=bench_results.json cargo bench --bench rt_timing`
- Performance gates: `python scripts/validate_performance.py bench_results.json --strict`
- ADR validation: `cargo run -p openracing-tools --bin validate-adr -- --verbose`
- Docs index: `cargo run -p openracing-tools --bin generate-docs-index --`
- Docs build: `cargo doc --all-features --workspace`
 - **No `unwrap()`/`expect()` in tests**: avoid panics in test code; prefer `Result`-returning tests (e.g. `#[test] fn foo() -> Result<(), Error>`), explicit assertions, or test helper macros.

## Multi-agent / worktree rules (F-003, F-014)

When multiple agents operate on the same repository concurrently:

1. **Each agent MUST use its own git worktree.** Never have two agents editing files in the same worktree directory — concurrent file edits during active builds cause cascading compilation errors and merge conflicts.

2. **Create a worktree per feature branch:**
   ```
   git worktree add ../OpenRacing-<feature> -b feat/<feature>
   ```
   Work entirely inside `../OpenRacing-<feature>`. Do not touch the parent worktree.

3. **Push and open a PR from the feature worktree.** Do not commit directly to `main` or to another agent's branch.

4. **After the parent PR merges**, rebase by cherry-picking your commits onto a fresh branch from `main` rather than rebasing the full history (avoids conflicts from squash merges):
   ```
   git checkout -b feat/<feature>-v2 origin/main
   git cherry-pick <commit1> <commit2> ...
   git push origin feat/<feature>-v2
   ```

5. **Workspace-hack drift:** After adding a new crate or changing feature flags, run `cargo hakari generate` and commit the result before pushing.

6. **Pre-commit hook:** Install the local pre-commit hook to catch workspace-hack drift and YAML sync issues before they reach CI:
   ```
   git config core.hooksPath .githooks
   ```
   The hook runs `cargo hakari verify` (if `cargo-hakari` is installed) and diffs the two `game_support_matrix.yaml` files.

## Dependency and config hygiene
- Use workspace dependencies where possible (see root `Cargo.toml`).
- If you add or update dependencies, update `Cargo.lock`.
- Check `deny.toml` for allowed licenses and advisories.

## Platform considerations
- This project targets Windows, Linux, and macOS.
- Avoid OS-specific assumptions unless the module is platform-specific.
- Keep cross-platform code paths behaviorally aligned.

## When editing safety-critical code
- Add or update tests (including fault injection where relevant).
- Validate timing and performance requirements.
- Document behavioral changes in `docs/` and/or ADRs.
