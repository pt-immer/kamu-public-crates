#!/usr/bin/env python3
"""Run cargo-deny with the lane's unpublished local dependency patched in."""

from __future__ import annotations

import os
from pathlib import Path
import shutil
import subprocess
import tempfile


LANE_ROOT = Path(__file__).resolve().parents[1]
CORE_ROOT = LANE_ROOT.parents[1] / "crates" / "money-core"


def main() -> None:
    cargo_deny = shutil.which("cargo-deny")
    if cargo_deny is None:
        raise SystemExit("cargo-deny is required; run `just setup`")

    # cargo-deny runs its own `cargo fetch` and cannot forward Cargo's
    # command-line `--config`. A temporary CARGO_HOME supplies only this local
    # patch, leaving the lane manifest and ordinary Cargo commands untouched.
    # Omitting CORE_PATCH therefore remains the release-resolution proof.
    with tempfile.TemporaryDirectory(prefix="kamu-money-pg-deny-") as cargo_home:
        config = Path(cargo_home) / "config.toml"
        config.write_text(
            "[patch.crates-io]\n"
            f"kamu-money-core = {{ path = {str(CORE_ROOT)!r} }}\n",
            encoding="utf-8",
        )
        env = os.environ.copy()
        env["CARGO_HOME"] = cargo_home
        subprocess.run(
            [
                cargo_deny,
                "--manifest-path",
                str(LANE_ROOT / "Cargo.toml"),
                "--config",
                str(LANE_ROOT / "deny.toml"),
                "check",
            ],
            cwd=LANE_ROOT,
            env=env,
            check=True,
        )


if __name__ == "__main__":
    main()
