#!/usr/bin/env python3
"""Install and verify the repository's pinned development environment."""

from __future__ import annotations

import argparse
import json
import os
import pathlib
import re
import shutil
import subprocess
import sys
from collections.abc import Sequence
from typing import Any, TextIO


ROOT = pathlib.Path(__file__).resolve().parent.parent
MANIFEST_PATH = ROOT / ".config" / "dev-tools.json"
TOOLS_BIN = ROOT / ".tools" / "bin"
NODE_MODULES = ROOT / "node_modules"
NODE_BIN = NODE_MODULES / ".bin"

# The Justfile exports this prefix order ahead of PATH, so a recipe runs the
# repository-local copy where setup installed one and the system copy where it
# did not. Doctor searches the same order, or it reports on a binary no recipe
# would ever run.
SEARCH_PREFIXES = (TOOLS_BIN, NODE_BIN)
SETUP_HINT = "run just setup"


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
    except OSError:
        # The OSError text is an absolute host path and no remedy. Callers
        # describe the absence themselves, in words a reader can act on.
        return 127, "not executable"
    return result.returncode, result.stdout.strip()


def first_line(output: str) -> str:
    """Reduce a version banner to the one line worth displaying."""
    return output.splitlines()[0] if output else "no output"


def contains_version(output: str, version: str) -> bool:
    """Return whether output contains one exact dotted version token."""
    return re.search(
        rf"(?<![0-9.]){re.escape(version)}(?![0-9.])",
        output,
    ) is not None


def parse_version(output: str) -> tuple[int, ...] | None:
    """Read the first dotted version out of a tool's banner.

    Scans the whole banner rather than its first line: ShellCheck prints its
    name on line one and `version: 0.11.0` on line two.
    """
    found = re.search(r"(?<![0-9.])(\d+(?:\.\d+)+)(?![0-9.])", output)
    if found is None:
        return None
    return tuple(int(part) for part in found.group(1).split("."))


def satisfies_floor(
    installed: tuple[int, ...],
    floor: tuple[int, ...],
) -> bool:
    """Report whether an installed version is at least the pinned one.

    Compares as integers, never as text: `0.9.140` is above `0.9.9` by number
    and below it by string order.
    """
    width = max(len(installed), len(floor))
    left = installed + (0,) * (width - len(installed))
    right = floor + (0,) * (width - len(floor))
    return left >= right


def search_path() -> str:
    """Build the tool search path the Justfile exports for every recipe."""
    return os.pathsep.join(
        [str(prefix) for prefix in SEARCH_PREFIXES]
        + [os.environ.get("PATH", "")]
    )


def resolve(binary: str) -> pathlib.Path | None:
    """Find one tool the way a recipe does: repository-local, then PATH."""
    found = shutil.which(binary, path=search_path())
    return pathlib.Path(found) if found else None


def is_repository_local(path: pathlib.Path) -> bool:
    """Report whether a resolved tool is the copy setup installs."""
    return path.parent in SEARCH_PREFIXES


def node_package_version(
    binary: pathlib.Path,
    package: str,
) -> str | None:
    """Read a Node tool's version from the package.json beside its binary.

    Asking the binary is not an option: markdownlint-cli2 treats every
    argument as a glob, so a version query lints the whole repository.
    """
    for parent in binary.resolve().parents:
        manifest = parent / "package.json"
        if not manifest.is_file():
            continue
        try:
            data = json.loads(manifest.read_text(encoding="utf-8"))
        except (OSError, ValueError):
            return None
        if data.get("name") == package:
            version = data.get("version")
            return None if version is None else str(version)
    return None


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


class Palette:
    """ANSI styling that disappears for a pipe, a file, or NO_COLOR."""

    RESET = "\033[0m"
    CODES = {
        "bold": "\033[1m",
        "dim": "\033[2m",
        "red": "\033[31m",
        "green": "\033[32m",
        "yellow": "\033[33m",
        "cyan": "\033[36m",
    }

    def __init__(self, enabled: bool) -> None:
        self.enabled = enabled

    def __call__(self, text: str, *styles: str) -> str:
        """Wrap text in the named styles, or return it untouched."""
        if not self.enabled or not text:
            return text
        opening = "".join(self.CODES[style] for style in styles)
        return f"{opening}{text}{self.RESET}"


