# Repo source-of-truth system

OpenRacing uses a linked source-of-truth stack so humans and agents can find the right truth in the right artifact without relying on chat history.

## Stack

```text
Roadmap
  -> Proposal
    -> Spec
      -> ADR
        -> Implementation plan
          -> Active goal
            -> PR
              -> Proof
```

## Artifact roles

| Artifact | Owns | Does not own |
|---|---|---|
| Roadmap | Release direction, milestone framing, lane discovery | Detailed PR queue, live status, proof receipts |
| Proposal | Why, users, alternatives, risks, non-goals | Behavior contract, PR sequence, generated status |
| Spec | Required behavior, acceptance, examples, proof | Product rationale, PR sequencing, live queue |
| ADR | Durable architecture or operating decision | Task list, current metric state, implementation queue |
| Plan | Work item order, dependencies, proof commands, rollback | Product rationale, durable architecture, generated status truth |
| Active goal | Current machine-readable lane and work items | Long prose, generated metrics, durable decisions |
| Support tiers | Public claim proof, limits, next promotion proof | Feature design, implementation sequence |
| Policy ledgers | Exceptions, owners, coverage, review dates | Broad architecture, undocumented debt |

## Rules

1. Keep one kind of truth per artifact.
2. Prefer one semantic artifact per PR unless a plan explicitly says otherwise.
3. Specs define behavior; plans define sequencing.
4. Proposals explain why; ADRs record durable decisions.
5. Active goals tell agents what to do now.
6. Generated status must be updated by tools, not by hand.
7. Public support claims require support-tier proof or an equivalent receipt pointer.
8. Policy exceptions require an owner, reason, coverage, and review date.

## Required headers

Use `n/a` when a field is not applicable. Existing legacy files may be normalized as they are touched.

### Proposal headers

```text
Status:
Owner:
Created:
Target milestone:
Linked specs:
Linked ADRs:
Linked plan:
Support/status impact:
Policy impact:
```

### Spec headers

```text
Status:
Owner:
Created:
Linked proposal:
Linked ADRs:
Linked plan:
Linked issues:
Linked PRs:
Support-tier impact:
Policy impact:
```

### ADR headers

```text
Status:
Date:
Owner:
Linked proposal:
Linked specs:
Linked plan:
```

### Plan headers

```text
Status:
Owner:
Linked proposal:
Linked specs:
Linked ADRs:
Active goal:
```

## Agent workflow

Agents must:

1. read `AGENTS.md` or the tool-specific instruction file;
2. read this file;
3. read `.openracing/goals/active.toml` when it exists;
4. read the linked implementation plan;
5. read the linked spec for the selected work item;
6. read linked ADRs for constraints;
7. choose exactly one ready work item;
8. implement only that item;
9. run the listed proof commands and `git diff --check`;
10. update receipts, status, or policy only when the work item requires it;
11. stop instead of guessing when source-of-truth links are missing or contradictory.

If `.openracing/goals/active.toml` is absent, the repo has no activated source-of-truth lane yet. Do not invent runtime work from this scaffold alone.

## Stop conditions

Stop and report instead of proceeding when:

- the active goal is required but missing or stale;
- linked files do not exist;
- a linked spec or ADR contradicts the requested work;
- proof commands cannot run and no substitute evidence is documented;
- generated status is dirty or would need hand edits;
- unrelated staged changes exist;
- a public claim lacks support-tier proof;
- a policy exception lacks owner, reason, coverage, or review date.

## Active goal lifecycle

Activate one lane at a time in:

```text
.openracing/goals/active.toml
```

Use `status = "active"` for a live lane. Use `status = "paused"` with a `reason` when no lane is selected.

Archive replaced goals under:

```text
.openracing/goals/archive/YYYY-MM-DD-<lane>.toml
```

Do not leave multiple active goal manifests.

## Plan work item requirements

Every plan work item should include:

- status;
- linked proposal, spec, ADR where applicable;
- goal;
- production delta;
- non-goals;
- acceptance;
- proof commands;
- rollback;
- notes when useful.

## Closeout

At the end of a lane, write:

```text
plans/<lane>/closeout.md
```

A closeout records what shipped, proof commands, receipts, PRs, CI runs, generated status, support-tier updates, policy updates, deferred work, claim boundaries, and the recommended next lane.

## Common failure modes

### Spec becomes a task list

Move PR order to `plans/<lane>/implementation-plan.md`; keep the spec to behavior, examples, and proof.

### Plan becomes product rationale

Move the rationale to `docs/proposals/`; keep the plan to work items, dependencies, proof, and rollback.

### Active goal becomes prose

Keep TOML short and machine-readable. Link out to docs instead of embedding long tables.

### Agent hand-edits generated status

Name the generator/checker in the plan item and run it instead.

### Support claims drift

Require support-tier impact headers and receipt-backed proof pointers for public claims.

### Policy exceptions become silent debt

Every exception needs owner, reason, `covered_by`, `review_after`, and optional `expires`.

### Mega PR

Split by semantic artifact or by one implementation work item.

## What good looks like

A new contributor or agent can arrive cold and answer:

```text
What are we doing?
Why?
What must be true?
What decision constrains it?
What PR lands next?
What command proves it?
What may we claim?
What must we not claim?
```

If the repo answers those questions without chat history, the source-of-truth stack is working.
