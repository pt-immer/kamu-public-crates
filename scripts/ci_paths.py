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

# A change to any of these bears on every crate: the lockfile and root manifest
# resolve them all, the Justfile defines every recipe, the workflows define every
# job, and `scripts/` holds this classifier. They select every class in
# DERIVED_CLASSES that lists `shared`, which is why editing one runs the matrix.
#
# Routing `scripts/test_*.py` somewhere narrower was considered and refused. It
# would be sound — the `changes` job runs this module and never its tests — but
# it buys a class of its own, and therefore one more edge to justify, for edits
# that almost always accompany a change to the script beside them.
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
    "rust-toolchain.toml",
    "rustfmt.toml",
    "taplo.toml",
    "typos.toml",
}

# Every class a workflow job gates on, the base classes that select it, and why
# working on those selects it. The reason is a required field: `test_ci_paths.py`
# refuses an entry without one, so a class cannot be added without answering why
# a change to A runs B's jobs.
#
# No entry is narrowed by inspecting diff content. This module receives paths,
# not hunks; a job missed by a content heuristic fails quietly, and the
# fail-closed direction is the one that cannot certify an unproven change.
DERIVED_CLASSES: dict[str, tuple[tuple[str, ...], str]] = {
    "rust": (
        ("iso3166", "logging", "money", "snap", "shared"),
        "fmt, workspace Clippy and the workspace test job resolve every member "
        "in one graph, so any member's source is an input to all of them",
    ),
    "iso": (("iso3166", "shared"), "the kamu-iso3166 jobs"),
    "log": (("logging", "shared"), "the kamu-logging jobs"),
    "money": (("money", "shared"), "the kamu-money-core jobs"),
    "snap": (
        ("snap", "shared"),
        "one class for six crates: they depend on each other, so testing one "
        "without the others proves less than it appears to",
    ),
    "moneypg": (
        ("moneypg", "shared"),
        "the excluded lane patches kamu-money-core to a local path and compiles "
        "it, so that crate's package inputs are inputs to this lane as well",
    ),
    "worker": (
        ("logging", "shared"),
        "the Cloudflare Worker example is a separate workspace whose only "
        "first-party dependency is kamu-logging's wasm feature set",
    ),
    "lint": (
        tuple(sorted(BASE_CLASSES)),
        "formatting, spelling, Markdown and TOML checks read files rather than "
        "crates, so every classified change is in scope for them",
    ),
    "shell": (
        ("shell",),
        "deliberately without `shared`: ShellCheck reads exactly the .sh files "
        "this class already tracks, so a Justfile or workflow edit changes "
        "nothing it looks at",
    ),
}

SHARED_PREFIXES = (
    ".cargo/",
    ".config/",
    ".fso-amem/",
    ".github/actions/",
    ".github/workflows/",
    "scripts/",
    "tools/",
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
        relative = path.removeprefix("crates/money-core/")
        # The extension lane patches kamu-money-core to this path and compiles it,
        # so its compiled package inputs select that lane as well.
        #
        # Unit tests live inline under `src/`, and a dependency's `#[cfg(test)]`
        # code is never compiled, so a test-only edit selects a lane it cannot
        # affect. That over-selection is deliberate. This function receives paths,
        # not diffs; deciding from hunk ranges would make a missed lane run the
        # quiet failure, and a missed lane run is how a release gate goes unproven.
        if (
            relative in {"Cargo.toml", "build.rs"}
            or relative.startswith(("build/", "src/", "vendor/"))
        ):
            classes.add("moneypg")
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

    # Derived values read the base snapshot, because some of them reuse a base
    # class name and would otherwise consume a value they had just replaced.
    base = dict(classes)
    classes.update(
        {
            name: any(base[source] for source in sources)
            for name, (sources, _reason) in DERIVED_CLASSES.items()
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
