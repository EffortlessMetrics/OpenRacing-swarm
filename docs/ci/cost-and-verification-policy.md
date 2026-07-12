# Cost and verification policy

OpenRacing treats CI as an evidence budget. The goal is not to reduce
verification; the goal is to remove wasted CI so the project can afford more
verification where it matters.

## Principles

- Default PR checks must be deterministic, high-signal, and review-fast.
- Expensive lanes must be routed by changed surface, risk label, main, nightly,
  manual dispatch, or release workflow.
- Runtime proof should be concentrated where it can falsify a real risk.
- Optional lanes must report `skipped-by-policy`, `advisory`, or `failed` rather
  than pretending that a skip is proof.
- Branch protection should prefer a stable aggregate gate over many slow leaf
  checks.

## Default PR evidence

Ordinary PRs should prioritize:

1. formatting and source policy checks;
2. cargo check/clippy for the affected Rust surfaces;
3. targeted tests close to the changed seam;
4. static mutation-exposure review when production Rust behavior changes;
5. unsafe-review evidence when unsafe seams change.

Default PRs should not run full coverage, full mutation, broad Miri, GPU,
Docker, macOS, Windows, or hardware-in-the-loop lanes unless the changed surface
or a label requires them.

## Deep evidence routing

Deep lanes remain available and valuable:

- coverage and Codecov for execution-surface telemetry;
- cargo-mutants for runtime mutation backstops;
- Miri for concrete UB execution witnesses;
- fuzzing and property tests for parser/protocol/input surfaces;
- hardware lanes for device claims;
- release-readiness lanes for public support claims.

These lanes should run on main, nightly, manual dispatch, release, or targeted
risk PRs. Their receipts must state what ran, what was skipped, and what may be
claimed.

## Aggregate gate posture

The preferred branch-protection model is a stable aggregate check such as `PR
Gate Success`. The aggregate gate may depend on fast blocking checks and may
summarize optional lanes as:

- `passed`;
- `failed`;
- `skipped-by-policy`;
- `advisory-failed`.

A skipped optional lane is never equivalent to a pass. It is visible policy
routing.
