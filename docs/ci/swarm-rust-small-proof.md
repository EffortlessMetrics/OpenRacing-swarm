# OpenRacing Swarm Rust Small Proof

This repository is the high-volume same-repo PR workspace for
`EffortlessMetrics/OpenRacing`.

The first protected swarm lane is `OpenRacing Rust Small Result`. Branch
protection must require that normalized result, not the conditional
implementation jobs for CX43, CX33, CX53, or GitHub-hosted fallback.

Initial proof captured:

- routed workflow setup PR: `#1`;
- GitHub-hosted PR fallback route: `26149791075`;
- manual dispatch fallback route: `26151027546`.

The routed route for a trusted same-repository pull request, merge group, or
explicit dispatch is now capacity-aware and ordered:

1. CX43: `em-ci`, `cx43`, `rust-medium`, `trusted-pr`;
2. CPX42: `em-ci`, `cpx42`, `rust-medium`, `rust-16gb`, `trusted-pr`;
3. CX53: `em-ci`, `cx53`, `rust-large`, `trusted-pr`;
4. GitHub-hosted fallback when no eligible runner is online and idle.

Untrusted or fork pull requests use the GitHub-hosted route. Missing runner
credentials, runner API failures, and parse/configuration failures are not
silently treated as capacity fallback: the normalized result fails closed so
the required `OpenRacing Rust Small Result` check remains trustworthy.

The route is selected by `orgs/${ORG}/actions/runners` and requires all labels
in the matrix, not merely a host label. The workflow keeps
`cancel-in-progress: false` so a heavy run already using self-hosted capacity
is not discarded near completion.

The routing guard is locally reproducible with:

```bash
scripts/check_runner_routing_test.sh
scripts/check_runner_routing.sh
```

Release, publish, signing, secrets-heavy deployment, GPU, and full-platform
workflows remain outside the protected Rust Small swarm lane until separate
deliberate migration work.
