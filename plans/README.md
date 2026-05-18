# OpenRacing Plans

Plans are the **how and what lands next** layer of the source-of-truth stack. They sequence implementation work after the roadmap, proposal, specs, and ADRs have defined the lane boundary.

## Role in the stack

```text
Roadmap -> Proposal -> Spec -> ADR -> Plan -> Active goal -> PR -> Proof
```

A plan owns:

- PR/work item sequence;
- dependencies and blockers;
- proof commands;
- rollback instructions;
- handoff and closeout state.

A plan must not own:

- product motivation;
- durable architecture decisions;
- generated status truth;
- public support claims without proof pointers.

## Layout

```text
plans/
  <lane>/
    README.md
    implementation-plan.md
    closeout.md
```

Use short kebab-case lane names that match linked proposal/spec IDs where practical.

## Implementation plan template

~~~md
# Lane implementation plan

Status: active
Owner: n/a
Linked proposal: docs/proposals/OR-PROP-0001-lane.md
Linked specs:
- docs/specs/OR-SPEC-0001-contract.md
Linked ADRs: n/a
Active goal: .openracing/goals/active.toml

## Current state

Short factual baseline. Link to status docs and receipts.

## Work item: short-id

Status: ready
Linked proposal: docs/proposals/OR-PROP-0001-lane.md
Linked spec: docs/specs/OR-SPEC-0001-contract.md
Linked ADR: n/a
Blocks: n/a
Blocked by: n/a

### Goal

One paragraph.

### Production delta

What files, commands, APIs, workflows, or behavior change?

### Non-goals

What is explicitly out of scope?

### Acceptance

What must be true for the PR to merge?

### Proof commands

```bash
git diff --check
```

### Rollback

How to undo this PR safely.

### Notes

Optional.
~~~

## Agent rule

Agents should select exactly one ready work item from the active goal manifest and the linked implementation plan. If no ready work item exists, stop and write a handoff instead of inventing work.
