#!/usr/bin/env python3
"""Regression tests for workflow supply-chain and reachability policy."""

from __future__ import annotations

import pathlib
import re
import unittest

from scripts.ci_paths import DERIVED_CLASSES, classify_paths, tracked_paths


ROOT = pathlib.Path(__file__).resolve().parent.parent
WORKFLOWS = ROOT / ".github" / "workflows"
JUSTFILE = ROOT / "Justfile"


WORKFLOW_PREFIX = ".github/workflows/"
ACTIONS_SPELLINGS = (".yml", ".yaml")


def workflow_files() -> list[pathlib.Path]:
    """Every workflow Actions runs.

    Both spellings are accepted wherever Actions reads YAML, and GitHub reads
    `.github/workflows` itself without recursing into it.
    """
    files = sorted(
        ROOT / path
        for path in tracked_paths()
        if path.startswith(WORKFLOW_PREFIX)
        and "/" not in path[len(WORKFLOW_PREFIX) :]
        and path.endswith(ACTIONS_SPELLINGS)
    )
    assert files, "no workflow to check; this would pass vacuously"
    return files


def action_files() -> list[pathlib.Path]:
    """Every action this repository defines.

    An action is named by its own file, so one grouped into a subdirectory, or kept
    outside `.github`, is still an action a `uses: ./path` step can reach.
    """
    return sorted(
        ROOT / path
        for path in tracked_paths()
        if path.rsplit("/", 1)[-1] in {"action.yml", "action.yaml"}
    )


def selected_pin(line: str) -> str:
    """The pin a `toolchain:` line reads, as the output name it names.

    The channels themselves live in `.config/dev-tools.json` and are held equal to the
    `rust-toolchain.toml` each one governs by `tools/repo-policy/tests/pins.rs`. What a job
    chooses is which of them to read, which is what this file checks.
    """
    value = line.split(":", 1)[1].strip()
    # The pins are indexed out of the published manifest rather than read from an output
    # named after each one, so what identifies the channel is the path, not an output name.
    indexed = re.search(r"outputs\.manifest\)\.rust\.([a-z0-9_]+)", value)
    if indexed:
        return f"rust_{indexed.group(1)}"
    if "matrix.toolchain" in value:
        return "matrix"
    return value


def lane_entry_recipes() -> set[str]:
    """Root recipes that run inside the extension lane, transitively.

    `just pg` is not the only way in -- `gate-pg` cds there too and `gate-all`
    composes it -- so a hand-written marker would be a claim about the Justfile
    rather than a reading of it. Derived, so a new entry point is covered on the
    day it is written.
    """
    text = (JUSTFILE).read_text(encoding="utf-8")
    bodies: dict[str, str] = {}
    dependencies: dict[str, list[str]] = {}
    current: str | None = None
    for line in text.splitlines():
        header = re.match(r"^([a-z0-9-]+)([^:=\n]*):(?!=)(.*)$", line)
        if header:
            current = header.group(1)
            bodies[current] = ""
            dependencies[current] = re.findall(r"[a-z0-9-]+", header.group(3))
        elif current is not None and line[:1] in {" ", "\t"}:
            bodies[current] += line + "\n"
        elif line.strip():
            current = None

    entries = {name for name, body in bodies.items() if "cd extensions/money-pg" in body}
    while True:
        reached = {
            name
            for name, needs in dependencies.items()
            if name not in entries and entries.intersection(needs)
        }
        if not reached:
            break
        entries |= reached
    if not entries:
        raise AssertionError("no root recipe enters the extension lane; re-point this derivation")
    return entries


def workflow_job_bodies(source: str) -> dict[str, str]:
    """Job id to body text.

    Separate from the `needs`/`if` parsing in `workflow_jobs`, which refuses
    spellings this repository does not use, because all a caller needs here is
    which job a step sits in.
    """
    split = source.split("\njobs:\n", 1)
    if len(split) == 1:
        raise AssertionError("workflow has no `jobs:` block to attribute steps to")
    jobs = split[1]
    starts = list(re.finditer(r"(?m)^  ([a-z0-9-]+):\n", jobs))
    # A job id this pattern misses is not skipped, it is CONCATENATED onto the previous job's
    # body, which would classify that job by another one's steps. The declaration pattern
    # deliberately does NOT anchor at end of line: requiring `:$` would share the very blind
    # spot it is here to close, and a trailing comment or space would evade both.
    declared = re.findall(r"(?m)^  (\S+):", jobs)
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
    # ONE splitter, so the refusal to attribute an unparsable job id protects the reachability
    # tests too. A job folded into its predecessor takes its `needs` and `if` with it, and both
    # `test_ci_success_reaches_every_leaf_job` and the cascade simulation would then be reasoning
    # about a job list that is missing an entry.
    parsed: dict[str, dict[str, object]] = {}
    for name, body in workflow_job_bodies(source).items():
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
                f"unsupported needs syntax in job {name}"
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
                    f"unsupported condition in job {name}: "
                    f"{condition.group(1)}"
                )
            output = selected.group(1)

        parsed[name] = {
            "needs": needs,
            "output": output,
            "always": always,
        }
    return parsed


