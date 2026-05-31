# OR-PROP-0002: Quality closure lane

Status: proposed
Owner: release/ci
Created: 2026-05-31
Target milestone: n/a
Linked specs: docs/specs/OR-SPEC-0003-ripr-plus-coverage-closure.md
Linked ADRs: n/a
Linked plan: plans/quality-closure/implementation-plan.md
Support/status impact: no public support claim
Policy impact: policy/quality-closure-exceptions.toml

## Problem

RIPR and code coverage are useful only when reviewers can tell what they mean.
Recent pull-request rollups showed RIPR passing while the Code Coverage workflow
could be skipped. That makes the quality state hard to audit: a skipped coverage
job can look like a harmless green check even though no coverage evidence was
collected for that PR.

The repo needs a quality closure lane that measures owned gaps before it tries
to add broad tests. The first problem is not low line coverage; it is the lack
of one receipt that says which quality surfaces passed, failed, skipped, stayed
advisory, or were intentionally not applicable.

## Users and surfaces

This lane is for maintainers and PR reviewers. It covers:

- `ripr+` repo-scope gap state;
- PR RIPR evidence;
- Codecov project and patch coverage status;
- coverage workflow execution state;
- policy-owned coverage exceptions;
- follow-up PR routing for test coverage gaps.

## Success criteria

- RIPR+ zero has a repo-owned definition.
- Coverage closure has a repo-owned definition.
- Coverage skipped is visible as not closure-satisfied.
- Patch coverage status is not confused with a required hard gate while it is
  informational.
- Generated, hardware-only, unreachable, and intentionally advisory surfaces
  have owner, reason, review date, and removal condition.
- The lane produces a machine-readable receipt suitable for CI artifacts.

## Proposed shape

Add a source-of-truth spec, a small implementation plan, an owned exception
ledger, and an `xtask quality-closure` receipt generator. The generator reads
existing repo state rather than running coverage or mutating tests.

## Alternatives considered

- Require 100 percent line coverage immediately. Rejected because it would
  reward weak tests and fake confidence.
- Treat Codecov informational status as closure. Rejected because advisory
  status is not a hard gate.
- Keep relying on workflow names in GitHub's check rollup. Rejected because a
  skipped workflow is not proof.

## Specs to create or update

- `docs/specs/OR-SPEC-0003-ripr-plus-coverage-closure.md`

## ADRs needed

n/a

## Implementation campaign shape

1. Define RIPR+ zero and coverage closure.
2. Generate a quality closure receipt from existing workflow and policy state.
3. Turn skipped coverage into explicit debt.
4. Close owned gaps by test surface, starting with protocol/domain logic.
5. Ratchet patch coverage and crate/module coverage as evidence stabilizes.

## Evidence plan

The first PR proves only the measurement rail:

```bash
cargo xtask quality-closure --check
```

Later PRs can make the receipt required or make additional fields hard gates.

## Risks

- The receipt could become another advisory artifact that nobody reads.
- Exceptions could become permanent debt if review dates are ignored.
- Coverage could be gamed with tests that execute code without useful
  assertions.

## Non-goals

- No hardware work.
- No forced 100 percent line coverage claim.
- No broad test-writing campaign in the scaffold PR.
- No mutation-testing expansion unless routed by existing policy.

## Exit criteria

The proposal is complete when the quality closure receipt reports:

```text
ripr_unresolved_gap_count = 0
ripr_plus_unowned_gap_count = 0
coverage_required = true
coverage_workflow_skipped = false
patch_coverage_status = pass
uncovered_owned_surface_count = 0
```

## Claim boundary

This proposal does not claim correctness, release readiness, hardware readiness,
simulator readiness, or full line coverage. It creates the auditable denominator
needed to drive those claims responsibly later.
