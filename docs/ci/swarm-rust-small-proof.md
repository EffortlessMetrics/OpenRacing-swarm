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

The current safe pull-request route is GitHub Hosted because this repository
still needs a reliable self-hosted runner capacity signal for `em-ci-small` and
`EM_RUNNER_READ_TOKEN`. Merge groups and explicit workflow dispatch can still
exercise the self-hosted route.

Release, publish, signing, secrets-heavy deployment, GPU, and full-platform
workflows remain outside the protected Rust Small swarm lane until separate
deliberate migration work.
