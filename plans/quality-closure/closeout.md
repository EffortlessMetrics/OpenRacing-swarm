# Quality closure closeout: RIPR+ badge evidence contract

Status: completed
Completed: 2026-08-02
Issue: https://github.com/EffortlessMetrics/OpenRacing-swarm/issues/215
Implementation PR: https://github.com/EffortlessMetrics/OpenRacing-swarm/pull/226
Merge commit: `d12b4ac454712c26321d43808e9b62908414e7b0`
Follow-up: https://github.com/EffortlessMetrics/OpenRacing-swarm/issues/227
Follow-up PR: https://github.com/EffortlessMetrics/OpenRacing-swarm/pull/230

## Landed

- `cargo xtask badges --check` now selects native `repo-badge-plus-shields`
  output when test-efficiency evidence exists.
- When the report is absent, it runs the checked repo-scoped exposure-only
  output, requires a numeric message, and projects it to a `ripr+` endpoint
  with `lightgrey`.
- The committed endpoint is numeric (`69638`) and no longer publishes a
  nonnumeric `needs test-efficiency` message or a false numeric zero.
- `quality-closure --check` continues to report
  `badge_endpoint_status = "skipped"` without test-efficiency evidence.
- The spec, verification guidance, implementation plan, and policy exception
  ledger now carry the same contract and claim boundary.

## Proof

- Local focused `openracing-tools` tests: 16 passed.
- `cargo xtask badges --check`: passed with the exposure-only fallback.
- `OPENRACING_COVERAGE_TOOL_STATUS=skipped cargo xtask quality-closure --check`:
  passed; receipt remained advisory with badge endpoint skipped.
- `python scripts/cargo_fmt_workspace.py`: passed.
- `python scripts/policy_file.py --strict`: passed.
- `cargo hakari verify`: passed.
- `git diff --check`: passed.
- Hosted exact-head run `30723660201` for commit `370d4a97` passed, including
  the RIPR+ receipt, policy/no-panic checks, full workspace validation,
  feature combinations, dependency governance, security/license audit,
  performance gate, and final workspace validation. Coverage and other
  policy-skipped lanes remained explicitly skipped.

## Claim boundary

This closes the invalid endpoint payload and missing-producer contract drift.
It does not provide test-efficiency evidence, RIPR+ zero, coverage closure,
mutation completeness, hardware validation, or release readiness.

The remaining producer work is tracked in issue #227. Until it lands, the
quality receipt must continue to treat test-efficiency evidence as skipped and
the policy exception must remain active.

## Follow-up status

Issue #227 adds the repo-owned producer and is being completed in a separate
PR. Once that PR merges, the producer becomes the source for native RIPR+
regeneration; the remaining quality-closure gaps are coverage and static
exposure debt, not missing test-efficiency input.