class WorkflowPolicyTests(unittest.TestCase):
    def test_remote_actions_are_pinned_to_full_commit_ids(self) -> None:
        for workflow in workflow_files():
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

        # The job also republishes the pinned versions, so every job reads them from one place
        # instead of restating them. Those come from the action, not from this list.
        action = (
            ROOT / ".github" / "actions" / "read-dev-tools" / "action.yml"
        ).read_text(encoding="utf-8")
        outputs = action.split("\noutputs:\n", 1)[1].split("\nruns:", 1)[0]
        pins = set(re.findall(r"(?m)^  ([a-z0-9_]+):$", outputs))
        self.assertTrue(pins, "the pins action declares no named output")

        self.assertEqual(set(DERIVED_CLASSES) | pins, declared)

    def test_no_action_installs_a_rust_toolchain(self) -> None:
        """The lane/public split is a property of the JOB, not of the step.

        `test_rust_toolchain_steps_select_an_explicit_toolchain` classifies a step by
        the recipes the job around it invokes. An action has no job, so a toolchain
        installed from inside one is reached by whichever job calls it and belongs to
        no side that could be checked -- it would pass that test by being invisible
        to it rather than by agreeing with it.
        """
        actions = action_files()
        self.assertTrue(actions, "no action to check; this would pass vacuously")
        for action in actions:
            with self.subTest(action=action.name):
                self.assertNotIn(
                    "dtolnay/rust-toolchain",
                    action.read_text(encoding="utf-8"),
                    f"{action.relative_to(ROOT)} installs a toolchain, which no job "
                    "owns and nothing can classify",
                )

    def test_rust_toolchain_steps_select_an_explicit_toolchain(self) -> None:
        """Each job installs the toolchain its own work will use.

        `rust-toolchain.toml` wins over whatever a job installed, so an
        extension-lane job handed the public workspace's toolchain still
        compiles with the lane's -- after rustup has downloaded it, inside every
        job, on every run, without ever producing a wrong answer to notice it
        by. The two versions agree today and are owned by different files, so
        nothing but this test would report the run where they stop agreeing.

        A job is a lane job when it invokes a root recipe that cds into the lane,
        and that set is read out of the Justfile rather than written here.
        """
        entries = lane_entry_recipes()
        enters_lane = re.compile(
            r"just (?:{})(?:\s|$)".format("|".join(re.escape(name) for name in sorted(entries)))
        )

        checked = {"extension lane": 0, "public workspace": 0}
        for workflow in workflow_files():
            text = workflow.read_text(encoding="utf-8")
            for job, body in workflow_job_bodies(text).items():
                if enters_lane.search(body):
                    expected, where = "rust_lane", "extension lane"
                    # The lane builds with one toolchain. Miri is the exception it
                    # actually has; a matrix is not, and would install the public
                    # workspace's MSRV into a lane job.
                    allowed = {expected, "nightly"}
                else:
                    expected, where = "rust_primary", "public workspace"
                    allowed = {expected, "nightly", "matrix"}
                steps = re.findall(
                    r"(?ms)^      - uses: dtolnay/rust-toolchain@[0-9a-f]{40}"
                    r".*?(?=^      - |\Z)",
                    body,
                )
                for step in steps:
                    selected = {
                        selected_pin(line)
                        for line in step.splitlines()
                        if line.strip().startswith("toolchain:")
                    }
                    # Only a step that reads the PINNED output counts. `nightly` is allowed on
                    # both sides, so counting it would leave the lane tally non-zero while no
                    # lane job had been compared to the lane's own pin.
                    if selected == {expected}:
                        checked[where] += 1
                    with self.subTest(workflow=workflow.name, job=job):
                        self.assertEqual(1, len(selected))
                        self.assertTrue(
                            selected <= allowed,
                            f"{job} selects {selected} but works in the {where}, "
                            f"which reads {expected}",
                        )

        # Per side, because one counter is kept non-zero by the roughly thirty
        # public-workspace jobs while every lane job goes unchecked, which is the
        # half this test was added for. The two sides read different pins, and
        # `tools/repo-policy` holds each pin equal to the file that governs it.
        for where, count in checked.items():
            with self.subTest(side=where):
                self.assertTrue(count, f"no {where} toolchain step checked; this would pass vacuously")

    def test_workflow_steps_do_not_repeat_mapping_keys(self) -> None:
        for workflow in workflow_files():
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
        for workflow in workflow_files():
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
