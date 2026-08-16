#!/usr/bin/env python3
"""Regression tests for deterministic development-environment policy."""

from __future__ import annotations

import contextlib
import io
import json
import os
import pathlib
import re
import tempfile
import unittest
from collections.abc import Callable
from typing import Any, TypeVar
from unittest import mock

from scripts import dev_environment

# Channels the root `setup` deliberately does not install. The extension lane installs its
# own from its own `rust-toolchain.toml` through `just pg setup`, and root setup doing it
# too would download a second toolchain for every contributor who never enters the lane.
# The manifest still names it because CI must select it without entering the lane. Stated
# here rather than derived away, so a channel added without a setup command still fails.
INSTALLED_BY_THE_LANE = {"lane"}

from scripts.dev_environment import (
    INSTALLED_BY_CI,
    Doctor,
    Palette,
    cargo_install_command,
    capture,
    check_bootstrap,
    check_floor,
    check_targets,
    check_toolchain,
    contains_version,
    rust_item_detail,
    search_path,
    is_repository_local,
    load_manifest,
    node_package_version,
    palette,
    parse_version,
    resolve,
    satisfies_floor,
    setup_commands,
    tool_sections,
    tools,
)


ROOT = pathlib.Path(__file__).resolve().parent.parent

T = TypeVar("T")


def executable(directory: pathlib.Path, name: str) -> pathlib.Path:
    """Create one runnable stub so shutil.which can resolve it."""
    path = directory / name
    path.write_text("#!/bin/sh\n", encoding="utf-8")
    path.chmod(0o755)
    return path


def captured(render: Callable[[], T]) -> tuple[T, str]:
    """Run one reporting call and return its result beside its output."""
    buffer = io.StringIO()
    with contextlib.redirect_stdout(buffer):
        result = render()
    return result, buffer.getvalue()


