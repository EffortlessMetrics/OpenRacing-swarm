# Quality closure implementation plan

Status: proposed
Owner: release/ci
Linked proposal: docs/proposals/OR-PROP-0002-quality-closure-lane.md
Linked specs:
- docs/specs/OR-SPEC-0003-ripr-plus-coverage-closure.md
Linked ADRs: n/a
Active goal: n/a; this plan does not replace the active Moza hardware goal

## Current state

The repository has a generated `ripr+` badge and PR-scoped RIPR evidence. The
repository also has Codecov configuration and coverage workflows, but PR
coverage can be skipped unless the PR is labeled for coverage/full CI. Codecov
patch coverage is currently informational.

That means the quality lane is not closure-satisfied even when ordinary PR
checks are green.

Local discovery for this scaffold also found three measurable infrastructure
debts:

- `cargo xtask badges --check` depends on a `test-efficiency.json` report, but
  this xtask does not yet expose the referenced generator.
- `scripts/coverage.sh --json` cannot run when the helper's shell environment
  cannot see `cargo-llvm-cov`; missing coverage tooling must stay visible as
  skipped evidence rather than disappearing into a shell helper failure.
- the direct Windows `cargo llvm-cov` workspace path can hit command-line
  length error 206 when the object list becomes too large.

All three are tracked in `policy/quality-closure-exceptions.toml`; none is
treated as a passing coverage, badge, or RIPR+ closure signal.

The receipt also needs to keep active RIPR+/coverage exceptions attributable to
owners and follow-up status, so follow-up PRs can distinguish required gate
debt from generated, advisory, or deferred surfaces without redefining coverage
closure as a line-percentage claim.

Review dates are part of that ownership model. Expired quality exceptions must
remain visible in the receipt rather than silently relying on old exception
metadata.

## Work item: define-ripr-plus-zero-and-coverage-closure-gates

Status: completed
Linked proposal: docs/proposals/OR-PROP-0002-quality-closure-lane.md
Linked spec: docs/specs/OR-SPEC-0003-ripr-plus-coverage-closure.md
Linked ADR: n/a
Blocks: follow-up RIPR+/coverage gap closure PRs
Blocked by: n/a

### Goal

Define RIPR+ zero and coverage closure in source-of-truth docs, add an owned
exception ledger, and generate a quality closure receipt that reports skipped
coverage as not closure-satisfied.

### Production delta

- Add `policy/quality-closure-exceptions.toml`.
- Add `cargo xtask quality-closure [--check]`.
- Emit JSON and Markdown receipts under `target/xtask/quality-closure/`.
- Add CI artifact workflow coverage for the receipt.
- Document the definitions in the proposal, spec, and verification docs.
- Keep the native-plugin SPSC benchmark inside the shared-memory cap when
  `cargo test --workspace --all-targets` executes benchmark harnesses.
- Gate engine HID integration tests to virtual devices by default, with live HID
  available only through `OPENRACING_LIVE_HID_TESTS`.

### Non-goals

- No hardware work.
- No Pit House, SimHub, USBPcap, PIDFF, firmware/DFU, serial, HID, or motion.
- No broad refactor.
- No attempt to fix every coverage gap.
- No weak tests just to improve a percentage.
- No forced 100 percent line coverage claim.

### Acceptance

- The repo has a documented definition of RIPR+ zero.
- The repo has a documented definition of coverage closure.
- The receipt distinguishes `pass`, `fail`, `advisory`, `skipped`, and
  `not_applicable`.
- Code Coverage skipped is not treated as coverage pass.
- Every initial exception is explicit, owned, and reviewable.
- The next PR order is documented as:
  1. core protocol/domain logic;
  2. receipt/schema/verifier logic;
  3. CLI parse/guard rails;
  4. CI/policy/xtask surfaces;
  5. hardware-only seams behind fake transports.

### Proof commands

```bash
python scripts/cargo_fmt_workspace.py
cargo test --locked --workspace --all-targets
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo run --locked -p openracing-tools --bin package-surface -- --check
python scripts/policy_file.py
cargo xtask quality-closure --check
cargo xtask ripr-pr
cargo xtask ripr-pr --check
cargo xtask ripr-review-comments
cargo xtask ripr-review-comments --check
cargo xtask impacted-evidence
git diff --check
```

Discovery-only checks that currently demonstrate owned debt rather than proof:

```bash
cargo xtask badges --check
scripts/coverage.sh --json
```

### Rollback

Revert the quality closure proposal/spec/plan, exception ledger, workflow, and
`xtask quality-closure` changes. The existing RIPR, Codecov, coverage, and Moza
hardware lanes remain unchanged.

## Next work

1. Turn the skipped coverage debt into a required non-skipped coverage sentinel
   or a required patch coverage job.
2. Use `quality_exception_breakdown` to close the highest-leverage owned gaps in
   core protocol/domain logic.
3. Keep quality exception `review_after` dates current or narrow/remove entries
   before they expire.
4. Remove exception entries as durable tests land.
5. Ratchet patch coverage, then crate/module coverage, using the receipt as the
   denominator.
