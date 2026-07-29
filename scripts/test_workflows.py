#!/usr/bin/env python3
"""Regression tests for workflow supply-chain and reachability policy."""

from __future__ import annotations

import json
import pathlib
import re
import unittest


ROOT = pathlib.Path(__file__).resolve().parent.parent
WORKFLOWS = ROOT / ".github" / "workflows"
JUSTFILE = ROOT / "Justfile"
TOOL_MANIFEST = json.loads(
    (ROOT / ".config" / "dev-tools.json").read_text(encoding="utf-8")
)


class WorkflowPolicyTests(unittest.TestCase):
    def test_remote_actions_are_pinned_to_full_commit_ids(self) -> None:
        for workflow in sorted(WORKFLOWS.glob("*.yml")):
            text = workflow.read_text(encoding="utf-8")
            for reference in re.findall(r"\buses:\s+([^#\s]+)", text):
                with self.subTest(workflow=workflow.name, reference=reference):
                    if reference.startswith("./"):
                        continue
                    self.assertRegex(reference, r"^[^@]+@[0-9a-f]{40}$")

    def test_ci_success_reaches_every_leaf_job(self) -> None:
        workflow = (
            WORKFLOWS / "on-pr-synced.yml"
        ).read_text(encoding="utf-8")
        jobs = workflow.split("\njobs:\n", 1)[1]
        job_ids = set(re.findall(r"(?m)^  ([a-z0-9-]+):\n", jobs))
        gate = jobs.split("\n  ci-success:\n", 1)[1]
        needs_block = gate.split("\n    if:", 1)[0]
        needs = set(re.findall(r"(?m)^      - ([a-z0-9-]+)$", needs_block))
        self.assertEqual(job_ids - {"ci-success"}, needs)

        allowed_block = gate.split("allowed-skips: >-", 1)[1]
        allowed = {
            item.strip()
            for item in allowed_block.replace("\n", " ").split(",")
            if item.strip()
        }
        self.assertEqual(needs - {"changes"}, allowed)

    def test_install_action_tools_are_exactly_pinned(self) -> None:
        versions = {
            tool["workflow_name"]: tool["version"]
            for group in ("cargo_tools", "system_tools")
            for tool in TOOL_MANIFEST[group]
        }
        versions.update(TOOL_MANIFEST["ci_only_tools"])

        for workflow in sorted(WORKFLOWS.glob("*.yml")):
            text = workflow.read_text(encoding="utf-8")
            for value in re.findall(r"(?m)^\s+tool:\s+(.+)$", text):
                for specification in value.split(","):
                    with self.subTest(
                        workflow=workflow.name,
                        specification=specification,
                    ):
                        name, separator, version = specification.partition("@")
                        self.assertEqual("@", separator)
                        self.assertIn(name, versions)
                        self.assertEqual(versions[name], version)

    def test_rust_toolchain_steps_select_an_explicit_toolchain(self) -> None:
        primary = TOOL_MANIFEST["rust"]["primary"]
        allowed = {
            f"toolchain: {primary}",
            "toolchain: ${{ matrix.toolchain }}",
            "toolchain: nightly",
        }

        for workflow in sorted(WORKFLOWS.glob("*.yml")):
            text = workflow.read_text(encoding="utf-8")
            steps = re.findall(
                r"(?ms)^      - uses: dtolnay/rust-toolchain@[0-9a-f]{40}"
                r".*?(?=^      - |\Z)",
                text,
            )
            for step in steps:
                selected = {
                    line.strip()
                    for line in step.splitlines()
                    if line.strip().startswith("toolchain:")
                }
                with self.subTest(workflow=workflow.name, step=step):
                    self.assertEqual(1, len(selected))
                    self.assertTrue(selected <= allowed)

    def test_workflow_steps_do_not_repeat_mapping_keys(self) -> None:
        for workflow in sorted(WORKFLOWS.glob("*.yml")):
            lines = workflow.read_text(encoding="utf-8").splitlines()
            starts = [
                index
                for index, line in enumerate(lines)
                if re.match(r"^      - \S", line)
            ]
            starts.append(len(lines))
            for start, end in zip(
                starts[:-1],
                starts[1:],
                strict=True,
            ):
                keys = re.findall(
                    r"^        ([a-zA-Z0-9_-]+):",
                    "\n".join(lines[start:end]),
                    flags=re.MULTILINE,
                )
                with self.subTest(workflow=workflow.name, line=start + 1):
                    self.assertEqual(len(keys), len(set(keys)))

    def test_docs_job_uses_the_package_lock(self) -> None:
        workflow = (
            WORKFLOWS / "on-pr-synced.yml"
        ).read_text(encoding="utf-8")
        self.assertIn("npm ci --no-fund --no-audit", workflow)
        self.assertNotIn("npm install ", workflow)

    def test_registry_token_jobs_use_the_protected_environment(self) -> None:
        for workflow in sorted(WORKFLOWS.glob("*.yml")):
            text = workflow.read_text(encoding="utf-8")
            if "SECRET_DEPLOY_CRATEIO" in text:
                with self.subTest(workflow=workflow.name):
                    self.assertIn("    environment: crates-io", text)

    def test_publish_all_verifies_workspace_dependencies_together(self) -> None:
        recipe = (
            JUSTFILE.read_text(encoding="utf-8")
            .split("\npublish-all:\n", 1)[1]
            .split("\n\n", 1)[0]
        )
        self.assertIn(
            "cargo publish --workspace --dry-run --allow-dirty",
            recipe,
        )
        self.assertNotIn("cargo publish -p", recipe)


if __name__ == "__main__":
    unittest.main()
