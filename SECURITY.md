# Security Policy

## Reporting a vulnerability

Please report security vulnerabilities through GitHub's private vulnerability
reporting: open the **Security** tab on this repository and choose **Report a
vulnerability**. That channel is private and does not create a public issue.

Do not open a public issue or pull request for a security problem before it has
been triaged.

Useful things to include, to the extent you have them:

- What an attacker can do, and what access they need to start
- Affected component (`wheeld`, `wheelctl`, a plugin, the IPC surface, a device
  protocol crate) and version or commit
- Reproduction steps, a proof of concept, or a failing test
- Platform and hardware involved, if the issue is device-specific

## Scope

This project drives force-feedback hardware that can apply substantial torque to
a physical wheel. Reports are especially welcome for anything that could:

- Defeat or bypass the safety interlocks, the torque cap, or the hardware
  watchdog
- Cause the real-time path to emit unbounded, unintended, or unsafe output
- Escape the WASM plugin sandbox, or load a native plugin without a valid
  Ed25519 signature
- Reach privileged operations over the IPC surface without the corresponding
  capability
- Escalate privilege through the packaged systemd unit, udev rules, or installer

Reports about dependencies are welcome, but please check first whether the
advisory is already tracked — `cargo audit` and `cargo deny check` run in CI and
known advisories may already be recorded in `deny.toml`.

## Out of scope

- Findings that require an attacker to already have root or physical access to
  the machine running `wheeld`
- Missing hardening that has no demonstrated impact
- Automated scanner output with no accompanying analysis

## Project status

OpenRacing is pre-validation and has not been end-to-end verified on real
hardware. There are no supported released versions yet, so fixes land on `main`
rather than being backported. Treat the safety claims in the documentation as
design intent under active verification, not as an assurance argument.

## Disclosure

We will acknowledge a report and give an initial assessment as quickly as we
can. Please give us a chance to ship a fix before publishing details, and tell
us if you have a disclosure deadline so we can plan around it. Credit is offered
to reporters who want it.
