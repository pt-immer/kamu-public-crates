#!/usr/bin/env python3
"""Ensure every standalone Cargo package has gate, CI, and update ownership."""

from __future__ import annotations

import pathlib
import re
import subprocess
import tomllib
import unittest

ROOT = pathlib.Path(__file__).resolve().parent.parent


def tracked_manifests() -> list[pathlib.Path]:
    output = subprocess.check_output(
        ["git", "ls-files", "*Cargo.toml"],
        cwd=ROOT,
        text=True,
    )
    return [ROOT / line for line in output.splitlines() if line]


def standalone_manifests() -> set[str]:
    result = set()
    for manifest in tracked_manifests():
        with manifest.open("rb") as source:
            data = tomllib.load(source)
        if "package" in data and "workspace" in data:
            result.add(manifest.relative_to(ROOT).as_posix())
    return result


class PackageOwnershipTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        with (ROOT / ".config/package-owners.toml").open("rb") as source:
            policy = tomllib.load(source)
        cls.owners = {
            row["manifest"]: row for row in policy.get("standalone", [])
        }
        cls.justfile = (ROOT / "Justfile").read_text(encoding="utf-8")
        cls.workflow = (
            ROOT / ".github/workflows/on-pr-synced.yml"
        ).read_text(encoding="utf-8")
        cls.dependabot = (
            ROOT / ".github/dependabot.yml"
        ).read_text(encoding="utf-8")

    def test_every_standalone_package_has_an_owner_record(self) -> None:
        self.assertEqual(set(self.owners), standalone_manifests())

    def test_owner_records_name_real_gate_recipes_and_ci_jobs(self) -> None:
        for manifest, owner in self.owners.items():
            with self.subTest(manifest=manifest):
                recipe = re.escape(owner["gate_recipe"])
                job = re.escape(owner["ci_job"])
                self.assertRegex(self.justfile, rf"(?m)^{recipe}:")
                self.assertRegex(
                    self.justfile,
                    rf'(?ms)^gate:.*"just {recipe}"',
                )
                self.assertRegex(self.workflow, rf"(?m)^  {job}:")
                self.assertRegex(
                    self.workflow,
                    rf"(?ms)^  {job}:.*?run: just {recipe}",
                )

    def test_owner_records_name_dependabot_directories(self) -> None:
        for manifest, owner in self.owners.items():
            for directory in owner["dependabot_directories"]:
                with self.subTest(manifest=manifest, directory=directory):
                    self.assertIn(
                        f'directory: "{directory}"',
                        self.dependabot,
                    )


if __name__ == "__main__":
    unittest.main()
