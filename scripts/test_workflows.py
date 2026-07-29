#!/usr/bin/env python3
"""Regression tests for workflow supply-chain and reachability policy."""

from __future__ import annotations

import json
import pathlib
import re
import unittest

from scripts.ci_paths import classify_paths, tracked_paths


ROOT = pathlib.Path(__file__).resolve().parent.parent
WORKFLOWS = ROOT / ".github" / "workflows"
JUSTFILE = ROOT / "Justfile"
TOOL_MANIFEST = json.loads(
    (ROOT / ".config" / "dev-tools.json").read_text(encoding="utf-8")
)


def workflow_jobs(source: str) -> dict[str, dict[str, object]]:
    jobs = source.split("\njobs:\n", 1)[1]
    starts = list(re.finditer(r"(?m)^  ([a-z0-9-]+):\n", jobs))
    parsed: dict[str, dict[str, object]] = {}
    for index, start in enumerate(starts):
        end = starts[index + 1].start() if index + 1 < len(starts) else len(jobs)
        body = jobs[start.end() : end]

        needs: list[str] = []
        inline = re.search(r"(?m)^    needs: \[([^]]+)\]$", body)
        scalar = re.search(r"(?m)^    needs: ([a-z0-9-]+)$", body)
        block = re.search(r"(?ms)^    needs:\n((?:      - [a-z0-9-]+\n)+)", body)
        if inline:
            needs = [item.strip() for item in inline.group(1).split(",")]
        elif scalar:
            needs = [scalar.group(1)]
        elif block:
            needs = re.findall(r"(?m)^      - ([a-z0-9-]+)$", block.group(1))
        elif re.search(r"(?m)^    needs:", body):
            raise AssertionError(
                f"unsupported needs syntax in job {start.group(1)}"
            )

        condition = re.search(r"(?m)^    if: (.+)$", body)
        always = condition is not None and condition.group(1) == "${{ always() }}"
        output = None
        if condition is not None and not always:
            selected = re.fullmatch(
                r"\$\{\{ needs\.changes\.outputs\.([a-z_]+) == 'true' \}\}",
                condition.group(1),
            )
            if selected is None:
                raise AssertionError(
                    f"unsupported condition in job {start.group(1)}: "
                    f"{condition.group(1)}"
                )
            output = selected.group(1)

        parsed[start.group(1)] = {
            "needs": needs,
            "output": output,
            "always": always,
        }
    return parsed


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

    def test_path_filtered_jobs_cannot_be_cascade_skipped(self) -> None:
        workflow = (
            WORKFLOWS / "on-pr-synced.yml"
        ).read_text(encoding="utf-8")
        jobs = workflow_jobs(workflow)
        offenders: list[str] = []

        for path in tracked_paths():
            outputs = classify_paths([path])
            direct = {
                job: (
                    True
                    if policy["output"] is None
                    else outputs[str(policy["output"])]
                )
                for job, policy in jobs.items()
            }
            scheduled: dict[str, bool] = {}

            def is_scheduled(job: str) -> bool:
                if job in scheduled:
                    return scheduled[job]
                policy = jobs[job]
                dependencies = policy["needs"]
                assert isinstance(dependencies, list)
                selected = direct[job] and (
                    bool(policy["always"])
                    or all(is_scheduled(dependency) for dependency in dependencies)
                )
                scheduled[job] = selected
                return selected

            for job, selected in direct.items():
                if (
                    selected
                    and not bool(jobs[job]["always"])
                    and not is_scheduled(job)
                ):
                    offenders.append(f"{path}: {job}")

        self.assertEqual(
            offenders,
            [],
            "jobs selected by their path condition but suppressed by a skipped dependency",
        )


if __name__ == "__main__":
    unittest.main()
