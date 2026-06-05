# OR-SPEC-0003: RIPR+ and coverage closure

Status: proposed
Owner: release/ci
Created: 2026-05-31
Linked proposal: docs/proposals/OR-PROP-0002-quality-closure-lane.md
Linked ADRs: n/a
Linked plan: plans/quality-closure/implementation-plan.md
Linked issues: n/a
Linked PRs: n/a
Support-tier impact: no public support claim
Policy impact: policy/quality-closure-exceptions.toml

## Terms

`RIPR+ zero` means the repo-scoped `ripr+` unresolved gap count is zero and any
RIPR+ exception is explicitly owned. It is an inbox-zero static evidence signal,
not line coverage, mutation proof, runtime correctness, or release readiness.

`Coverage closure` means every Rust product surface is either:

- covered by a named unit, integration, property, golden, receipt, fake
  transport, or verifier test surface; or
- explicitly excluded in `policy/quality-closure-exceptions.toml` with owner,
  path/module/crate, reason, test surface, review date, and removal condition.

`Owned coverage status` is the controlled target. The lane MUST NOT treat 100
percent line coverage as the goal when assertions are weak, generated code is
counted against humans, or hardware-only branches are faked as covered.

## Required receipt

The quality closure command MUST emit a machine-readable receipt with at least:

```json
{
  "schema_version": 1,
  "lane": "ripr-plus-coverage-closure",
  "status": "pass|fail|advisory",
  "quality_closure_satisfied": false,
  "ripr_unresolved_gap_count": 0,
  "ripr_plus_unowned_gap_count": 0,
  "coverage_required": false,
  "coverage_workflow_skipped": true,
  "coverage_tool_status": "pass|fail|advisory|skipped|not_applicable",
  "patch_coverage_status": "pass|fail|advisory|skipped|not_applicable",
  "uncovered_owned_surface_count": 0,
  "exception_count": 0
}
```

The receipt MUST distinguish these statuses:

- `pass`: the surface produced the required evidence.
- `fail`: the surface is required but not satisfied.
- `advisory`: the surface reported information that is not currently a hard
  gate.
- `skipped`: no evidence was produced, and the skip is not equivalent to pass.
- `not_applicable`: the surface is intentionally out of scope for this receipt.

## RIPR+ requirements

- The repo-scoped `ripr+` badge message is the default unresolved gap count.
- `ripr_unresolved_gap_count` MUST be numeric.
- `ripr_plus_unowned_gap_count` MUST count active exception entries of kind
  `ripr_unowned_gap`.
- RIPR PR artifacts remain diff-scoped and MUST NOT be reused as repo-scope
  closure proof.

## Coverage requirements

- A skipped Code Coverage workflow MUST set `coverage_workflow_skipped = true`.
- Skipped coverage MUST NOT satisfy `coverage_required`.
- Missing local or CI coverage tooling MUST report
  `coverage_tool_status = "skipped"` or `coverage_tool_status = "fail"` and
  MUST NOT be treated as coverage evidence.
- Codecov informational patch coverage MUST report
  `patch_coverage_status = "advisory"`.
- Generated, test-harness, benchmark, fuzz, build-script, UI, integration-test,
  workspace-hack, hardware-only, and unreachable surfaces MUST be represented by
  policy entries before they are excluded from the closure denominator.
- Hardware-only code SHOULD be covered through fake transports or receipt
  validators before any live hardware path is counted as covered.

## Exception ledger

`policy/quality-closure-exceptions.toml` MUST require each active exception to
include:

```toml
id = "stable-id"
owner = "team-or-area"
path = "path/module/crate"
kind = "coverage_gate_debt"
reason = "why this is excluded or not closure-satisfied"
test_surface = ["command or workflow that keeps it visible"]
review_after = "YYYY-MM-DD"
removal_condition = "what removes this exception"
```

Exceptions may be active or retired. Empty owner, reason, test surface, review
date, or removal condition is invalid.

## Follow-up priority

The receipt or plan MUST route follow-up work in this order:

1. core protocol/domain logic;
2. receipt/schema/verifier logic;
3. CLI parse and guard rails;
4. CI, policy, and xtask surfaces;
5. hardware-only seams behind fake transports.

## Non-goals

- No hardware work.
- No broad refactor.
- No tests that only execute code without behavior assertions.
- No forced 100 percent line coverage claim.
- No release-readiness claim.

## Proof

The scaffold proof is:

```bash
cargo xtask quality-closure --check
```

Full validation PRs may add workspace tests, clippy, package-surface, policy,
and coverage/mutation commands as they close specific gaps.
