#!/usr/bin/env python3
"""Guardrails for .github/workflows/em-ci-routed-rust.yml medium-lane routing."""
from pathlib import Path
import sys

WORKFLOW = Path('.github/workflows/em-ci-routed-rust.yml')
text = WORKFLOW.read_text(encoding='utf-8')
errors: list[str] = []

required = [
    'orgs/${ORG}/actions/runners?per_page=100',
    'GH_TOKEN: ${{ secrets.EM_RUNNER_READ_TOKEN }}',
    'emit "github" "fork_pr" "false" "$trusted"',
    'emit "cpx42" "cpx42_idle" "false" "$trusted"',
    'labels: [self-hosted, linux, x64, em-ci, cpx42, rust-16gb, rust-medium, trusted-pr]',
    'mkdir -p "$TMPDIR" "$CARGO_TARGET_DIR"',
    'uses: dtolnay/rust-toolchain@v1',
    'toolchain: 1.95.0',
    "needs.route-rust-small.outputs.target == 'cpx42'",
    '- rust-small-cpx42',
    '- rust-small-cx43',
    '- rust-small-cx53',
    '- rust-small-github',
]
for snippet in required:
    if snippet not in text:
        errors.append(f'missing required snippet: {snippet}')

if 'repos/' in text and '/actions/runners' in text:
    errors.append('forbidden repository runner endpoint present')

for forbidden in ['rust-small-cx33', 'cx33_idle', "'cx33'", 'em-ci-rust:1.95 exists on CPX42']:
    if forbidden in text:
        errors.append(f'forbidden medium-lane artifact present: {forbidden}')

if errors:
    print('EM CI routed rust policy check failed:')
    for err in errors:
        print(f'- {err}')
    sys.exit(1)

print('EM CI routed rust policy check passed')
