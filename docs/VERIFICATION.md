# Verification

`OpenRacing` has three verification surfaces:

- README badges are public, repo-scoped trust markers.
- Pull request evidence is diff-scoped reviewer and agent feedback.
- Release evidence is shipped-truth proof for public version handoff.

Badges are the front panel. Generated evidence, CI receipts, and release artifacts remain the source of truth.

## README badges

### `ripr+`

`ripr+` is a repo-scoped static evidence badge. It counts unresolved static exposure gaps plus actionable test-efficiency findings under repository policy.

It is an inbox-zero signal, not coverage, runtime mutation proof, or correctness proof. Diff-scoped `ripr` artifacts belong in pull request summaries and CI artifacts, not public README badges.

### Release

The release badge shows the latest GitHub release. GitHub releases are the public version surface for this repository; crates.io downloads and docs.rs remain registry and documentation surfaces.

## Regeneration

Regenerate public badge endpoints:

```bash
cargo xtask badges
```

Check committed endpoint drift:

```bash
cargo xtask badges --check
```

Both badge commands run `cargo xtask test-efficiency-report` first. The
producer writes the advisory `0.1` test ledger under `target/ripr/reports/`,
after which the native repo-scoped RIPR+ format renders the numeric endpoint.
The quality-closure receipt reports `badge_endpoint_status = "pass"` only when
that generated evidence and the committed endpoint agree. The report remains
static advisory evidence, not runtime mutation, coverage, or closure proof.

Generate the report directly when inspecting its ledger:

```bash
cargo xtask test-efficiency-report
```

Committed endpoint files live under `badges/`. Detailed reports stay under `target/` locally or in CI artifacts.

## Pull Request Evidence

Pull requests run advisory `ripr` evidence, impacted evidence, fast gates, docs-sync, publish preflight, example smoke checks, and targeted mutation when routing rules require it.

`ripr` may suggest focused tests or route targeted mutation. It does not edit code, generate tests, run mutation, or make merge decisions by default.

Pull request artifacts and summaries are diff-scoped. They must not be reused as repo-scope README badges.

## Quality Closure

RIPR+ zero and coverage closure are tracked by the quality closure lane:

- Proposal: `docs/proposals/OR-PROP-0002-quality-closure-lane.md`
- Spec: `docs/specs/OR-SPEC-0003-ripr-plus-coverage-closure.md`
- Plan: `plans/quality-closure/implementation-plan.md`
- Exception ledger: `policy/quality-closure-exceptions.toml`

Generate the current receipt:

```bash
cargo xtask quality-closure --check
```

The receipt distinguishes `pass`, `fail`, `advisory`, `skipped`, and
`not_applicable`. Skipped coverage is not a coverage pass. Informational
Codecov patch status is advisory until a later PR turns it into a hard gate or
replaces it with an equivalent repo-owned patch coverage check.

If a local coverage or RIPR+ generator cannot produce evidence, that is tracked
as closure debt in the exception ledger. It is not treated as a successful gate.

## Unsafe Review Closure

Unsafe-review closure is tracked separately from RIPR+ and coverage:

- Proposal: `docs/proposals/OR-PROP-0003-unsafe-review-closure-lane.md`
- Spec: `docs/specs/OR-SPEC-0004-unsafe-review-closure.md`
- Plan: `plans/unsafe-review-closure/implementation-plan.md`
- Exception ledger: `policy/unsafe-review-exceptions.toml`

Generate the current receipt:

```bash
cargo xtask unsafe-review-closure --check
```

The receipt reports unsafe site inventory, changed unsafe sites, missing local
contracts, missing local guards, missing witnesses, missing owners, expired
reviews, unreviewed unsafe gaps, and Miri status.

Unsafe-review closure makes unsafe seams reviewable. It does not prove unsafe
Rust soundness, UB-freedom, Miri-clean status, release readiness, or hardware
readiness. Skipped Miri evidence is not an unsafe-review pass.
