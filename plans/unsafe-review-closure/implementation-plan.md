# Unsafe-review closure implementation plan

Status: proposed
Owner: release/ci
Linked proposal: docs/proposals/OR-PROP-0003-unsafe-review-closure-lane.md
Linked specs:
- docs/specs/OR-SPEC-0004-unsafe-review-closure.md
Linked ADRs: n/a
Active goal: n/a; this plan does not replace the active Moza hardware goal

## Current state

The repository has legitimate unsafe seams across platform HID, RT execution,
native plugins, telemetry shared memory, service/platform integration, and
third-party shared-memory support. Those seams are not automatically wrong, but
they are not yet represented by one reviewability receipt.

The receipt also needs to keep aggregate missing-evidence counts attributable to
the active exception entries that own them, so follow-up PRs can target the
largest unsafe-review seams without redefining unsafe-review as soundness proof.

Miri is not treated as a required gate in this scaffold. Missing Miri evidence
is visible as `skipped` or `advisory`; it is not a claim that unsafe Rust is
sound or UB-free.

## Work item: define-unsafe-review-audit-closure

Status: completed
Linked proposal: docs/proposals/OR-PROP-0003-unsafe-review-closure-lane.md
Linked spec: docs/specs/OR-SPEC-0004-unsafe-review-closure.md
Linked ADR: n/a
Blocks: follow-up unsafe-review gap closure PRs
Blocked by: n/a

### Goal

Define unsafe-review closure in source-of-truth docs, add an owned exception
ledger, and generate an unsafe-review receipt that makes missing evidence
explicit without claiming safety.

### Production delta

- Add `policy/unsafe-review-exceptions.toml`.
- Add `cargo xtask unsafe-review-closure [--check]`.
- Emit JSON and Markdown receipts under `target/xtask/unsafe-review-closure/`.
- Add CI artifact workflow coverage for the unsafe-review receipt.
- Document unsafe-review definitions and non-claim boundaries.

### Non-goals

- No hardware work.
- No Pit House, SimHub, USBPcap, PIDFF, firmware/DFU, serial, HID, or motion.
- No live capture.
- No broad refactor.
- No forced unsafe removal.
- No claim that unsafe Rust is sound, UB-free, or Miri-clean.

### Acceptance

- The repo has a documented definition of unsafe-review closure.
- The repo has a documented non-claim boundary: unsafe-review makes unsafe
  reviewable; it does not prove soundness.
- The receipt distinguishes `pass`, `fail`, `advisory`, `skipped`, and
  `not_applicable`.
- Missing unsafe-review evidence is not treated as pass.
- Every initial unsafe exception is explicit, owned, and reviewable.
- The follow-up PR order is documented as:
  1. changed unsafe seams;
  2. FFI / raw pointer / transmute-like seams;
  3. shared-memory / concurrency / RT boundaries;
  4. HID/USB/driver-facing unsafe boundaries;
  5. generated or platform-specific unsafe surfaces;
  6. Miri/property/fake-transport witnesses.

### Proof commands

```bash
python scripts/cargo_fmt_workspace.py
cargo test --locked -p openracing-tools --bin xtask -- --nocapture
cargo xtask unsafe-review-closure --check
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo run --locked -p openracing-tools --bin package-surface -- --check
python scripts/policy_file.py
git diff --check
```

Discovery-only checks for this scaffold:

```bash
cargo miri test
python scripts/policy_no_panic.py
```

Miri is not currently promoted to required unsafe-review evidence by this plan.

### Rollback

Revert the unsafe-review proposal/spec/plan, exception ledger, workflow, and
`xtask unsafe-review-closure` changes. The RIPR+/coverage and Moza hardware
lanes remain unchanged.

## Next work

1. Close changed unsafe seams first.
2. Use `unsafe_exception_breakdown` to add contracts and witnesses for the
   highest-leverage FFI, raw pointer, and transmute-like seams.
3. Add fake-memory/fake-transport witnesses for shared-memory, RT, HID, USB, and
   driver-facing seams.
4. Narrow or remove exception entries as evidence lands.
5. Promote Miri/property checks only where they are cheap, supported, and
   actually run in CI.
