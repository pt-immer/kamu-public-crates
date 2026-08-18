#!/usr/bin/env python3
"""Fail-closed crates.io sparse-index probes with Cargo requirement matching."""

from __future__ import annotations

import argparse
import json
import os
import re
import sys
import time
import urllib.error
import urllib.request
from dataclasses import dataclass
from functools import total_ordering
from typing import Any

INDEX_ROOT = "https://index.crates.io"
USER_AGENT = "kamu-public-crates-ci (https://github.com/pt-immer/kamu-public-crates)"

EXIT_ANSWERED_NO = 1
EXIT_UNREADABLE = 2

POLL_SECONDS = 15


@total_ordering
@dataclass(frozen=True)
class Version:
    """A SemVer value sufficient for Cargo registry requirements."""

    major: int
    minor: int
    patch: int
    prerelease: tuple[int | str, ...] = ()

    @classmethod
    def parse(cls, raw: str) -> "Version":
        match = re.fullmatch(
            r"(?P<major>0|[1-9]\d*)"
            r"(?:\.(?P<minor>0|[1-9]\d*))?"
            r"(?:\.(?P<patch>0|[1-9]\d*))?"
            r"(?:-(?P<pre>[0-9A-Za-z.-]+))?"
            r"(?:\+[0-9A-Za-z.-]+)?",
            raw.strip(),
        )
        if not match:
            raise ValueError(f"invalid SemVer: {raw!r}")
        prerelease: tuple[int | str, ...] = tuple(
            int(part) if part.isdigit() else part
            for part in (match.group("pre") or "").split(".")
            if part
        )
        return cls(
            int(match.group("major")),
            int(match.group("minor") or 0),
            int(match.group("patch") or 0),
            prerelease,
        )

    def __lt__(self, other: object) -> bool:
        if not isinstance(other, Version):
            return NotImplemented
        stable_self = (self.major, self.minor, self.patch)
        stable_other = (other.major, other.minor, other.patch)
        if stable_self != stable_other:
            return stable_self < stable_other
        if not self.prerelease:
            return False
        if not other.prerelease:
            return True
        for left, right in zip(self.prerelease, other.prerelease, strict=False):
            if left == right:
                continue
            if isinstance(left, int) and isinstance(right, str):
                return True
            if isinstance(left, str) and isinstance(right, int):
                return False
            return left < right
        return len(self.prerelease) < len(other.prerelease)

    def __str__(self) -> str:
        base = f"{self.major}.{self.minor}.{self.patch}"
        if not self.prerelease:
            return base
        return f"{base}-" + ".".join(str(part) for part in self.prerelease)


def _components(raw: str) -> tuple[list[int], bool]:
    core = raw.split("-", 1)[0]
    parts = core.split(".")
    wildcard = any(part.lower() in {"*", "x"} for part in parts)
    numeric = [int(part) for part in parts if part.lower() not in {"*", "x"}]
    return numeric, wildcard


def _caret_upper(version: Version, component_count: int) -> Version:
    if version.major > 0:
        return Version(version.major + 1, 0, 0)
    if component_count == 1:
        return Version(1, 0, 0)
    if version.minor > 0:
        return Version(0, version.minor + 1, 0)
    if component_count == 2:
        return Version(0, 1, 0)
    return Version(0, 0, version.patch + 1)


