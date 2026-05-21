# Contributing with Rails

When you create or update durable source-of-truth artifacts in this repository, place them under `.rails/` and index them through `.rails/index.toml`.

## Rules

- Use `.rails/` for durable proposal/spec/ADR/lane/support/policy/receipt/closeout artifacts.
- Keep external namespaces (`.codex/`, `.spec/`, `.claude/`, `.jules/`) awareness-only for Rails ownership.
- Do not place Rails-owned artifact paths under external namespaces.
- Keep lane tracking focused with per-lane trackers under `.rails/lanes/`.

## Minimal PR proof

Run:

```bash
git diff --check
```
