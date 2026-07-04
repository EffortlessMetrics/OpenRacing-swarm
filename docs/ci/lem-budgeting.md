# Linux-equivalent minute budgeting

Linux-equivalent minutes (LEM) are the CI fuel gauge for comparing runner cost
across platforms and lane types.

```text
LEM = wall-clock minutes × runner multiplier
```

## Runner multipliers

Use Linux hosted runners as the baseline multiplier of `1.0`. Higher-cost or
scarcer runners should use larger multipliers when planning CI spend:

| Runner class | Planning multiplier |
|---|---:|
| Ubuntu/Linux hosted | 1.0 |
| Windows hosted | 2.0 |
| macOS hosted | 10.0 |
| Docker-heavy lane | 6.0 |
| GPU or specialized lane | 6.0 |
| External AI review | 1.0 plus vendor-specific cost notes |

The multiplier is a planning value, not a billing statement. When actual spend
or queue time differs, update the lane receipt or CI budget policy rather than
hiding the cost.

## Budget posture

Default PR lanes should target the cheapest evidence that can confidently catch
ordinary regressions. A healthy default PR budget is small enough that many
agent-assisted PRs can run without starving deeper validation.

Escalate beyond the default only when one of these is true:

- the changed paths match a high-risk pack;
- a maintainer applies a risk or full-CI label;
- the PR changes CI, release, dependency, unsafe, parser, protocol, or public
  API surfaces;
- the PR broadens a public claim;
- main, nightly, manual, or release workflows are running.

## Receipts

CI receipts should record:

- selected lanes;
- skipped lanes and the policy reason;
- estimated LEM before the run when available;
- actual wall-clock time and runner class after the run when available;
- whether a lane was blocking, advisory, or discovery-only.

The receipt boundary matters: a cheap PR can merge only the claims proven by its
selected lanes. More expensive proof must remain routed to the lane that really
ran it.
