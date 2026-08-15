#!/usr/bin/env python3
"""Regression tests for the total CI path classifier."""

from __future__ import annotations

import unittest

from ci_paths import BASE_CLASSES, DERIVED_CLASSES, classify_path, classify_paths

# One path selecting exactly one base class, for every base class. Purity is
# asserted rather than assumed, because an impure entry would let a derived class
# appear load-bearing on a source it does not actually read.
REPRESENTATIVE_PATHS = {
    "iso3166": "crates/iso3166/src/lib.rs",
    "logging": "crates/logging/src/lib.rs",
    "money": "crates/money-core/tests/facade.rs",
    "moneypg": "extensions/money-pg/Cargo.toml",
    "snap": "crates/snap-crypto/src/lib.rs",
    "shared": "Cargo.lock",
    "docs": "README.md",
    "shell": "ops/new-check.sh",
}

# The fan-out this repository intends, written independently of the map that
# implements it. Deriving the expectation from DERIVED_CLASSES would move both
# sides of the assertion together and prove nothing.
EXPECTED_FAN_OUT = {
    "rust": {"iso3166", "logging", "money", "snap", "shared"},
    "iso": {"iso3166", "shared"},
    "log": {"logging", "shared"},
    "money": {"money", "shared"},
    "snap": {"snap", "shared"},
    "moneypg": {"moneypg", "shared"},
    "worker": {"logging", "shared"},
    "lint": set(BASE_CLASSES),
    "shell": {"shell"},
}


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
            "crates/money-core/src/arithmetic/kernel/add_sub.rs",
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
        for name, (sources, _reason) in DERIVED_CLASSES.items():
            with self.subTest(output=name):
                self.assertEqual("shared" in sources, values[name])

    def test_root_docs_only_stay_docs_only(self) -> None:
        values = classify_paths(["README.md"])
        self.assertTrue(values["docs"])
        self.assertTrue(values["lint"])
        self.assertFalse(values["rust"])
        self.assertFalse(values["worker"])


class DerivedClassTests(unittest.TestCase):
    """The fan-out map is an executable specification, not a description."""

    def test_every_edge_names_a_base_class_and_a_reason(self) -> None:
        for name, (sources, reason) in DERIVED_CLASSES.items():
            with self.subTest(derived=name):
                self.assertTrue(sources, "a class with no source can never fire")
                self.assertTrue(
                    reason.strip(),
                    "state why working on these runs this class's jobs",
                )
                for source in sources:
                    self.assertIn(source, BASE_CLASSES)

    def test_every_base_class_has_a_representative_path(self) -> None:
        self.assertEqual(BASE_CLASSES, set(REPRESENTATIVE_PATHS))

    def test_each_representative_selects_exactly_its_own_base_class(self) -> None:
        for name, path in REPRESENTATIVE_PATHS.items():
            with self.subTest(base=name):
                self.assertEqual({name}, classify_path(path))

    def test_the_map_declares_exactly_the_intended_classes(self) -> None:
        self.assertEqual(set(EXPECTED_FAN_OUT), set(DERIVED_CLASSES))

    def test_a_derived_class_fires_on_its_sources_and_on_nothing_else(self) -> None:
        """Both directions: a missing source is a lie, and so is a spare one."""
        for name, expected in EXPECTED_FAN_OUT.items():
            for base, path in REPRESENTATIVE_PATHS.items():
                with self.subTest(derived=name, base=base):
                    self.assertEqual(
                        base in expected,
                        classify_paths([path])[name],
                    )


if __name__ == "__main__":
    unittest.main()
