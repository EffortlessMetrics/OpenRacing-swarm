# OpenRacing spec-style control plane

Status: draft
Owner: hardware
Created: 2026-05-21

## Durable home

OpenRacing keeps its durable source-of-truth rails in `/.openracing-spec/`.

That namespace is repo-owned and long-lived. It should hold the full chain:

Roadmap -> Proposal -> Spec -> ADR -> Lane tracker -> Implementation plan -> PR -> Proof -> Support/policy reference -> Closeout.

## Separation of concerns

- `/.openracing-spec/`: durable repo knowledge base and specification rails.
- `/docs/`: human-facing explanation and contributor guidance.
- `/policy/`: live enforcement ledgers, referenced where relevant.
- `/plans/`: used only when already part of the repo's non-agent planning surface.

## External agent state

Directories such as `.codex/`, `.claude/`, `.jules/`, and `.spec/` are external/tool-specific state.

They are not the durable source of truth for this repo-native spec system.
