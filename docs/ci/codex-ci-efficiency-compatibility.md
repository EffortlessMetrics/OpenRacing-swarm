# Codex CI Efficiency Compatibility Invariants

Status: draft
Owner: ci
Created: 2026-05-24

## Intent

Do not optimize CI by blindly canceling active work or by routing
metadata/control-plane edits through Rust lanes. Optimize by classifying change
surfaces correctly, preserving heavy-lane queue semantics, and keeping default
PR paths minimal.

## 1) Concurrency semantics (heavy/core lanes)

For heavy/core PR workflows, do **not** set `cancel-in-progress: true` unless a
repository-specific policy explicitly marks the workflow as safe-to-cancel.

Required behavior:

- one run is executing -> keep it running;
- newer commit arrives -> queue newer run;
- another newer commit arrives while one is pending -> replace older pending run;
- active run completes -> run latest pending one.

Canonical configuration:

```yaml
concurrency:
  group: ${{ github.workflow }}-${{ github.repository }}-${{ github.event.pull_request.number || github.ref }}
  cancel-in-progress: false
```

Rationale: canceling an active heavy run near completion wastes self-hosted
compute time and can increase queue churn.

## 2) Change classification invariants

Do not treat all edits as Rust-source changes.

Route these surfaces to docs/policy/light paths unless mixed with real
build/test/runtime changes:

- `docs/**`
- markdown-only edits (`*.md`, `README*`, `CHANGELOG*`, `SECURITY*`, `CONTRIBUTING*`)
- `policy/**`
- `plans/**`
- `badges/**`
- `AGENTS.md`
- `.github/CODEOWNERS`
- `.github/dependabot.yml`
- `.github/pull_request_template.md`
- `.github/PULL_REQUEST_TEMPLATE/**`
- `.codex/campaigns/**`
- `docs/tracking/**`
- `ci/hardware/**` receipt-only changes
- `.rails/**`
- `.uselesskey/**`

Special case:

- `.github/workflows/**` is **not** docs-light; route workflow-only edits to a
  minimal hosted validation/safety lane unless a stronger policy requires more.

## 3) Default PR routing policy

Classify first, then choose the cheapest truthful lane:

- docs/control-plane only -> no Rust compile.
- workflow-only -> hosted YAML/workflow validation only.
- Rust source/build/test touched -> self-hosted `rust-small`.
- hardware/GPU/receipt-only -> syntax/receipt validation only.
- unknown or mixed -> `rust-small` (not full CI).

Reserve full CI for explicit triggers (label, manual dispatch, main push,
release, schedule, merge queue, or equivalent policy).

## 4) Hosted fallback policy

Do not silently replace a self-hosted `rust-small` path with a full expensive
GitHub-hosted equivalent.

- Fork PRs may run a tiny hosted safe lane.
- No idle runner, token/readiness issues, or transient runner faults should not
  auto-trigger 75-120 minute hosted equivalents.
- Require explicit opt-in for expensive hosted fallback (for example labels or
  workflow dispatch input):
  - `full-ci`
  - `allow-github-hosted`
  - `ci-budget-ack`

## 5) Artifacts policy

Default PR paths should not upload bulky artifacts with `if: always()` unless
merge policy explicitly requires them.

- Prefer upload-on-failure.
- Use short retention (3-7 days) for diagnostic artifacts.
- Keep policy-required receipts minimal; avoid uploads on docs/control-plane-only
  routes.

## 6) CI-only PR test minimums

Every CI-efficiency PR must include:

- `git diff --check`
- YAML parse/validation for edited workflow files
- classification dry-run or unit tests covering:
  - docs-only;
  - `.rails/**`;
  - `.uselesskey/**`;
  - workflow file change;
  - Rust file change;
  - mixed docs + Rust
- explicit verification that heavy/core lanes remain no-cancel
  (`cancel-in-progress: false`) unless intentionally documented.

## 7) Reviewer rejection gates

Reject CI-efficiency PRs that do not answer yes to all:

1. Heavy/core lanes preserve `cancel-in-progress: false` semantics?
2. Metadata/control-plane-only edits avoid Rust CI?
3. Workflow edits stay out of docs-light routing?
4. No silent expensive hosted fallback path added?
5. Actual billable work reduced (not just shifted)?