def palette(stream: TextIO | None = None) -> Palette:
    """Enable color only for an interactive stream without NO_COLOR set."""
    target = sys.stdout if stream is None else stream
    interactive = bool(getattr(target, "isatty", lambda: False)())
    return Palette(interactive and not os.environ.get("NO_COLOR"))


class Doctor:
    """Accumulate gate-prerequisite diagnostics and render them."""

    LABEL_WIDTH = 30

    def __init__(self, style: Palette) -> None:
        self.style = style
        self.passes = 0
        self.system = 0
        self.failed: list[str] = []

    def section(self, title: str) -> None:
        """Open a titled group of checks."""
        print(f"\n{self.style(title, 'bold', 'cyan')}")

    def row(self, marker: str, label: str, detail: str) -> None:
        """Print one aligned diagnostic line."""
        print(f"  {marker} {label:<{self.LABEL_WIDTH}} {detail}")

    def ok(self, label: str, detail: str) -> None:
        """Record one satisfied prerequisite."""
        self.row(self.style("✓", "green"), label, self.style(detail, "dim"))
        self.passes += 1

    def ok_system(
        self,
        label: str,
        detail: str,
        path: pathlib.Path,
    ) -> None:
        """Record a pinned tool served from outside the repository."""
        self.row(
            self.style("•", "yellow"),
            label,
            f"{self.style(detail, 'dim')}  "
            f"{self.style(f'(system: {path})', 'dim')}",
        )
        self.passes += 1
        self.system += 1

    def fail(self, label: str, detail: str, hint: str) -> None:
        """Record one missing or mismatched prerequisite."""
        self.row(
            self.style("✗", "red"),
            label,
            self.style(f"{detail} — {hint}", "red"),
        )
        self.failed.append(label)

    def verdict(
        self,
        label: str,
        passed: bool,
        detail: str,
        hint: str,
    ) -> None:
        """Record a check whose only outcome is present or absent."""
        if passed:
            self.ok(label, detail)
        else:
            self.fail(label, detail, hint)

    def summary(self) -> int:
        """Print the closing tally and return the process exit status."""
        style = self.style
        counts = (
            f"{style(f'✓ {self.passes} ok', 'green')}   "
            f"{style(f'• {self.system} system', 'yellow')}"
        )
        print()
        if self.failed:
            print(
                f"{style(f'✗ {len(self.failed)} failed', 'bold', 'red')}"
                f"   {counts}"
            )
            print(style(f"  fix: {', '.join(self.failed)}", "red"))
            return 1
        print(f"{style('✓ all good', 'bold', 'green')}   {counts}")
        return 0


def check_bootstrap(
    checks: Doctor,
    command: str,
    args: Sequence[str],
    version: str | None,
) -> None:
    """Check one command that setup itself cannot install."""
    path = shutil.which(command)
    if path is None:
        checks.fail(command, "not found", "install it before setup")
        return
    status, output = capture([path, *args])
    checks.verdict(
        command,
        status == 0
        and (version is None or contains_version(output, version)),
        first_line(output),
        "not runnable" if version is None else f"expected {version}",
    )


def check_floor(
    checks: Doctor,
    label: str,
    detail: str,
    installed: tuple[int, ...] | None,
    pinned: str,
    hint: str,
    path: pathlib.Path | None = None,
) -> None:
    """Judge one tool against its pinned floor and record the verdict.

    A version that cannot be read is reported as unreadable, never as
    satisfied: a check that cannot see its input has not passed it.
    """
    floor = parse_version(pinned)
    if floor is None:
        checks.fail(label, detail, f"unreadable pin {pinned} in the manifest")
    elif installed is None:
        checks.fail(label, detail, f"no readable version; {hint}")
    elif not satisfies_floor(installed, floor):
        checks.fail(label, detail, f"below the pinned {pinned}; {hint}")
    else:
        # ShellCheck names itself on line one and versions itself on line two,
        # so the banner alone does not always show what was compared.
        found = ".".join(str(part) for part in installed)
        shown = detail if found in detail else f"{detail} {found}"
        if path is None or is_repository_local(path):
            checks.ok(label, f"{shown}  ≥ {pinned}")
        else:
            checks.ok_system(label, f"{shown}  ≥ {pinned}", path)


