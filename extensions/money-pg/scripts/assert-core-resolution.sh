#!/usr/bin/env bash
# Assert that Cargo resolved kamu-money-core the way this image intends.
#
#   bash scripts/assert-core-resolution.sh local|registry
#
# The lane's container builds carry a named Docker context holding Cargo's normalized
# kamu-money-core package and append a `[patch.crates-io]` entry pointing at it. PASSING that
# context is not the same as USING it. A patch offering a version `Cargo.lock` does not pin is
# ignored: Cargo prints `patch ... was not used in the crate graph` and compiles the published
# crate instead, and the build succeeds either way.
#
# That is not hypothetical. A dependency bump ran bare `cargo update` in this lane, which re-locked
# the patched kamu-money-core entry to the registry. Every PostgreSQL and YugabyteDB container
# suite then went on testing the PUBLISHED crate while its caller believed it was testing the tree,
# and nothing said so -- the repository's guard checks that the context is passed, which it was.
#
# So the claim is checked where it is made. Both directions matter and both are checked: ordinary
# suites must resolve the local package, and the release proof must resolve the registry, so an
# assertion covering only one of them would leave the other free to drift.
set -euo pipefail

WANT="${1:-}"
case "$WANT" in
    local | registry) ;;
    *)
        echo "assert-core-resolution: usage: assert-core-resolution.sh local|registry" >&2
        exit 2
        ;;
esac

# `cargo tree --invert` names the source of the RESOLVED package: a path in parentheses when a
# patch applied, and nothing when it came from the registry. `|| true` keeps a resolution failure
# from being swallowed by `set -e` before the diagnostic below can name it; stderr stays on the
# build log.
TREE="$(cargo tree --invert kamu-money-core --depth 0 || true)"
RESOLVED="${TREE%%$'\n'*}"

[ -n "$RESOLVED" ] || {
    echo "assert-core-resolution: kamu-money-core is not in the resolved graph at all" >&2
    exit 1
}

PATCHED=0
case "$RESOLVED" in
    *"(/opt/kamu-money-core)"*) PATCHED=1 ;;
esac

if [ "$WANT" = local ] && [ "$PATCHED" -ne 1 ]; then
    echo "assert-core-resolution: REFUSING -- the normalized kamu-money-core context is present" >&2
    echo "assert-core-resolution: and patched in, but Cargo resolved '$RESOLVED'." >&2
    echo "assert-core-resolution: A patch offering a version Cargo.lock does not pin is IGNORED," >&2
    echo "assert-core-resolution: and this build would then test the published crate. Re-lock the" >&2
    echo "assert-core-resolution: lane with the patch active: just pg core-relock." >&2
    exit 1
fi

if [ "$WANT" = registry ] && [ "$PATCHED" -eq 1 ]; then
    echo "assert-core-resolution: REFUSING -- the release proof must resolve kamu-money-core from" >&2
    echo "assert-core-resolution: crates.io, but Cargo resolved '$RESOLVED'." >&2
    exit 1
fi

echo "assert-core-resolution: kamu-money-core resolved ${WANT} -- ${RESOLVED}"
