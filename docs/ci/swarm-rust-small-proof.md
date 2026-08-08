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

## Unfit selected runner

Runner discovery can only see `online` and `busy`. A runner that reports both
as healthy may still be unfit to build — most commonly when `/mnt/ci-scratch`
is below the 100 GB disk guard, which fails the lane before checkout. Because
route order prefers any idle runner, such a runner would otherwise win the
route on every attempt and the required check could never pass.

When the router succeeds without error and the selected self-hosted lane
fails, `OpenRacing Rust Small on GitHub Hosted (Fallback)` re-runs the same
`cargo check` and `cargo test --lib` commands. The normalized result then
reports `proof_lane: github-fallback` and the run emits a warning so the
degraded path stays visible.

This is a retry on different hardware, not a relaxed gate:

- the disk guard and its thresholds are unchanged;
- no lane skips build or test;
- a real code failure fails on the self-hosted lane and on the fallback, so
  the normalized result still fails;
- a lane that was skipped rather than run is never rescued.

The normalized-result contract is locally reproducible with:

```bash
scripts/check_routed_rust_result_test.sh
```

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
