#!/usr/bin/env python3
"""Regression tests for workflow supply-chain and reachability policy."""

from __future__ import annotations

import pathlib
import re
import unittest


ROOT = pathlib.Path(__file__).resolve().parent.parent
WORKFLOWS = ROOT / ".github" / "workflows"


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


if __name__ == "__main__":
    unittest.main()
