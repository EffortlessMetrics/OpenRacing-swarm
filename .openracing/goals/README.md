# OpenRacing Active Goals

Active goals are the **what now** layer of OpenRacing's source-of-truth stack. They are intentionally small, machine-readable TOML manifests that point agents to the current lane and ready work items.

## Role in the stack

```text
Roadmap -> Proposal -> Spec -> ADR -> Plan -> Active goal -> PR -> Proof
```

An active goal owns:

- current lane ID and title;
- machine-readable objective;
- linked proposal, specs, ADRs, and plan;
- ready/active/completed work items;
- proof commands for each work item;
- claim boundaries and status pointers.

An active goal must not own:

- long-form rationale;
- durable decisions;
- generated metrics;
- public support claims without receipt links.

## Files

```text
.openracing/goals/
  README.md
  active.toml
  archive/
    YYYY-MM-DD-<lane>.toml
```

This scaffold does not activate a lane by itself. Add `active.toml` only when a proposal/spec/plan lane is ready for execution.

## Template

```toml
id = "openracing-lane-id"
title = "Human readable lane title"
status = "active"
owner = "n/a"
created = "YYYY-MM-DD"

proposal = "docs/proposals/OR-PROP-0001-lane.md"
plan = "plans/lane/implementation-plan.md"

specs = [
  "docs/specs/OR-SPEC-0001-contract.md",
]

adrs = []

objective = """
State the current lane objective in one paragraph.
"""

end_state = [
  "Checkable end-state outcome.",
]

claim_boundaries = [
  "Do not broaden behavior from docs-only PRs.",
]

status_docs = []

[[work_item]]
id = "work-item-id"
status = "ready"
spec = "docs/specs/OR-SPEC-0001-contract.md"
adr = "n/a"
plan = "plans/lane/implementation-plan.md#work-item-work-item-id"
current_pointer = "n/a"
claim_boundary = "What this work item may and may not claim."
commands = [
  "git diff --check",
]
```

## Lifecycle

- Keep at most one `active.toml`.
- Use `status = "paused"` with a `reason` when no lane is active.
- Archive replaced manifests under `archive/YYYY-MM-DD-<lane>.toml`.
- Do not hand-edit generated status from this directory; link to the generator/checker named by the plan.