def check_cargo_tool(checks: Doctor, tool: dict[str, Any]) -> None:
    """Check one pinned Cargo tool wherever a recipe would find it."""
    binary = tool["binary"]
    path = resolve(binary)
    if path is None:
        checks.fail(binary, "not found", SETUP_HINT)
        return
    status, output = capture([str(path), *tool["version_args"]])
    check_floor(
        checks,
        binary,
        first_line(output),
        parse_version(output) if status == 0 else None,
        tool["version"],
        SETUP_HINT,
        path,
    )


def check_node_tool(checks: Doctor, tool: dict[str, Any]) -> None:
    """Check one pinned Node tool without invoking it."""
    binary = tool["binary"]
    path = resolve(binary)
    if path is None:
        checks.fail(binary, "not found", SETUP_HINT)
        return
    actual = node_package_version(path, tool["package"])
    if actual is None:
        checks.fail(
            binary,
            "version unreadable",
            f"no {tool['package']} package.json beside it; {SETUP_HINT}",
        )
        return
    check_floor(
        checks,
        binary,
        f"{binary} {actual}",
        parse_version(actual),
        tool["version"],
        SETUP_HINT,
        path,
    )


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


def check_toolchain(
    checks: Doctor,
    label: str,
    version: str,
    components: Sequence[str],
) -> None:
    """Check one pinned compiler and every component the gates reach."""
    status, output = capture(["rustup", "run", version, "rustc", "--version"])
    checks.verdict(
        label,
        status == 0 and contains_version(output, version),
        first_line(output),
        f"expected {version}; {SETUP_HINT}",
    )
    available, installed = installed_rust_items(version, "component")
    for component in components:
        checks.verdict(
            f"{version} {component}",
            available and component_present(installed, component),
            "installed" if available else "toolchain unavailable",
            f"rustup component add {component} --toolchain {version}",
        )


def check_targets(
    checks: Doctor,
    version: str,
    targets: Sequence[str],
) -> None:
    """Check every cross-compilation target the gates build for."""
    available, installed = installed_rust_items(version, "target")
    for target in targets:
        checks.verdict(
            f"{version} {target}",
            available and target in installed,
            "installed" if available else "toolchain unavailable",
            f"rustup target add {target} --toolchain {version}",
        )


def check_system_tool(checks: Doctor, tool: dict[str, Any]) -> None:
    """Check one tool the operating system package manager owns."""
    path = shutil.which(tool["binary"])
    if path is None:
        checks.fail(tool["binary"], "not found", tool["install_hint"])
        return
    status, output = capture([path, *tool["version_args"]])
    check_floor(
        checks,
        tool["binary"],
        first_line(output),
        parse_version(output) if status == 0 else None,
        tool["version"],
        tool["install_hint"],
    )


def doctor(manifest: dict[str, Any]) -> int:
    """Verify every prerequisite reached by the root gate."""
    style = palette()
    checks = Doctor(style)
    rust = manifest["rust"]

    print(
        f"\n{style('kamu · doctor', 'bold', 'cyan')}  "
        f"{style('root-gate prerequisites', 'dim')}"
    )

    checks.section("Bootstrap commands")
    for command, args, version in (
        ("git", ["--version"], None),
        ("rustup", ["--version"], None),
        ("cargo", ["--version"], rust["primary"]),
        ("rustc", ["--version"], rust["primary"]),
        ("python3", ["--version"], None),
        ("node", ["--version"], None),
        ("npm", ["--version"], None),
    ):
        check_bootstrap(checks, command, args, version)

    checks.section("Rust toolchains and components")
    check_toolchain(
        checks,
        "primary compiler",
        rust["primary"],
        rust["primary_components"],
    )
    check_toolchain(
        checks,
        "MSRV compiler",
        rust["msrv"],
        rust["msrv_components"],
    )
    check_targets(checks, rust["primary"], rust["primary_targets"])

    checks.section("Repository tools")
    for tool in manifest["cargo_tools"]:
        check_cargo_tool(checks, tool)
    for tool in manifest["node_tools"]:
        check_node_tool(checks, tool)

    checks.section("System tools")
    for tool in manifest["system_tools"]:
        check_system_tool(checks, tool)

    checks.section("Vendored data")
    submodule_file = (
        ROOT
        / "crates"
        / "iso3166"
        / "vendor"
        / "iso3166-csv"
        / "countries.csv"
    )
    checks.verdict(
        "ISO 3166 submodule",
        submodule_file.is_file(),
        "initialized",
        f"not initialized; {SETUP_HINT}",
    )

    return checks.summary()


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
