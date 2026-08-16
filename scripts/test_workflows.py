#!/usr/bin/env python3
"""Regression tests for workflow supply-chain and reachability policy."""

from __future__ import annotations

import json
import pathlib
import re
import tomllib
import unittest

from scripts.ci_paths import DERIVED_CLASSES, classify_paths, tracked_paths


ROOT = pathlib.Path(__file__).resolve().parent.parent
WORKFLOWS = ROOT / ".github" / "workflows"
JUSTFILE = ROOT / "Justfile"
TOOL_MANIFEST = json.loads(
    (ROOT / ".config" / "dev-tools.json").read_text(encoding="utf-8")
)


def lane_channel() -> str:
    """The toolchain rustup selects inside the extension lane.

    A separate fact from `rust.primary`, and read from a separate file, because
    assuming them equal is what the test below exists to refuse.
    """
    path = ROOT / "extensions" / "money-pg" / "rust-toolchain.toml"
    pinned = tomllib.loads(path.read_text(encoding="utf-8"))
    return pinned["toolchain"]["channel"]


def workflow_job_bodies(source: str) -> dict[str, str]:
    """Job id to body text.

    Separate from `workflow_jobs`, which refuses `needs` and `if` spellings this
    repository does not use. That is right for reachability and wrong here,
    where all a caller needs is which job a step sits in.
    """
    split = source.split("\njobs:\n", 1)
    if len(split) == 1:
        raise AssertionError("workflow has no `jobs:` block to attribute steps to")
    jobs = split[1]
    starts = list(re.finditer(r"(?m)^  ([a-z0-9-]+):\n", jobs))
    # A job id this pattern misses is not skipped, it is CONCATENATED onto the previous job's
    # body, which would classify that job by another one's steps. Refuse instead.
    declared = re.findall(r"(?m)^  (\S+):$", jobs)
    unmatched = set(declared) - {start.group(1) for start in starts}
    if unmatched:
        raise AssertionError(f"job ids outside [a-z0-9-]+ cannot be attributed: {sorted(unmatched)}")
    return {
        start.group(1): jobs[
            start.end() : (
                starts[index + 1].start() if index + 1 < len(starts) else len(jobs)
            )
        ]
        for index, start in enumerate(starts)
    }


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

    def test_classifier_outputs_and_derived_classes_are_the_same_set(self) -> None:
        """A declared output nothing derives is dead; a derived class nothing
        declares is unreachable. Neither shows up as a failing job."""
        source = (WORKFLOWS / "on-pr-synced.yml").read_text(encoding="utf-8")
        block = re.search(r"(?m)^    outputs:\n((?:      \S.*\n)+)", source)
        self.assertIsNotNone(block, "the changes job declares no outputs")
        declared = set(re.findall(r"(?m)^      ([a-z0-9_]+):", block.group(1)))
        self.assertEqual(set(DERIVED_CLASSES), declared)

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
        """Each job installs the toolchain its own work will use.

        `rust-toolchain.toml` wins over whatever a job installed, so an
        extension-lane job handed the public workspace's toolchain still
        compiles with the lane's -- after rustup has downloaded it, inside every
        job, on every run, without ever producing a wrong answer to notice it
        by. The two versions agree today and are owned by different files, so
        nothing but this test would report the run where they stop agreeing.

        A job is a lane job when it runs `just pg`, which is how a lane job
        reaches the lane at all. Deriving it that way rather than from a list of
        job names means a new one is covered on the day it is written.
        """
        primary = TOOL_MANIFEST["rust"]["primary"]
        lane = lane_channel()

        checked = {"extension lane": 0, "public workspace": 0}
        for workflow in sorted(WORKFLOWS.glob("*.yml")):
            text = workflow.read_text(encoding="utf-8")
            for job, body in workflow_job_bodies(text).items():
                if "just pg " in body:
                    expected, where = lane, "extension lane"
                    # The lane builds with one toolchain. Miri is the exception it
                    # actually has; a matrix is not, and would install the public
                    # workspace's MSRV into a lane job.
                    allowed = {f"toolchain: {expected}", "toolchain: nightly"}
                else:
                    expected, where = primary, "public workspace"
                    allowed = {
                        f"toolchain: {expected}",
                        "toolchain: nightly",
                        "toolchain: ${{ matrix.toolchain }}",
                    }
                steps = re.findall(
                    r"(?ms)^      - uses: dtolnay/rust-toolchain@[0-9a-f]{40}"
                    r".*?(?=^      - |\Z)",
                    body,
                )
                for step in steps:
                    selected = {
                        line.strip()
                        for line in step.splitlines()
                        if line.strip().startswith("toolchain:")
                    }
                    checked[where] += 1
                    with self.subTest(workflow=workflow.name, job=job):
                        self.assertEqual(1, len(selected))
                        self.assertTrue(
                            selected <= allowed,
                            f"{job} installs {selected} but works in the {where}, "
                            f"which pins {expected}",
                        )

        # Per side, because the two values agree today: a single counter is kept
        # non-zero by the ~30 public-workspace jobs while every lane job goes
        # unchecked, which is the half this test was added for.
        for where, count in checked.items():
            with self.subTest(side=where):
                self.assertTrue(count, f"no {where} toolchain step checked; this would pass vacuously")

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
