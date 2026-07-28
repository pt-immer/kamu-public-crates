#!/usr/bin/env python3
"""Classify changed paths for CI and reject unknown repository surfaces."""

from __future__ import annotations

import argparse
import os
import subprocess
import sys
from collections.abc import Iterable
from pathlib import PurePosixPath

BASE_CLASSES = {
    "iso3166",
    "logging",
    "money",
    "moneypg",
    "snap",
    "shared",
    "docs",
    "shell",
}

SHARED_FILES = {
    ".editorconfig",
    ".gitignore",
    ".gitmodules",
    ".mcp.json",
    "Cargo.lock",
    "Cargo.toml",
    "Justfile",
    "LICENSE-APACHE",
    "LICENSE-MIT",
    "clippy.toml",
    "deny.toml",
    "package-lock.json",
    "package.json",
    "rustfmt.toml",
    "taplo.toml",
    "typos.toml",
}

SHARED_PREFIXES = (
    ".cargo/",
    ".config/",
    ".fso-amem/",
    ".github/actions/",
    ".github/workflows/",
    "scripts/",
)

SHARED_GITHUB_FILES = {
    ".github/CODEOWNERS",
    ".github/dependabot.yml",
}

DOC_CONFIG_FILES = {
    ".markdownlint-cli2.jsonc",
    "taplo.toml",
    "typos.toml",
}


def classify_path(raw_path: str) -> set[str]:
    """Return every CI class owning one repository-relative path."""
    path = PurePosixPath(raw_path).as_posix()
    classes: set[str] = set()

    if path.startswith("crates/iso3166/"):
        classes.add("iso3166")
    elif path.startswith("crates/logging/"):
        classes.add("logging")
    elif path.startswith("crates/money-core/"):
        classes.add("money")
    elif path.startswith("crates/snap-"):
        classes.add("snap")
    elif path.startswith("extensions/money-pg/"):
        classes.add("moneypg")

    if path.endswith(".md") or path in DOC_CONFIG_FILES:
        classes.add("docs")
    if path.endswith(".sh"):
        classes.add("shell")
    if (
        path in SHARED_FILES
        or path in SHARED_GITHUB_FILES
        or path.startswith(SHARED_PREFIXES)
    ):
        classes.add("shared")

    return classes


def classify_paths(paths: Iterable[str]) -> dict[str, bool]:
    """Classify paths and raise when any path has no declared owner."""
    path_list = sorted({path for path in paths if path})
    classes = {name: False for name in BASE_CLASSES}
    unmatched: list[str] = []

    for path in path_list:
        owned = classify_path(path)
        if not owned:
            unmatched.append(path)
        for name in owned:
            classes[name] = True

    if unmatched:
        rendered = "\n".join(f"  - {path}" for path in unmatched)
        raise ValueError(
            "CI path classification is incomplete. Add an explicit owner for:\n"
            f"{rendered}"
        )

    shared = classes["shared"]
    classes.update(
        {
            "rust": any(
                classes[name]
                for name in ("iso3166", "logging", "money", "snap")
            )
            or shared,
            "iso": classes["iso3166"] or shared,
            "log": classes["logging"] or shared,
            "money": classes["money"] or shared,
            "snap": classes["snap"] or shared,
            "moneypg": classes["moneypg"] or shared,
            "lint": any(classes.values()),
            "worker": classes["logging"] or shared,
        }
    )
    return classes


def git_paths(*args: str) -> list[str]:
    """Read NUL-delimited paths from a Git command."""
    result = subprocess.run(
        ["git", *args],
        check=True,
        stdout=subprocess.PIPE,
    )
    return [
        item.decode("utf-8", "surrogateescape")
        for item in result.stdout.split(b"\0")
        if item
    ]


def changed_paths(base: str, head: str) -> list[str]:
    """Return every changed path, including deletions, renames, and gitlinks."""
    return git_paths(
        "diff",
        "--name-only",
        "-z",
        "--find-renames",
        base,
        head,
        "--",
    )


def tracked_paths() -> list[str]:
    """Return every tracked path in the current worktree."""
    return git_paths("ls-files", "-z")


def write_github_outputs(values: dict[str, bool], destination: str) -> None:
    """Append boolean values to a GitHub Actions output file."""
    with open(destination, "a", encoding="utf-8") as output:
        for name in sorted(values):
            output.write(f"{name}={str(values[name]).lower()}\n")


def parser() -> argparse.ArgumentParser:
    result = argparse.ArgumentParser(description=__doc__)
    subparsers = result.add_subparsers(dest="command", required=True)

    emit = subparsers.add_parser(
        "emit",
        help="classify a Git diff and write GitHub Actions outputs",
    )
    emit.add_argument("--base", required=True)
    emit.add_argument("--head", required=True)
    emit.add_argument(
        "--github-output",
        default=os.environ.get("GITHUB_OUTPUT"),
    )

    subparsers.add_parser(
        "check-tracked",
        help="prove every currently tracked path has at least one owner",
    )

    classify = subparsers.add_parser(
        "classify",
        help="classify explicit fixture paths",
    )
    classify.add_argument("paths", nargs="+")

    return result


def main(argv: list[str] | None = None) -> int:
    args = parser().parse_args(argv)
    try:
        if args.command == "emit":
            paths = changed_paths(args.base, args.head)
            values = classify_paths(paths)
            if not args.github_output:
                raise ValueError("--github-output or GITHUB_OUTPUT is required")
            write_github_outputs(values, args.github_output)
        elif args.command == "check-tracked":
            paths = tracked_paths()
            classify_paths(paths)
        else:
            paths = args.paths
            values = classify_paths(paths)
            for name in sorted(values):
                print(f"{name}={str(values[name]).lower()}")
            return 0
    except (OSError, subprocess.CalledProcessError, ValueError) as error:
        print(f"ci-paths: {error}", file=sys.stderr)
        return 1

    print(f"ci-paths: classified {len(paths)} path(s)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
