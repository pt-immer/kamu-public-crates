#!/usr/bin/env python3
"""Regression tests for deterministic development-environment policy."""

from __future__ import annotations

import json
import pathlib
import unittest

from scripts.dev_environment import (
    cargo_install_command,
    load_manifest,
    setup_commands,
)


ROOT = pathlib.Path(__file__).resolve().parent.parent


class DevelopmentEnvironmentPolicyTests(unittest.TestCase):
    def setUp(self) -> None:
        self.manifest = load_manifest()

    def test_rust_toolchain_matches_the_tool_manifest(self) -> None:
        toolchain = (ROOT / "rust-toolchain.toml").read_text(
            encoding="utf-8"
        )
        rust = self.manifest["rust"]
        self.assertIn(f'channel = "{rust["primary"]}"', toolchain)
        for component in rust["primary_components"]:
            self.assertIn(f'"{component}"', toolchain)
        for target in rust["primary_targets"]:
            self.assertIn(f'"{target}"', toolchain)

    def test_setup_commands_install_every_required_rust_item(self) -> None:
        commands = setup_commands(self.manifest)
        rust = self.manifest["rust"]
        rendered = [" ".join(command) for command in commands]

        self.assertTrue(
            any(
                f"toolchain install {rust['primary']}" in command
                for command in rendered
            )
        )
        self.assertTrue(
            any(
                f"toolchain install {rust['msrv']}" in command
                for command in rendered
            )
        )
        self.assertTrue(
            any(
                command[:5]
                == [
                    "rustup",
                    "target",
                    "add",
                    "--toolchain",
                    rust["primary"],
                ]
                for command in commands
            )
        )
        self.assertIn(
            ["npm", "ci", "--no-fund", "--no-audit"],
            commands,
        )
        self.assertFalse(
            any(
                "||" in argument
                for command in commands
                for argument in command
            )
        )

    def test_every_cargo_tool_install_is_locked_and_exact(self) -> None:
        primary = self.manifest["rust"]["primary"]
        for tool in self.manifest["cargo_tools"]:
            with self.subTest(tool=tool["crate"]):
                command = cargo_install_command(primary, tool)
                self.assertIn("--locked", command)
                self.assertIn("--force", command)
                version_index = command.index("--version") + 1
                self.assertEqual(f"={tool['version']}", command[version_index])

    def test_node_manifest_and_lock_use_the_exact_pin(self) -> None:
        package = json.loads(
            (ROOT / "package.json").read_text(encoding="utf-8")
        )
        lock = json.loads(
            (ROOT / "package-lock.json").read_text(encoding="utf-8")
        )
        for tool in self.manifest["node_tools"]:
            with self.subTest(package=tool["package"]):
                self.assertEqual(
                    tool["version"],
                    package["devDependencies"][tool["package"]],
                )
                self.assertEqual(
                    tool["version"],
                    lock["packages"][""]["devDependencies"][tool["package"]],
                )


if __name__ == "__main__":
    unittest.main()
