#!/usr/bin/env python3
"""Regression tests for the total CI path classifier."""

from __future__ import annotations

import unittest

from ci_paths import classify_path, classify_paths


class PathClassifierTests(unittest.TestCase):
    def test_each_crate_family_has_an_owner(self) -> None:
        cases = {
            "crates/iso3166/src/lib.rs": {"iso3166"},
            "crates/logging/src/lib.rs": {"logging"},
            "crates/money-core/src/lib.rs": {"money", "moneypg"},
            "crates/snap-response/src/lib.rs": {"snap"},
            "extensions/money-pg/Cargo.toml": {"moneypg"},
        }
        for path, expected in cases.items():
            with self.subTest(path=path):
                self.assertTrue(expected <= classify_path(path))

    def test_money_core_package_inputs_retest_the_extension(self) -> None:
        for path in (
            "crates/money-core/Cargo.toml",
            "crates/money-core/build.rs",
            "crates/money-core/build/iso4217.rs",
            "crates/money-core/src/arith.rs",
            "crates/money-core/vendor/list-one.xml",
        ):
            with self.subTest(path=path):
                values = classify_paths([path])
                self.assertTrue(values["money"])
                self.assertTrue(values["moneypg"])

        self.assertNotIn(
            "moneypg",
            classify_path("crates/money-core/README.md"),
            "documentation does not change the extension's compiled dependency",
        )

    def test_root_policy_files_are_shared(self) -> None:
        for path in (
            ".gitignore",
            ".gitmodules",
            ".github/CODEOWNERS",
            "LICENSE-APACHE",
            "Cargo.toml",
            "rust-toolchain.toml",
        ):
            with self.subTest(path=path):
                self.assertIn("shared", classify_path(path))

    def test_docs_symlinks_and_submodule_paths_are_owned(self) -> None:
        self.assertIn("docs", classify_path("CLAUDE.md"))
        self.assertIn(
            "docs",
            classify_path(".github/copilot-instructions.md"),
        )
        self.assertIn(
            "iso3166",
            classify_path("crates/iso3166/vendor/iso3166-csv"),
        )

    def test_shell_ownership_follows_extension_not_directory(self) -> None:
        self.assertEqual({"shell"}, classify_path("ops/new-check.sh"))
        values = classify_paths(["ops/new-check.sh"])
        self.assertTrue(values["shell"])
        self.assertTrue(values["lint"])

    def test_crate_markdown_runs_crate_and_docs_checks(self) -> None:
        owned = classify_path("crates/logging/README.md")
        self.assertEqual({"logging", "docs"}, owned)

    def test_unknown_directory_fails_closed(self) -> None:
        with self.assertRaisesRegex(ValueError, "scaffolds/new/config.yml"):
            classify_paths(["scaffolds/new/config.yml"])

    def test_shared_change_fans_out(self) -> None:
        values = classify_paths(["Cargo.lock"])
        for output in (
            "rust",
            "iso",
            "log",
            "money",
            "snap",
            "moneypg",
            "lint",
            "worker",
        ):
            with self.subTest(output=output):
                self.assertTrue(values[output])

    def test_root_docs_only_stay_docs_only(self) -> None:
        values = classify_paths(["README.md"])
        self.assertTrue(values["docs"])
        self.assertTrue(values["lint"])
        self.assertFalse(values["rust"])
        self.assertFalse(values["worker"])


if __name__ == "__main__":
    unittest.main()