def _matches_one(version: Version, raw_comparator: str) -> bool:
    comparator = raw_comparator.strip()
    if not comparator or comparator == "*":
        return True

    match = re.fullmatch(r"(>=|<=|>|<|=|\^|~)?\s*(.+)", comparator)
    if not match:
        raise ValueError(f"unsupported Cargo comparator: {comparator!r}")
    operator = match.group(1) or "^"
    raw_version = match.group(2).strip()
    numeric, wildcard = _components(raw_version)

    if wildcard:
        if not numeric:
            return True
        lower = Version(*(numeric + [0] * (3 - len(numeric))))
        if len(numeric) == 1:
            upper = Version(lower.major + 1, 0, 0)
        else:
            upper = Version(lower.major, lower.minor + 1, 0)
        return lower <= version < upper

    target = Version.parse(raw_version)
    if operator == ">=":
        return version >= target
    if operator == "<=":
        return version <= target
    if operator == ">":
        return version > target
    if operator == "<":
        return version < target
    if operator == "=":
        return version == target
    if operator == "~":
        upper = (
            Version(target.major + 1, 0, 0)
            if len(numeric) == 1
            else Version(target.major, target.minor + 1, 0)
        )
        return target <= version < upper

    return target <= version < _caret_upper(target, len(numeric))


def matches_requirement(version: Version, requirement: str) -> bool:
    """Return whether a stable registry version satisfies a Cargo requirement."""
    comparators = [part for part in requirement.split(",") if part.strip()]
    if not comparators:
        raise ValueError("empty Cargo version requirement")
    if version.prerelease and "-" not in requirement:
        return False
    return all(_matches_one(version, comparator) for comparator in comparators)


def sparse_index_path(crate: str) -> str:
    """Return the crates.io sparse-index path for a crate name."""
    name = crate.lower()
    if len(name) == 1:
        return f"1/{name}"
    if len(name) == 2:
        return f"2/{name}"
    if len(name) == 3:
        return f"3/{name[0]}/{name}"
    return f"{name[:2]}/{name[2:4]}/{name}"


def fetch_index(crate: str, attempts: int = 3) -> tuple[int, bytes]:
    """Fetch one sparse-index entry, retrying only transient failures."""
    url = f"{INDEX_ROOT}/{sparse_index_path(crate)}"
    request = urllib.request.Request(url, headers={"User-Agent": USER_AGENT})
    last_error: Exception | None = None

    for attempt in range(1, attempts + 1):
        try:
            with urllib.request.urlopen(request, timeout=20) as response:
                return response.status, response.read()
        except urllib.error.HTTPError as error:
            if error.code == 404:
                return 404, b""
            if error.code != 429 and not 500 <= error.code <= 599:
                raise RuntimeError(
                    f"crates.io returned HTTP {error.code} for {crate}"
                ) from error
            last_error = error
        except (TimeoutError, urllib.error.URLError) as error:
            last_error = error

        if attempt < attempts:
            time.sleep(attempt)

    raise RuntimeError(
        f"crates.io lookup failed after {attempts} attempts for {crate}: "
        f"{last_error}"
    )


def parse_index(body: bytes) -> list[dict[str, Any]]:
    """Parse newline-delimited sparse-index records."""
    records = []
    for line in body.decode("utf-8").splitlines():
        if line:
            records.append(json.loads(line))
    return records


def available_versions(records: list[dict[str, Any]]) -> list[Version]:
    """Return non-yanked versions in ascending SemVer order."""
    return sorted(
        Version.parse(record["vers"])
        for record in records
        if not record.get("yanked", False)
    )


def append_github_output(destination: str | None, name: str, value: str) -> None:
    if not destination:
        return
    with open(destination, "a", encoding="utf-8") as output:
        output.write(f"{name}={value}\n")


def latest_satisfying(crate: str, requirement: str) -> Version | None:
    """Return the latest non-yanked version satisfying a Cargo requirement."""
    status, body = fetch_index(crate)
    versions = [] if status == 404 else available_versions(parse_index(body))
    matches = [
        version
        for version in versions
        if matches_requirement(version, requirement)
    ]
    observed = ", ".join(str(version) for version in versions[-5:]) or "none"
    print(
        f"crates.io: {crate} requirement {requirement!r}; "
        f"latest observed versions: {observed}; satisfying: "
        f"{matches[-1] if matches else 'none'}"
    )
    return matches[-1] if matches else None


