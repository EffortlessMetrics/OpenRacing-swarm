# Rails framework

This repository uses `.rails/` as the durable Rails knowledge base.

## Directory responsibilities

- `.rails/` stores durable source-of-truth artifacts (proposals, specs, ADRs, lane trackers, support maps, policy references, receipts, closeouts, and schemas).
- `docs/` explains Rails to humans and contributors.

## External namespaces (awareness-only)

Rails does not own these directories and this lane does not migrate or rewrite them:

- `.codex/` (Codex execution state)
- `.spec/` (Spec Kit / speckit state)
- `.claude/` (external agent/session state)
- `.jules/` (external agent/session state)

## Source-of-truth principle

Keep why, behavior, decisions, sequencing, proof, claims, policy, and closeout in separate artifacts instead of merging all truth into one document type.