class DevelopmentEnvironmentPolicyTests(unittest.TestCase):
    def setUp(self) -> None:
        self.manifest = load_manifest()

    # The toolchain channels, the components and targets, and the MSRV each manifest declares
    # are held equal to `.config/dev-tools.json` by `tools/repo-policy/tests/pins.rs`, which
    # reads TOML with a TOML parser rather than matching its text.

    def test_every_pinned_toolchain_literal_names_a_manifest_version(self) -> None:
        """`rustup run <version>` addresses a toolchain by name, so a literal
        that outlives a bump either resolves to a stale install or fails to
        resolve at all. Both read as the gate proving something CI does not.
        """
        rust = self.manifest["rust"]
        allowed = set()
        for version in (rust["msrv"], rust["primary"]):
            allowed.add(version)
            allowed.add(".".join(version.split(".")[:2]))

        sites = {
            "Justfile": r"(?:cargo \+|rustup run |msrv\()([0-9]+\.[0-9]+(?:\.[0-9]+)?)",
            "README.md": r"Rust[- ]([0-9]+\.[0-9]+(?:\.[0-9]+)?)",
            # Clippy gates which lints apply on this, so a stale value lints the
            # workspace against a Rust it no longer supports.
            "clippy.toml": r'(?m)^msrv = "([0-9]+\.[0-9]+(?:\.[0-9]+)?)"',
        }
        for name, pattern in sites.items():
            found = set(
                re.findall(pattern, (ROOT / name).read_text(encoding="utf-8"))
            )
            with self.subTest(file=name):
                self.assertTrue(found, "no pinned toolchain literal to bind")
                self.assertEqual(set(), found - allowed)

        labels = re.findall(
            r"msrv\(([0-9]+\.[0-9]+(?:\.[0-9]+)?)\)",
            (ROOT / "Justfile").read_text(encoding="utf-8"),
        )
        self.assertTrue(labels, "no msrv stage label to bind")
        for label in labels:
            with self.subTest(label=label):
                self.assertEqual(rust["msrv"], label)

    def test_a_missing_system_tool_is_reported_with_the_version_to_install(self) -> None:
        """AGENTS.md states this as a guarantee: setup cannot install these, so the
        row a developer reads has to name the version to install.

        Asserted through the rendered row rather than the hint string. The hint is
        built from the entry, so comparing it against that entry would be true by
        construction and could not fail.
        """
        checked = 0
        for tool in tools(self.manifest, "system_tools"):
            checks = Doctor(palette())
            absent = dict(tool, binary=f"{tool['name']}-absent-from-this-machine")
            with mock.patch.object(dev_environment.shutil, "which", return_value=None):
                buffer = io.StringIO()
                with contextlib.redirect_stdout(buffer):
                    dev_environment.check_system_tool(checks, absent)
            with self.subTest(tool=tool["name"]):
                self.assertIn(tool["version"], buffer.getvalue())
            checked += 1
        self.assertTrue(checked, "the manifest pins no system tool to check")

    def test_setup_commands_install_every_required_rust_item(self) -> None:
        commands = setup_commands(self.manifest)
        rust = self.manifest["rust"]
        rendered = [" ".join(command) for command in commands]

        # A channel is a key naming a version; the component and target lists are keyed off
        # one. Deriving the set from `_components` instead exempts any channel that declares
        # none, which is the same silence this test exists to break.
        channels = {name for name, value in rust.items() if isinstance(value, str)}
        self.assertTrue(channels, "the manifest names no channel to install")
        self.assertLessEqual(
            INSTALLED_BY_THE_LANE,
            channels,
            f"{sorted(INSTALLED_BY_THE_LANE - channels)} is exempted but names no channel",
        )
        for channel in sorted(channels - INSTALLED_BY_THE_LANE):
            with self.subTest(channel=channel):
                self.assertIn(f"{channel}_components", rust, f"{channel} lists no component")
                installs = [
                    command
                    for command in rendered
                    if f"toolchain install {rust[channel]}" in command
                ]
                # Exactly one, so two channels sharing a version cannot satisfy each other's
                # components through a command built for the other.
                self.assertEqual(
                    1,
                    len(installs),
                    f"setup builds {len(installs)} install commands for {channel}",
                )
                for component in rust[f"{channel}_components"]:
                    self.assertIn(
                        f"--component {component}",
                        installs[0],
                        f"setup installs {channel} without {component}",
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
        for tool in tools(self.manifest, "cargo_tools"):
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
        for tool in tools(self.manifest, "node_tools"):
            with self.subTest(package=tool["package"]):
                self.assertEqual(
                    tool["version"],
                    package["devDependencies"][tool["package"]],
                )
                self.assertEqual(
                    tool["version"],
                    lock["packages"][""]["devDependencies"][tool["package"]],
                )


class ToolResolutionTests(unittest.TestCase):
    """Doctor resolves a tool exactly as an invoked recipe would."""

    @contextlib.contextmanager
    def workspace(self) -> Any:
        """Yield a repository-local prefix and a separate system prefix."""
        with tempfile.TemporaryDirectory() as root:
            local = pathlib.Path(root) / "local"
            system = pathlib.Path(root) / "system"
            local.mkdir()
            system.mkdir()
            with mock.patch.object(
                dev_environment, "SEARCH_PREFIXES", (local,)
            ), mock.patch.dict(os.environ, {"PATH": str(system)}):
                yield local, system

    def test_the_repository_local_copy_wins_over_the_system_copy(
        self,
    ) -> None:
        with self.workspace() as (local, system):
            wanted = executable(local, "taplo")
            executable(system, "taplo")
            found = resolve("taplo")
            self.assertEqual(wanted, found)
            self.assertTrue(is_repository_local(found))

    def test_resolution_falls_back_to_the_wider_path(self) -> None:
        with self.workspace() as (_, system):
            wanted = executable(system, "taplo")
            found = resolve("taplo")
            self.assertEqual(wanted, found)
            self.assertFalse(is_repository_local(found))

    def test_an_absent_tool_resolves_to_nothing(self) -> None:
        with self.workspace():
            self.assertIsNone(resolve("taplo"))

    def test_a_missing_binary_never_reports_a_host_path(self) -> None:
        with tempfile.TemporaryDirectory() as root:
            absent = pathlib.Path(root) / "nowhere" / "taplo"
            status, output = capture([str(absent)])
            self.assertEqual(127, status)
            self.assertNotIn(root, output)


class NodeVersionTests(unittest.TestCase):
    """A Node tool's version is read from disk, never asked for."""

    def build(
        self,
        root: str,
        name: str,
        version: str,
    ) -> pathlib.Path:
        """Lay out a Node package with its bin shim inside it."""
        package = pathlib.Path(root) / "lib" / name
        package.mkdir(parents=True)
        (package / "package.json").write_text(
            json.dumps({"name": name, "version": version}),
            encoding="utf-8",
        )
        return executable(package, f"{name}-bin.mjs")

    def test_the_version_comes_from_the_package_beside_the_binary(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as root:
            binary = self.build(root, "markdownlint-cli2", "0.23.2")
            self.assertEqual(
                "0.23.2",
                node_package_version(binary, "markdownlint-cli2"),
            )

    def test_a_bin_symlink_is_followed_to_its_package(self) -> None:
        with tempfile.TemporaryDirectory() as root:
            binary = self.build(root, "markdownlint-cli2", "0.23.2")
            shims = pathlib.Path(root) / ".bin"
            shims.mkdir()
            link = shims / "markdownlint-cli2"
            link.symlink_to(binary)
            self.assertEqual(
                "0.23.2",
                node_package_version(link, "markdownlint-cli2"),
            )

    def test_an_unreadable_version_is_reported_as_unknown(self) -> None:
        with tempfile.TemporaryDirectory() as root:
            binary = self.build(root, "markdownlint-cli2", "0.23.2")
            self.assertIsNone(node_package_version(binary, "other-package"))

    def test_a_shim_that_is_not_a_symlink_still_resolves(self) -> None:
        # `npm ci --no-bin-links`, and filesystems without symlinks, leave a
        # real file in .bin whose parents never reach the owning package.
        with tempfile.TemporaryDirectory() as root:
            modules = pathlib.Path(root) / "node_modules"
            package = modules / "markdownlint-cli2"
            package.mkdir(parents=True)
            (package / "package.json").write_text(
                json.dumps({"name": "markdownlint-cli2", "version": "0.23.2"}),
                encoding="utf-8",
            )
            shims = modules / ".bin"
            shims.mkdir()
            shim = executable(shims, "markdownlint-cli2")
            self.assertFalse(shim.is_symlink())

            with mock.patch.object(dev_environment, "NODE_MODULES", modules):
                self.assertEqual(
                    "0.23.2",
                    node_package_version(shim, "markdownlint-cli2"),
                )

    def test_an_unreadable_manifest_does_not_abort_the_walk(self) -> None:
        with tempfile.TemporaryDirectory() as root:
            binary = self.build(root, "markdownlint-cli2", "0.23.2")
            # A malformed manifest between the binary and its package.
            (pathlib.Path(root) / "lib" / "package.json").write_text(
                "{ not json",
                encoding="utf-8",
            )
            self.assertEqual(
                "0.23.2",
                node_package_version(binary, "markdownlint-cli2"),
            )


class PaletteTests(unittest.TestCase):
    """Styling is for a terminal, and only ever for a terminal."""

    @contextlib.contextmanager
    def without_no_color(self) -> Any:
        """Run with NO_COLOR unset whatever the caller's shell exported."""
        with mock.patch.dict(os.environ):
            os.environ.pop("NO_COLOR", None)
            yield

    def test_a_disabled_palette_emits_no_escape_codes(self) -> None:
        style = Palette(enabled=False)
        self.assertEqual("taplo", style("taplo", "bold", "red"))

    def test_an_enabled_palette_wraps_and_resets(self) -> None:
        style = Palette(enabled=True)
        self.assertEqual(
            f"{Palette.CODES['red']}taplo{Palette.RESET}",
            style("taplo", "red"),
        )

    def test_color_is_off_for_a_redirected_stream(self) -> None:
        with self.without_no_color():
            self.assertFalse(palette(io.StringIO()).enabled)

    def test_no_color_disables_an_interactive_stream(self) -> None:
        stream = mock.Mock()
        stream.isatty.return_value = True
        with mock.patch.dict(os.environ, {"NO_COLOR": "1"}):
            self.assertFalse(palette(stream).enabled)

    def test_an_interactive_stream_without_no_color_is_styled(self) -> None:
        stream = mock.Mock()
        stream.isatty.return_value = True
        with self.without_no_color():
            self.assertTrue(palette(stream).enabled)


class DoctorReportingTests(unittest.TestCase):
    """The tally decides the exit status; every failure names a remedy."""

    def doctor(self) -> Doctor:
        return Doctor(Palette(enabled=False))

    def test_a_clean_run_exits_zero(self) -> None:
        checks = self.doctor()

        def render() -> int:
            checks.ok("taplo", "taplo 0.10.0")
            return checks.summary()

        status, output = captured(render)
        self.assertEqual(0, status)
        self.assertIn("all good", output)

    def test_a_failure_exits_one_and_lists_what_to_fix(self) -> None:
        checks = self.doctor()

        def render() -> int:
            checks.ok("taplo", "taplo 0.10.0")
            checks.fail("typos", "typos-cli 1.49.0", "pinned 1.48.0")
            return checks.summary()

        status, output = captured(render)
        self.assertEqual(1, status)
        self.assertIn("1 failed", output)
        self.assertIn("fix: typos", output)

    def test_a_failing_row_carries_its_remedy(self) -> None:
        checks = self.doctor()
        _, output = captured(
            lambda: checks.fail("typos", "not found", "run just setup")
        )
        self.assertIn("typos", output)
        self.assertIn("run just setup", output)

    def test_a_system_copy_passes_and_is_counted_apart(self) -> None:
        checks = self.doctor()
        _, output = captured(
            lambda: checks.ok_system(
                "taplo",
                "taplo 0.10.0",
                pathlib.Path("/usr/bin/taplo"),
            )
        )
        self.assertEqual([], checks.failed)
        self.assertEqual(1, checks.passes)
        self.assertEqual(1, checks.system)
        self.assertIn("system: /usr/bin/taplo", output)

    def test_every_manifest_label_fits_the_reported_column(self) -> None:
        manifest = load_manifest()
        rust = manifest["rust"]
        labels = [
            f"{rust['primary']} {item}"
            for item in (
                *rust["primary_components"],
                *rust["primary_targets"],
            )
        ]
        labels += [f"{rust['msrv']} {item}" for item in rust["msrv_components"]]
        # The sections doctor reports on, found by shape. A list here would be a fourth
        # place a section has to be named, and the one that fails silently: a name too long
        # for the column would break alignment rather than any check.
        for group in tool_sections(manifest) - INSTALLED_BY_CI:
            labels += [tool["binary"] for tool in tools(manifest, group)]
        for label in labels:
            with self.subTest(label=label):
                self.assertLessEqual(len(label), Doctor.LABEL_WIDTH)


class VersionParsingTests(unittest.TestCase):
    """Every banner doctor probes must yield the version it displays."""

    BANNERS = (
        ("just 1.58.0", (1, 58, 0)),
        ("taplo 0.10.0", (0, 10, 0)),
        ("typos-cli 1.49.0", (1, 49, 0)),
        ("cargo-llvm-cov 0.8.7", (0, 8, 7)),
        ("cargo-deny 0.20.2", (0, 20, 2)),
        ("cargo-nextest 0.9.143", (0, 9, 143)),
        ("markdownlint-cli2 v0.22.0 (markdownlint v0.40.0)", (0, 22, 0)),
        ("git version 2.55.0", (2, 55, 0)),
        ("rustup 1.29.0 (2026-03-23)", (1, 29, 0)),
        ("node v24.18.0", (24, 18, 0)),
        ("npm 11.16.0", (11, 16, 0)),
        ("Python 3.14.6", (3, 14, 6)),
    )

    def test_every_probed_banner_parses(self) -> None:
        for banner, expected in self.BANNERS:
            with self.subTest(banner=banner):
                self.assertEqual(expected, parse_version(banner))

    def test_a_build_date_is_not_mistaken_for_a_version(self) -> None:
        banner = "rustc 1.96.0 (ac68faa20 2026-05-25)"
        self.assertEqual((1, 96, 0), parse_version(banner))

    def test_the_version_may_sit_below_the_first_line(self) -> None:
        # ShellCheck names itself first and versions itself second.
        banner = (
            "ShellCheck - shell script analysis tool\n"
            "version: 0.11.0\n"
            "license: GNU General Public License, version 3"
        )
        self.assertEqual((0, 11, 0), parse_version(banner))

    def test_output_without_a_version_parses_to_nothing(self) -> None:
        for banner in ("", "no output", "not executable", "command not found"):
            with self.subTest(banner=banner):
                self.assertIsNone(parse_version(banner))


class VersionFloorTests(unittest.TestCase):
    """The pin is a floor, and the comparison is numeric."""

    def test_an_equal_version_satisfies_the_floor(self) -> None:
        self.assertTrue(satisfies_floor((0, 10, 0), (0, 10, 0)))

    def test_a_newer_version_satisfies_the_floor(self) -> None:
        self.assertTrue(satisfies_floor((1, 58, 0), (1, 57, 0)))

    def test_an_older_version_does_not(self) -> None:
        self.assertFalse(satisfies_floor((0, 22, 0), (0, 23, 2)))

    def test_the_comparison_is_numeric_and_not_lexical(self) -> None:
        # "0.9.140" sorts below "0.9.9" as text and above it as numbers.
        self.assertTrue(satisfies_floor((0, 9, 140), (0, 9, 9)))
        self.assertFalse(satisfies_floor((0, 9, 9), (0, 9, 140)))

    def test_a_shorter_version_is_padded_not_truncated(self) -> None:
        self.assertTrue(satisfies_floor((1, 58), (1, 58, 0)))
        self.assertFalse(satisfies_floor((1, 58), (1, 58, 1)))
        self.assertTrue(satisfies_floor((2,), (1, 99, 99)))


class FloorVerdictTests(unittest.TestCase):
    """check_floor turns a parsed version into a recorded verdict."""

    def doctor(self) -> Doctor:
        return Doctor(Palette(enabled=False))

    def judge(self, installed, pinned, path=None):
        """Run one floor verdict and return the doctor beside its output."""
        checks = self.doctor()
        _, output = captured(
            lambda: check_floor(
                checks,
                "taplo",
                "taplo banner",
                installed,
                pinned,
                "run just setup",
                path,
            )
        )
        return checks, output

    def test_a_satisfied_floor_shows_the_comparison(self) -> None:
        checks, output = self.judge((1, 58, 0), "1.57.0")
        self.assertEqual([], checks.failed)
        self.assertIn("≥ 1.57.0", output)

    def test_a_version_below_the_floor_fails_with_its_remedy(self) -> None:
        checks, output = self.judge((0, 22, 0), "0.23.2")
        self.assertEqual(["taplo"], checks.failed)
        self.assertIn("below the pinned 0.23.2", output)
        self.assertIn("run just setup", output)

    def test_an_unreadable_version_fails_rather_than_passing(self) -> None:
        checks, output = self.judge(None, "0.10.0")
        self.assertEqual(["taplo"], checks.failed)
        self.assertIn("no readable version", output)

    def test_an_unreadable_pin_is_reported_against_the_manifest(self) -> None:
        checks, output = self.judge((0, 10, 0), "not-a-version")
        self.assertEqual(["taplo"], checks.failed)
        self.assertIn("unreadable pin", output)

    def test_a_version_absent_from_the_banner_is_still_shown(self) -> None:
        # ShellCheck's first line carries no version; the row must still say
        # what was actually compared.
        checks = self.doctor()
        _, output = captured(
            lambda: check_floor(
                checks,
                "shellcheck",
                "ShellCheck - shell script analysis tool",
                (0, 11, 0),
                "0.11.0",
                "install it",
            )
        )
        self.assertEqual([], checks.failed)
        self.assertIn("0.11.0  ≥ 0.11.0", output)

    def test_a_system_copy_above_the_floor_still_counts_as_system(self) -> None:
        checks, output = self.judge(
            (1, 58, 0),
            "1.57.0",
            pathlib.Path("/usr/bin/just"),
        )
        self.assertEqual([], checks.failed)
        self.assertEqual(1, checks.system)
        self.assertIn("system: /usr/bin/just", output)


class RowWiringTests(unittest.TestCase):
    """A row's detail must never contradict the marker beside it."""

    def doctor(self) -> Doctor:
        return Doctor(Palette(enabled=False))

    def test_an_absent_component_reads_missing_not_installed(self) -> None:
        self.assertEqual("missing", rust_item_detail(True, False))
        self.assertEqual("installed", rust_item_detail(True, True))
        self.assertEqual("toolchain unavailable", rust_item_detail(False, False))

    def test_check_toolchain_never_calls_an_absent_component_installed(
        self,
    ) -> None:
        checks = self.doctor()
        with mock.patch.object(
            dev_environment, "capture", return_value=(0, "rustc 1.96.0")
        ), mock.patch.object(
            dev_environment, "installed_rust_items", return_value=(True, set())
        ):
            _, output = captured(
                lambda: check_toolchain(
                    checks, "primary compiler", "1.96.0", ["rust-src"]
                )
            )
        self.assertIn("1.96.0 rust-src", checks.failed)
        self.assertIn("missing", output)
        self.assertNotIn("rust-src                       installed", output)

    def test_check_targets_never_calls_an_absent_target_installed(self) -> None:
        checks = self.doctor()
        with mock.patch.object(
            dev_environment, "installed_rust_items", return_value=(True, set())
        ):
            _, output = captured(
                lambda: check_targets(
                    checks, "1.96.0", ["wasm32-unknown-unknown"]
                )
            )
        self.assertIn("1.96.0 wasm32-unknown-unknown", checks.failed)
        self.assertIn("missing", output)

    def test_a_bootstrap_command_that_exits_nonzero_says_so(self) -> None:
        checks = self.doctor()
        with mock.patch.object(
            dev_environment, "capture", return_value=(1, "boom")
        ), mock.patch("shutil.which", return_value="/usr/bin/cargo"):
            _, output = captured(
                lambda: check_bootstrap(checks, "cargo", ["--version"], "1.96.0")
            )
        self.assertEqual(["cargo"], checks.failed)
        self.assertIn("exited 1", output)
        # It never printed a version, so the row must not send the reader
        # looking for a version mismatch.
        self.assertNotIn("expected 1.96.0", output)


class SearchPathTests(unittest.TestCase):
    """An empty PATH entry means the working directory to shutil.which."""

    def test_an_unset_path_contributes_no_empty_entry(self) -> None:
        with mock.patch.dict(os.environ, {"PATH": ""}):
            entries = search_path().split(os.pathsep)
        self.assertNotIn("", entries)

    def test_interior_empty_entries_are_dropped(self) -> None:
        crafted = os.pathsep.join(["/usr/bin", "", "/bin"])
        with mock.patch.dict(os.environ, {"PATH": crafted}):
            entries = search_path().split(os.pathsep)
        self.assertNotIn("", entries)
        self.assertIn("/usr/bin", entries)
        self.assertIn("/bin", entries)


class ToolchainIdentityTests(unittest.TestCase):
    """Rust toolchains are addressed by exact name, so they are not floors."""

    def test_a_newer_toolchain_does_not_satisfy_an_exact_pin(self) -> None:
        banner = "rustc 1.97.0 (aaaaaaaaa 2026-07-01)"
        self.assertFalse(contains_version(banner, "1.96.0"))
        self.assertTrue(contains_version(banner, "1.97.0"))

    def test_the_exact_check_is_not_fooled_by_a_longer_version(self) -> None:
        self.assertFalse(contains_version("rustc 1.96.01", "1.96.0"))
        self.assertFalse(contains_version("rustc 11.96.0", "1.96.0"))


if __name__ == "__main__":
    unittest.main()