def probe(crate: str, requirement: str, github_output: str | None) -> int:
    match = latest_satisfying(crate, requirement)
    append_github_output(
        github_output,
        "published",
        str(match is not None).lower(),
    )
    return 0


def require(crate: str, requirement: str, wait_seconds: float = 0.0) -> int:
    """Fail unless a non-yanked registry version satisfies the requirement.

    The sparse index lags a publish, so `wait_seconds` polls until that deadline
    before an absent version is a final answer. Exhausting the wait on an index
    that never answered re-raises, so the caller reports an unreadable registry
    rather than a missing crate.
    """
    deadline = time.monotonic() + wait_seconds
    unreadable: RuntimeError | None = None

    while True:
        try:
            if latest_satisfying(crate, requirement) is not None:
                return 0
            unreadable = None
        except RuntimeError as error:
            unreadable = error

        remaining = deadline - time.monotonic()
        if remaining <= 0:
            break
        print(
            f"crates.io: {crate} does not yet satisfy {requirement!r}; "
            f"{remaining:.0f}s of wait left",
            file=sys.stderr,
        )
        time.sleep(min(POLL_SECONDS, remaining))

    if unreadable is not None:
        raise unreadable
    print(
        f"crates.io: {crate} has no version satisfying {requirement!r}",
        file=sys.stderr,
    )
    return EXIT_ANSWERED_NO


def ensure_absent(crate: str, raw_version: str) -> int:
    target = Version.parse(raw_version)
    status, body = fetch_index(crate)
    if status == 404:
        print(f"crates.io: {crate} has no sparse-index entry; {target} is absent")
        return 0
    records = parse_index(body)
    if any(Version.parse(record["vers"]) == target for record in records):
        print(
            f"crates.io: {crate} {target} is already published",
            file=sys.stderr,
        )
        return 1
    print(f"crates.io: {crate} exists; target version {target} is absent")
    return 0


def build_parser() -> argparse.ArgumentParser:
    result = argparse.ArgumentParser(description=__doc__)
    subparsers = result.add_subparsers(dest="command", required=True)

    probe_parser = subparsers.add_parser(
        "probe",
        help="report whether a non-yanked version satisfies a Cargo requirement",
    )
    probe_parser.add_argument("crate")
    probe_parser.add_argument("requirement")
    probe_parser.add_argument(
        "--github-output",
        default=os.environ.get("GITHUB_OUTPUT"),
    )

    absent_parser = subparsers.add_parser(
        "ensure-absent",
        help="fail when an exact crate version is already in the index",
    )
    absent_parser.add_argument("crate")
    absent_parser.add_argument("version")

    require_parser = subparsers.add_parser(
        "require",
        help="fail unless a non-yanked version satisfies a Cargo requirement",
    )
    require_parser.add_argument("crate")
    require_parser.add_argument("requirement")
    require_parser.add_argument(
        "--wait-seconds",
        type=float,
        default=0.0,
        help="poll until this deadline before reporting the version absent",
    )

    match_parser = subparsers.add_parser(
        "matches",
        help="test one Cargo requirement without network access",
    )
    match_parser.add_argument("requirement")
    match_parser.add_argument("versions", nargs="+")

    return result


def main(argv: list[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    try:
        if args.command == "probe":
            return probe(args.crate, args.requirement, args.github_output)
        if args.command == "ensure-absent":
            return ensure_absent(args.crate, args.version)
        if args.command == "require":
            return require(args.crate, args.requirement, args.wait_seconds)
        for raw_version in args.versions:
            version = Version.parse(raw_version)
            print(
                f"{version}={str(matches_requirement(version, args.requirement)).lower()}"
            )
        return 0
    except (json.JSONDecodeError, OSError, RuntimeError, ValueError) as error:
        print(f"crates-io: {error}", file=sys.stderr)
        return EXIT_UNREADABLE


if __name__ == "__main__":
    raise SystemExit(main())
