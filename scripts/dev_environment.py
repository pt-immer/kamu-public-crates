#!/usr/bin/env python3
"""Install and verify the repository's pinned development environment."""

from __future__ import annotations

import argparse
import json
import pathlib
import re
import shutil
import subprocess
import sys
from collections.abc import Sequence
from typing import Any


ROOT = pathlib.Path(__file__).resolve().parent.parent
MANIFEST_PATH = ROOT / ".config" / "dev-tools.json"
TOOLS_BIN = ROOT / ".tools" / "bin"
NODE_MODULES = ROOT / "node_modules"


def load_manifest() -> dict[str, Any]:
    """Load the single source of truth for local and CI tool versions."""
    return json.loads(MANIFEST_PATH.read_text(encoding="utf-8"))


def render(command: Sequence[str]) -> str:
    """Render a command for progress output without invoking a shell."""
    return " ".join(command)


def run(command: Sequence[str]) -> None:
    """Run one setup command from the repository root."""
    print(f"+ {render(command)}", flush=True)
    subprocess.run(command, cwd=ROOT, check=True)


def capture(command: Sequence[str]) -> tuple[int, str]:
    """Run one diagnostic command and combine its output streams."""
    try:
        result = subprocess.run(
            command,
            cwd=ROOT,
            check=False,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
        )
    except OSError as error:
        return 127, str(error)
    return result.returncode, result.stdout.strip()


def contains_version(output: str, version: str) -> bool:
    """Return whether output contains one exact dotted version token."""
    return re.search(
        rf"(?<![0-9.]){re.escape(version)}(?![0-9.])",
        output,
    ) is not None


def local_tool_is_exact(tool: dict[str, Any]) -> bool:
    """Check one repository-local Cargo binary against its manifest version."""
    binary = TOOLS_BIN / tool["binary"]
    if not binary.is_file():
        return False
    status, output = capture([str(binary), *tool["version_args"]])
    return status == 0 and contains_version(output, tool["version"])


def setup_commands(manifest: dict[str, Any]) -> list[list[str]]:
    """Build the deterministic non-Cargo portion of the setup command list."""
    rust = manifest["rust"]
    commands = [["git", "submodule", "update", "--init", "--recursive"]]

    for version, component_key in (
        (rust["primary"], "primary_components"),
        (rust["msrv"], "msrv_components"),
    ):
        command = [
            "rustup",
            "toolchain",
            "install",
            version,
            "--profile",
            "minimal",
        ]
        for component in rust[component_key]:
            command.extend(["--component", component])
        commands.append(command)

    commands.append(
        [
            "rustup",
            "target",
            "add",
            "--toolchain",
            rust["primary"],
            *rust["primary_targets"],
        ]
    )
    commands.append(["npm", "ci", "--no-fund", "--no-audit"])
    return commands


def cargo_install_command(
    primary: str,
    tool: dict[str, Any],
) -> list[str]:
    """Build one exact repository-local Cargo install command."""
    return [
        "rustup",
        "run",
        primary,
        "cargo",
        "install",
        "--locked",
        "--force",
        "--root",
        str(TOOLS_BIN.parent),
        "--version",
        f"={tool['version']}",
        tool["crate"],
    ]


def setup(manifest: dict[str, Any]) -> int:
    """Install exact toolchains, targets, and repository-local tools."""
    missing = [
        command
        for command in ("git", "rustup", "cargo", "node", "npm")
        if shutil.which(command) is None
    ]
    if missing:
        print(
            "setup: install bootstrap command(s) first: "
            + ", ".join(missing),
            file=sys.stderr,
        )
        return 1

    commands = setup_commands(manifest)
    for command in commands[:-1]:
        run(command)

    TOOLS_BIN.parent.mkdir(parents=True, exist_ok=True)
    primary = manifest["rust"]["primary"]
    for tool in manifest["cargo_tools"]:
        if local_tool_is_exact(tool):
            print(
                f"= {tool['binary']} {tool['version']} already installed",
                flush=True,
            )
            continue
        run(cargo_install_command(primary, tool))

    run(commands[-1])
    return doctor(manifest)


class Doctor:
    """Accumulate gate-prerequisite diagnostics."""

    def __init__(self) -> None:
        self.failures = 0

    def report(self, label: str, passed: bool, detail: str) -> None:
        """Print one diagnostic and remember failures."""
        marker = "PASS" if passed else "FAIL"
        print(f"  {marker:<4} {label:<28} {detail}")
        if not passed:
            self.failures += 1

    def command(
        self,
        label: str,
        command: Sequence[str],
        *,
        version: str | None = None,
    ) -> None:
        """Require one command and optionally its exact version."""
        status, output = capture(command)
        first_line = output.splitlines()[0] if output else "no output"
        passed = status == 0 and (
            version is None or contains_version(output, version)
        )
        detail = first_line
        if version is not None and not passed:
            detail = f"expected {version}; got {first_line}"
        self.report(label, passed, detail)


