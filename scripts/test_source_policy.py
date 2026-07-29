"""Repository-wide source policy tests."""

import re
import subprocess
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
UNSTABLE_HASHER = re.compile(r"\bDefaultHasher\s*::\s*new\s*\(")


def tracked_rust_files(root: Path) -> list[Path]:
    result = subprocess.run(
        ["git", "ls-files", "-z", "--", "*.rs"],
        cwd=root,
        check=True,
        capture_output=True,
    )
    return [
        root / Path(raw.decode("utf-8"))
        for raw in result.stdout.split(b"\0")
        if raw
    ]


def unstable_hasher_lines(source: str) -> list[int]:
    return [
        line
        for line, text in enumerate(source.splitlines(), start=1)
        if UNSTABLE_HASHER.search(text)
    ]


def unstable_hasher_offenders(
    root: Path,
    files: list[Path],
) -> dict[str, list[int]]:
    return {
        str(path.relative_to(root)): lines
        for path in files
        if (lines := unstable_hasher_lines(path.read_text(encoding="utf-8")))
    }


def assert_no_unstable_hasher(root: Path) -> None:
    files = tracked_rust_files(root)
    if not files:
        raise AssertionError("tracked Rust source discovery failed")
    offenders = unstable_hasher_offenders(root, files)
    if offenders:
        raise AssertionError(
            "DefaultHasher output is not stable across Rust releases; "
            "use kamu_money_core::advanced::stable_hash for persisted values: "
            f"{offenders}"
        )


class SourcePolicyTests(unittest.TestCase):
    def test_planted_nested_violation_fails_the_guard(self) -> None:
        self.assertEqual(
            unstable_hasher_lines(
                "let h = std::collections::hash_map::DefaultHasher :: new ();"
            ),
            [1],
            "positive control must recognize a spaced constructor",
        )

        with tempfile.TemporaryDirectory() as raw_root:
            root = Path(raw_root)
            subprocess.run(
                ["git", "init", "--quiet"],
                cwd=root,
                check=True,
            )
            source = root / "extensions/example/src/nested/guard.rs"
            source.parent.mkdir(parents=True)
            source.write_text(
                "fn bad() { let _ = "
                "std::collections::hash_map::DefaultHasher::new(); }\n",
                encoding="utf-8",
            )
            subprocess.run(
                ["git", "add", str(source.relative_to(root))],
                cwd=root,
                check=True,
            )
            with self.assertRaisesRegex(
                AssertionError,
                "extensions/example/src/nested/guard.rs",
            ):
                assert_no_unstable_hasher(root)

    def test_default_hasher_cannot_back_persisted_values(self) -> None:
        self.assertGreater(
            len(tracked_rust_files(ROOT)),
            50,
            "tracked Rust source discovery found too few files",
        )
        assert_no_unstable_hasher(ROOT)


if __name__ == "__main__":
    unittest.main()
