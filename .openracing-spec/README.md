# OpenRacing durable spec namespace

Status: draft
Owner: hardware
Created: 2026-05-21

` .openracing-spec/` is OpenRacing's durable, repo-owned source-of-truth namespace for proposal/spec/ADR/lane/closeout rails.

## Ownership boundaries

This namespace owns long-lived, tool-neutral artifacts such as:

- proposals (why / alternatives / success criteria)
- specs (behavior + evidence contracts)
- ADRs (durable architecture decisions)
- lane trackers and implementation plans (PR-sized execution flow)
- closeouts (what landed, what proved it, what remains)
- references to support tiers and policy ledgers

## External agent/tool namespaces

OpenRacing may contain external tool state directories such as:

- `.codex/`
- `.spec/`
- `.claude/`
- `.jules/`

Those directories are awareness-only for this system and are not owned or mutated by this namespace.
