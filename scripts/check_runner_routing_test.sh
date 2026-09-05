#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
exec "${PYTHON:-python3}" - "$script_dir/check_runner_routing.sh" "$@" <<'PY'
import itertools
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import textwrap
import unittest

GATE = str(Path(sys.argv.pop(1)).resolve())
BASH = shutil.which("bash")


class RoutingContract(unittest.TestCase):
    def setUp(self):
        self.scratch = tempfile.TemporaryDirectory(prefix="runner-routing-")
        self.addCleanup(self.scratch.cleanup)
        self.root = Path(self.scratch.name)
        self.workflows = self.root / "workflows"
        self.workflows.mkdir()

    def write(self, body, name="fixture.yml"):
        path = self.workflows / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(textwrap.dedent(body), encoding="utf-8")
        return path

    def run_gate(self, status, *, env=None, message=None):
        process_env = dict(os.environ, WORKFLOW_DIR=str(self.workflows), PYTHON=sys.executable)
        process_env.update(env or {})
        result = subprocess.run([BASH, GATE], env=process_env, capture_output=True, text=True, timeout=15)
        output = result.stdout + result.stderr
        self.assertEqual(result.returncode, status, output)
        if message is not None:
            self.assertIn(message, output)
        return output

    def test_qualified_block(self):
        self.write("""
            jobs:
              qualified:
                runs-on:
                  - self-hosted
                  - linux
                  - x64
                  - em-ci
                  - rust-medium
        """)
        self.run_gate(0, message="1 static Linux/x64 self-hosted")

    def test_bare_block(self):
        self.write("""
            jobs:
              bare:
                runs-on:
                  - self-hosted
                  - linux
                  - x64
        """)
        self.run_gate(1, message="jobs.bare")

    def test_bare_inline_all_label_orders(self):
        for labels in itertools.permutations(["self-hosted", "linux", "x64"]):
            with self.subTest(labels=labels):
                self.write(f"jobs:\n  bare:\n    runs-on: [{', '.join(labels)}]\n")
                self.run_gate(1)

    def test_qualified_inline_all_label_orders(self):
        for labels in itertools.permutations(["self-hosted", "linux", "x64", "rust-medium"]):
            with self.subTest(labels=labels):
                self.write(f"jobs:\n  qualified:\n    runs-on: [{', '.join(labels)}]\n")
                self.run_gate(0)

    def test_case_insensitive_quoted_labels(self):
        self.write('jobs:\n  bare:\n    runs-on: ["SELF-HOSTED", "Linux", "X64"]\n')
        self.run_gate(1)

    def test_group_qualifies_only_its_job(self):
        self.write("""
            jobs:
              bare:
                runs-on:
                  - self-hosted
                  - linux
                  - x64
              qualified:
                runs-on:
                  group: em-ci-rust
                  labels: [self-hosted, linux, x64]
        """)
        output = self.run_gate(1, message="jobs.bare")
        self.assertNotIn("jobs.qualified: bare", output)

    def test_neighbour_labels_do_not_qualify_bare_job(self):
        self.write("""
            jobs:
              bare:
                runs-on:
                  - self-hosted
                  - linux
                  - x64
              qualified:
                runs-on:
                  - self-hosted
                  - linux
                  - x64
                  - rust-medium
        """)
        self.run_gate(1, message="jobs.bare")

    def test_unrelated_step_text_is_not_a_qualifier(self):
        self.write("""
            jobs:
              bare:
                runs-on:
                  - self-hosted
                  - linux
                  - x64
                steps:
                  - run: 'echo group: em-ci-rust'
        """)
        self.run_gate(1)

    def test_comment_is_not_a_selector(self):
        self.write("""
            # runs-on: [self-hosted, linux, x64]
            jobs:
              hosted:
                runs-on: ubuntu-latest
        """)
        self.run_gate(0)

    def test_multiline_inline_selector(self):
        self.write("""
            jobs:
              bare:
                runs-on: [
                  self-hosted,
                  linux,
                  x64
                ]
        """)
        self.run_gate(1)

    def test_selector_is_not_limited_to_sixteen_lines(self):
        self.write("jobs:\n  bare:\n    runs-on:\n      - self-hosted\n" + "      # padding\n" * 20 + "      - linux\n      - x64\n")
        self.run_gate(1)

    def test_group_and_mapping_labels(self):
        self.write("jobs:\n  qualified:\n    runs-on:\n      group: em-ci-rust\n      labels: [self-hosted, linux, x64]\n")
        self.run_gate(0)

    def test_wrong_group_rejected(self):
        self.write("jobs:\n  bare:\n    runs-on:\n      group: unrelated\n      labels: [self-hosted, linux, x64]\n")
        self.run_gate(1)

    def test_unknown_capacity_is_not_a_qualifier(self):
        self.write("jobs:\n  bare:\n    runs-on: [self-hosted, linux, x64, unknown-capacity]\n")
        self.run_gate(1)

    def test_yaml_alias_is_checked(self):
        self.write("jobs:\n  first:\n    runs-on: &runner [self-hosted, linux, x64]\n  second:\n    runs-on: *runner\n")
        self.run_gate(1, message="jobs.second")

    def test_dynamic_and_reusable_selectors_are_explicitly_out_of_scope(self):
        self.write("""
            jobs:
              matrix:
                runs-on: ${{ matrix.os }}
              reusable:
                uses: ./.github/workflows/reusable.yml
        """)
        self.run_gate(0, message="1 selector(s) contain expressions not evaluated")

    def test_dynamic_group_is_not_proof(self):
        self.write("jobs:\n  bare:\n    runs-on:\n      group: em-ci-${{ inputs.group }}\n      labels: [self-hosted, linux, x64]\n")
        self.run_gate(1)

    def test_mixed_dynamic_labels_keep_static_guard(self):
        self.write('jobs:\n  bare:\n    runs-on: [self-hosted, linux, x64, "${{ inputs.extra }}"]\n')
        self.run_gate(1)

    def test_missing_directory_fails_closed(self):
        self.run_gate(2, env={"WORKFLOW_DIR": str(self.root / "absent")}, message="directory does not exist")

    def test_empty_directory_fails_closed(self):
        self.run_gate(2, message="no workflow YAML files")

    def test_invalid_yaml_fails_closed(self):
        self.write("jobs: [\n")
        self.run_gate(2, message="could not complete")

    def test_invalid_shapes_fail_closed(self):
        malformed_jobs = [
            "", "[]", "jobs: []", "jobs: {}", "jobs:\n  bad: []",
            "jobs:\n  bad: {}", "jobs:\n  bad:\n    runs-on: null",
            "jobs:\n  bad:\n    runs-on: [linux, 4]",
            "jobs:\n  bad:\n    runs-on:\n      group: []",
        ]
        malformed_jobs.extend(
            f"jobs:\n  bad:\n    uses: {uses}\n"
            for uses in ["{}", "[]", "null", "false", "42", '""', '"   "']
        )
        for body in malformed_jobs:
            with self.subTest(body=body):
                self.write(body)
                self.run_gate(2, message="could not complete")

    def test_yaml_extensions_and_nested_paths_are_scanned(self):
        self.write("jobs:\n  hosted:\n    runs-on: ubuntu-latest\n")
        self.write("jobs:\n  bare:\n    runs-on: [self-hosted, linux, x64]\n", "nested name/bare.yaml")
        self.run_gate(1, message="bare.yaml: jobs.bare")

    def test_non_yaml_files_are_not_workflows(self):
        self.write("jobs:\n  hosted:\n    runs-on: ubuntu-latest\n")
        self.write("runs-on: [self-hosted, linux, x64]", "README.md")
        self.run_gate(0)

    def test_missing_python_fails_closed(self):
        self.run_gate(2, env={"PYTHON": str(self.root / "missing-python")}, message="requires Python")

    def test_missing_pyyaml_fails_closed(self):
        (self.root / "yaml.py").write_text("raise ModuleNotFoundError('fixture: yaml unavailable')\n", encoding="utf-8")
        self.run_gate(2, env={"PYTHONPATH": str(self.root)}, message="requires PyYAML; no workflows checked")

    def test_ripgrep_is_no_longer_required(self):
        self.write("jobs:\n  hosted:\n    runs-on: ubuntu-latest\n")
        empty_path = self.root / "empty-path"
        empty_path.mkdir()
        self.run_gate(0, env={"PATH": str(empty_path)})


unittest.main(verbosity=2)
PY
