# Coverage

Codecov coverage is Rust execution-surface evidence.

## What coverage answers

> Did tests execute this Rust code surface?

## What coverage does NOT answer

- whether real-time deadlines are met
- whether physical hardware behaves correctly
- whether force-feedback output is safe
- whether telemetry adapters are correct
- whether simulator integrations are validated
- whether hardware-in-the-loop testing passed
- whether BDD feature coverage is complete
- whether fuzzing is sufficient
- whether release readiness is proven

Those are separate proof lanes.

## Workflow

The Coverage workflow runs on:

- **push to main**: Full coverage collection and upload to Codecov
- **workflow_dispatch**: Manual trigger (e.g., for debugging)
- **PRs labeled**: `coverage`, `full-ci`, or `ci:full`

Other PRs do not run coverage by default to save CI cost.

## Artifacts

Durable receipts are:

- `codecov.json` — Codecov JSON upload format
- `coverage-summary.txt` — Human-readable summary
- `target/coverage/coverage-receipt.json` — Claim boundary record
- GitHub Actions artifact upload (30-day retention)
- Codecov dashboard

## Configuration

See `codecov.yml` at the repository root for status checks, ignored paths, and Codecov settings.

## Codecov token setup

To enable Codecov uploads to the Codecov dashboard:

1. Go to [codecov.io](https://codecov.io) and sign in with GitHub
2. Navigate to the OpenRacing repository
3. Copy the repository upload token
4. Go to the repository settings → Secrets and variables → Actions
5. Create a new secret: `CODECOV_TOKEN` = `<token>`

Without this token, coverage artifacts are still generated and uploaded to GitHub Actions, but the Codecov dashboard will not receive updates.

## Codecov comments

Codecov comments are **disabled**. Coverage status is advisory only and does not block merges.

If you need coverage details, check:

1. The Codecov dashboard (linked from the PR check)
2. The uploaded artifacts
3. The receipt: `target/coverage/coverage-receipt.json`
