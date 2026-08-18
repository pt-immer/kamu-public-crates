#!/usr/bin/env python3
"""Tests for fail-closed crates.io probing and Cargo requirement matching."""

from __future__ import annotations

import io
import unittest
import urllib.error
from unittest.mock import patch

from crates_io import (
    EXIT_ANSWERED_NO,
    EXIT_UNREADABLE,
    Version,
    fetch_index,
    latest_satisfying,
    main,
    matches_requirement,
    parse_index,
    require,
    sparse_index_path,
)


class VersionRequirementTests(unittest.TestCase):
    def assert_matches(
        self,
        requirement: str,
        accepted: tuple[str, ...],
        rejected: tuple[str, ...],
    ) -> None:
        for raw in accepted:
            with self.subTest(requirement=requirement, accepted=raw):
                self.assertTrue(
                    matches_requirement(Version.parse(raw), requirement)
                )
        for raw in rejected:
            with self.subTest(requirement=requirement, rejected=raw):
                self.assertFalse(
                    matches_requirement(Version.parse(raw), requirement)
                )

    def test_cargo_zero_major_caret(self) -> None:
        self.assert_matches(
            "0.1",
            ("0.1.0", "0.1.99"),
            ("0.0.99", "0.2.0", "1.0.0"),
        )

    def test_cargo_major_caret(self) -> None:
        self.assert_matches(
            "2",
            ("2.0.0", "2.99.0"),
            ("1.99.0", "3.0.0"),
        )

    def test_ranges_tilde_wildcards_and_exact(self) -> None:
        self.assert_matches(
            ">=1.2, <2",
            ("1.2.0", "1.9.9"),
            ("1.1.9", "2.0.0"),
        )
        self.assert_matches("~1.2", ("1.2.0", "1.2.9"), ("1.3.0",))
        self.assert_matches("1.2.*", ("1.2.0", "1.2.9"), ("1.3.0",))
        self.assert_matches("=1.2.3", ("1.2.3",), ("1.2.4",))

    def test_prerelease_requires_an_explicit_prerelease_requirement(self) -> None:
        self.assertFalse(
            matches_requirement(Version.parse("1.2.3-rc.1"), "1.2.3")
        )
        self.assertTrue(
            matches_requirement(
                Version.parse("1.2.3-rc.2"),
                ">=1.2.3-rc.1, <1.2.3",
            )
        )

    def test_sparse_index_paths(self) -> None:
        self.assertEqual("1/a", sparse_index_path("a"))
        self.assertEqual("2/ab", sparse_index_path("ab"))
        self.assertEqual("3/a/abc", sparse_index_path("abc"))
        self.assertEqual(
            "ka/mu/kamu-money-core",
            sparse_index_path("kamu-money-core"),
        )

    def test_yanked_records_are_parseable_but_preserved_for_exact_lookup(self) -> None:
        records = parse_index(
            b'{"name":"x","vers":"1.0.0","yanked":false}\n'
            b'{"name":"x","vers":"1.1.0","yanked":true}\n'
        )
        self.assertEqual(["1.0.0", "1.1.0"], [row["vers"] for row in records])

    @patch(
        "crates_io.fetch_index",
        return_value=(
            200,
            b'{"name":"x","vers":"0.1.8","yanked":false}\n'
            b'{"name":"x","vers":"0.2.0","yanked":false}\n',
        ),
    )
    def test_registry_requirement_uses_versions_not_path_presence(
        self,
        _fetch,
    ) -> None:
        self.assertEqual(Version.parse("0.1.8"), latest_satisfying("x", "0.1"))
        self.assertEqual(1, require("x", "0.3"))


class NetworkFailureTests(unittest.TestCase):
    @patch("crates_io.time.sleep", return_value=None)
    @patch("crates_io.urllib.request.urlopen")
    def test_repeated_rate_limit_fails_instead_of_becoming_absence(
        self,
        urlopen,
        _sleep,
    ) -> None:
        urlopen.side_effect = urllib.error.HTTPError(
            "https://index.crates.io/x",
            429,
            "rate limited",
            {},
            io.BytesIO(),
        )
        with self.assertRaisesRegex(RuntimeError, "failed after 3 attempts"):
            fetch_index("example-crate")

    @patch("crates_io.urllib.request.urlopen")
    def test_verified_404_means_absence(self, urlopen) -> None:
        urlopen.side_effect = urllib.error.HTTPError(
            "https://index.crates.io/x",
            404,
            "not found",
            {},
            io.BytesIO(),
        )
        self.assertEqual((404, b""), fetch_index("example-crate"))


class UnreadableIsNotAbsentTests(unittest.TestCase):
    """An index that never answered must not be reported as one that said no."""

    @patch("crates_io.fetch_index", return_value=(404, b""))
    def test_absent_version_exits_answered_no(self, _fetch) -> None:
        self.assertEqual(EXIT_ANSWERED_NO, main(["require", "x", "=1.0.0"]))

    @patch(
        "crates_io.fetch_index",
        side_effect=RuntimeError("crates.io lookup failed after 3 attempts for x"),
    )
    def test_unreachable_index_exits_unreadable(self, _fetch) -> None:
        self.assertEqual(EXIT_UNREADABLE, main(["require", "x", "=1.0.0"]))

    @patch("crates_io.time.sleep", return_value=None)
    @patch("crates_io.time.monotonic")
    @patch("crates_io.fetch_index")
    def test_waiting_out_an_unreachable_index_still_exits_unreadable(
        self,
        fetch,
        monotonic,
        _sleep,
    ) -> None:
        monotonic.side_effect = [0.0, 10.0, 100.0]
        fetch.side_effect = RuntimeError("crates.io lookup failed")
        self.assertEqual(
            EXIT_UNREADABLE,
            main(["require", "x", "=1.0.0", "--wait-seconds", "60"]),
        )


class IndexLagTests(unittest.TestCase):
    @patch("crates_io.time.sleep", return_value=None)
    @patch("crates_io.time.monotonic")
    @patch("crates_io.fetch_index")
    def test_wait_succeeds_once_the_version_reaches_the_index(
        self,
        fetch,
        monotonic,
        _sleep,
    ) -> None:
        monotonic.side_effect = [0.0, 10.0, 20.0]
        fetch.side_effect = [
            (404, b""),
            (404, b""),
            (200, b'{"name":"x","vers":"1.0.0","yanked":false}\n'),
        ]
        self.assertEqual(0, require("x", "=1.0.0", wait_seconds=60))

    @patch("crates_io.time.sleep", return_value=None)
    @patch("crates_io.time.monotonic")
    @patch("crates_io.fetch_index", return_value=(404, b""))
    def test_wait_expires_into_answered_no(
        self,
        _fetch,
        monotonic,
        _sleep,
    ) -> None:
        monotonic.side_effect = [0.0, 10.0, 100.0]
        self.assertEqual(
            EXIT_ANSWERED_NO,
            require("x", "=1.0.0", wait_seconds=60),
        )


if __name__ == "__main__":
    unittest.main()
