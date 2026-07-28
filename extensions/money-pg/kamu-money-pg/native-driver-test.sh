#!/usr/bin/env bash
# Prove the join between the native extension and the Rust drivers (review F8).
#
# kamu-money-pg's own suite runs INSIDE the backend through SPI, so it cannot exercise a client
# driver. The driver suites run against `text` columns with no extension installed. Neither
# proves the thing an application actually does: read a native `kmoney` column through
# postgres-types / sqlx. This script builds the pgrx image, starts a PostgreSQL that has
# kmoney installed, and runs kamu-money-core's `pg_native_column` test against it.
#
# The test runs INSIDE the container on purpose. It needs an extension-bearing PostgreSQL, which
# means this image; spawning a sibling container from a test running inside it would mean nesting
# a Docker daemon. So the container lifetime moves here, owned by a trap rather than by whoever
# remembers -- the shell equivalent of the `Drop` guard the testcontainers-based suites use.
set -euo pipefail
cd "$(dirname "$0")/.."   # repo root
# shellcheck source=scripts/docker-core-context.sh
source ./scripts/docker-core-context.sh

PG_MAJOR="${1:-18}"
RUN_ID="$$-$(date +%s)"
LABEL="kamu-money-pg.driver=${RUN_ID}"
NAME="kamu-money-pg-driver-${RUN_ID}"
IIDFILE="$(mktemp)"

cleanup() {
    docker rm -f "$NAME" >/dev/null 2>&1 || true
    # Scoped to THIS run's label -- never a broad prune. The daemon is shared.
    for id in $(docker ps -aq --filter "label=${LABEL}" 2>/dev/null || true); do
        docker rm -f "$id" >/dev/null 2>&1 || true
    done
    [ -n "${IIDFILE:-}" ] && rm -f "$IIDFILE" 2>/dev/null
    return 0
}
trap cleanup EXIT INT TERM HUP

echo "=== building the kamu-money-pg image (PG${PG_MAJOR}) ==="
# --iidfile for the same reason test-matrix.sh uses it: the tag is mutable on a shared daemon,
# so run the image IDENTITY that this build produced, not a name someone else can repoint.
# A TAG OF ITS OWN, NOT `kamu-money-pg:pg${PG_MAJOR}`.
#
# That is test-matrix.sh's tag, and test-matrix.sh labels what it builds with
# `kamu-money-pg.revision`, which `bench-pg` requires before it will time an image. This build
# carries only the driver run's own label, so retagging the shared name silently repointed it at
# an unlabelled image -- and since `release-check` ends with the driver column, a SUCCESSFUL
# release deterministically left `just bench-pg 18` refusing until a full test rebuild.
#
# The refusal was correct; the collision was not. `--iidfile` below is what this script actually
# runs, so the tag is convenience either way -- it just has no business overwriting another
# suite's.
docker build "${KMONEY_CORE_DOCKER_ARGS[@]}" \
    -f kamu-money-pg/Dockerfile --build-arg "PG_MAJOR=${PG_MAJOR}" \
    --label "${LABEL}" --iidfile "${IIDFILE}" -t "kamu-money-pg-driver:pg${PG_MAJOR}" .
IMAGE_ID="$(cat "${IIDFILE}")"
rm -f "${IIDFILE}"; IIDFILE=""
echo "=== running the native-column driver test in ${IMAGE_ID} ==="

# ALWAYS 127.0.0.1, never localhost, and never published to the host: the test runs in the
# container, so the port never needs to leave it.
docker run --rm --name "$NAME" --label "${LABEL}" \
    -e "PG_MAJOR=${PG_MAJOR}" "${IMAGE_ID}" bash -euxo pipefail -c '
    PGCFG="/usr/lib/postgresql/${PG_MAJOR}/bin/pg_config"

    # Install the extension into the PostgreSQL this image already carries.
    cargo pgrx install -p kamu-money-pg --no-default-features \
        --features "pg${PG_MAJOR}" -c "$PGCFG"

    # pgrx manages its own instance; its port convention is 28800 + major.
    cargo pgrx start "pg${PG_MAJOR}"
    PORT=$((28800 + PG_MAJOR))

    export MONEY_PG_NATIVE_URL="postgres://$(id -un)@127.0.0.1:${PORT}/postgres"
    echo "native driver URL: ${MONEY_PG_NATIVE_URL}"

    # The one test that joins the extension to the drivers. --features both, because C9 requires
    # postgres-types and sqlx to agree -- including about which columns they refuse.
    #
    # --test-threads=1 is REQUIRED, not tidiness. All four tests share one database and each runs
    # CREATE EXTENSION IF NOT EXISTS, which is NOT atomic: run in parallel, two sessions pass
    # the existence check and the loser dies on
    #   duplicate key value violates unique constraint "pg_extension_name_index".
    # Their tables are distinct, so the extension is the only shared resource -- and serialising
    # is the honest fix, because these tests genuinely are not independent.
    # Tee and assert the COUNT. Every test returns early and PASSES when MONEY_PG_NATIVE_URL is
    # unset, so "cargo test succeeded" alone would let this whole harness print OK while proving
    # nothing -- a skip is not a pass. The url is exported above, so a skip here means something
    # unset it, and that must be loud rather than green.
    #
    # FOUR since 2026-07-27, when the two write halves were added. The number is hardcoded on
    # purpose: it is what makes a test that stops running loud instead of invisible, which is the
    # entire job of this assertion. Update it deliberately when the suite grows.
    CORE_MANIFEST="$(./scripts/resolve-core-manifest.sh)"
    cargo test --manifest-path "$CORE_MANIFEST" \
        --features postgres,sqlx --test pg_native_column \
        -- --nocapture --test-threads=1 2>&1 | tee /tmp/native-driver.out
    if ! grep -q "4 passed" /tmp/native-driver.out; then
        echo "native-driver-test: FAILED -- expected 4 passing tests; the suite changed size" >&2
        exit 1
    fi
    if grep -q "skipping:" /tmp/native-driver.out; then
        echo "native-driver-test: FAILED -- tests SKIPPED, so nothing was proven" >&2
        exit 1
    fi
'
echo "native-driver-test: OK (PG${PG_MAJOR})"