def installed_rust_items(
    toolchain: str,
    item: str,
) -> tuple[bool, set[str]]:
    """Read installed rustup components or targets for one toolchain."""
    status, output = capture(
        ["rustup", item, "list", "--toolchain", toolchain, "--installed"]
    )
    return status == 0, set(output.splitlines())


def component_present(installed: set[str], component: str) -> bool:
    """Match rustup's target-qualified component names."""
    return component in installed or any(
        item.startswith(f"{component}-") for item in installed
    )


def doctor(manifest: dict[str, Any]) -> int:
    """Verify every prerequisite reached by the root gate."""
    checks = Doctor()
    rust = manifest["rust"]
    print("bootstrap commands:")
    for command, args, version in (
        ("git", ["--version"], None),
        ("rustup", ["--version"], None),
        ("cargo", ["--version"], rust["primary"]),
        ("rustc", ["--version"], rust["primary"]),
        ("python3", ["--version"], None),
        ("node", ["--version"], None),
        ("npm", ["--version"], None),
    ):
        path = shutil.which(command)
        if path is None:
            checks.report(command, False, "missing")
        else:
            checks.command(command, [path, *args], version=version)

    print("Rust toolchains and components:")
    for label, version, component_key in (
        ("primary compiler", rust["primary"], "primary_components"),
        ("MSRV compiler", rust["msrv"], "msrv_components"),
    ):
        checks.command(
            label,
            ["rustup", "run", version, "rustc", "--version"],
            version=version,
        )
        available, installed = installed_rust_items(version, "component")
        for component in rust[component_key]:
            present = available and component_present(installed, component)
            detail = "toolchain unavailable"
            if available:
                detail = "installed" if present else "missing"
            checks.report(
                f"{version} {component}",
                present,
                detail,
            )

    available, targets = installed_rust_items(rust["primary"], "target")
    for target in rust["primary_targets"]:
        present = available and target in targets
        detail = "toolchain unavailable"
        if available:
            detail = "installed" if present else "missing"
        checks.report(
            f"{rust['primary']} {target}",
            present,
            detail,
        )

    print("repository-local tools:")
    for tool in manifest["cargo_tools"]:
        binary = TOOLS_BIN / tool["binary"]
        checks.command(
            tool["binary"],
            [str(binary), *tool["version_args"]],
            version=tool["version"],
        )

    for tool in manifest["node_tools"]:
        package = NODE_MODULES / tool["package"] / "package.json"
        binary = NODE_MODULES / ".bin" / tool["binary"]
        actual = None
        if package.is_file():
            actual = json.loads(package.read_text(encoding="utf-8")).get(
                "version"
            )
        checks.report(
            tool["binary"],
            actual == tool["version"] and binary.is_file(),
            (
                str(actual)
                if actual == tool["version"] and binary.is_file()
                else f"expected {tool['version']}; got {actual or 'missing'}"
            ),
        )

    print("system tools:")
    for tool in manifest["system_tools"]:
        path = shutil.which(tool["binary"])
        if path is None:
            checks.report(tool["binary"], False, tool["install_hint"])
        else:
            checks.command(
                tool["binary"],
                [path, *tool["version_args"]],
                version=tool["version"],
            )

    print("vendored data:")
    submodule_file = (
        ROOT
        / "crates"
        / "iso3166"
        / "vendor"
        / "iso3166-csv"
        / "countries.csv"
    )
    checks.report(
        "ISO 3166 submodule",
        submodule_file.is_file(),
        "initialized" if submodule_file.is_file() else "run setup",
    )

    if checks.failures:
        print(f"doctor: {checks.failures} required prerequisite(s) failed")
        return 1
    print("doctor: every root-gate prerequisite is ready")
    return 0


def parser() -> argparse.ArgumentParser:
    """Build the command-line parser."""
    result = argparse.ArgumentParser(description=__doc__)
    result.add_argument("command", choices=("setup", "doctor"))
    return result


def main(argv: Sequence[str] | None = None) -> int:
    """Run setup or doctor."""
    args = parser().parse_args(argv)
    manifest = load_manifest()
    if args.command == "setup":
        return setup(manifest)
    return doctor(manifest)


if __name__ == "__main__":
    raise SystemExit(main())
