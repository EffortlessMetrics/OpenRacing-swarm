# Non-Rust file policy

OpenRacing is a Rust workspace, but it has legitimate non-Rust surfaces:
GitHub Actions workflows, packaging assets, protobuf and JSON schemas,
fixtures, captures, scripts, docs, etc. This policy makes the supported
non-Rust surface area **explicit and reviewable** instead of an
ever-growing junk drawer.

## Where the policy lives

* `policy/non-rust-allowlist.toml` — TOML allowlist of expected
  non-Rust surfaces, with owner / surface / classification / reason /
  covered-by metadata for each.
* `scripts/policy_file.py` — checker that walks every git-tracked
  file and matches it against the allowlist.

## Schema

Each allowlist entry has:

| Field | Required | Notes |
|---|---|---|
| `glob` or `path` | yes | Glob (relative to repo root) or exact path. |
| `kind` | yes | Short stable identifier (no spaces). |
| `owner` | yes | Team / area owner; matches CODEOWNERS-style label. |
| `surface` | yes | One of: `ci`, `docs`, `fixtures`, `tooling`, `packaging`, `proto`, `schema`, `shader`, `editor`, `website`, `badge`, `scripts`, `hardware`, `telemetry`, `thirdparty`. |
| `classification` | yes | One of: `config`, `test`, `production`, `tooling`, `generated`, `thirdparty`. |
| `reason` | yes | Human reason this surface exists. |
| `covered_by` | yes | List of commands / workflows that exercise the file. May be empty for static metadata. |
| `expires` | no | Time-boxed exception expiry (ISO date). |
| `retired` | no | If `true`, entry is preserved for history but ignored for matching. |

## Discipline

The checker treats non-allowlisted non-Rust files as **policy
violations**, even if they happen to compile. The intended workflow:

1. Add a new non-Rust surface (workflow, fixture, script, schema, …).
2. Add a matching entry in `policy/non-rust-allowlist.toml`.
3. Run `python scripts/policy_file.py` to confirm no drift.

For one-off generated artifacts that should not be tracked here (e.g.
build outputs in `target/`), prefer to keep them out of git via
`.gitignore`.

## Stages

* **Stage 1 — current.** Checker runs and reports drift. Stage A
  rollout includes a comprehensive seed allowlist intended to cover the
  current OpenRacing tree without extra triage work.
* **Stage 2.** Checker becomes blocking. Drift fails CI; new non-Rust
  surfaces require a policy entry to land.
* **Stage 3.** `expires` is enforced; retired entries with no matching
  files are deleted in cleanup PRs.

## Why TOML, not pipe-delimited text

Earlier policy tooling used `glob|kind|owner|reason` text files. TOML
gives us:

* multi-value fields (`covered_by`),
* optional metadata (`expires`, `retired`),
* strict parsing,
* better diff review.

This is the same direction other Effortless Metrics repos
(`ripr`, `perfgate`) are converging on, so the schema is intentionally
stable across the estate.
