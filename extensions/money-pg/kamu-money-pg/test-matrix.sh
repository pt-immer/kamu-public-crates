#!/usr/bin/env bash
# Build and test kamu-money-pg against every supported PostgreSQL major, in containers.
#
# Nothing touches the host. The expensive `cargo install cargo-pgrx` layer is shared across
# every image (see the layer-order note in the Dockerfile), so only the apt step and
# `pgrx init` are repeated per version.
#
#   ./kamu-money-pg/test-matrix.sh            # PG15..latest supported
#   ./kamu-money-pg/test-matrix.sh 17 18      # a subset
#
# CONCURRENCY. This daemon is shared across several organisations' runners. An earlier version
# built `kamu-money-pg:pg18` and then ran `kamu-money-pg:pg18` -- which are two different images if
# anyone retags in between, so a run could report green for a revision it never executed.
# Tags are mutable and daemon-global; image IDs are neither. The build therefore captures an
# image ID and the run uses that ID. The tag is still written, for humans reading
# `docker images`, and is never the thing tested.
set -euo pipefail

# PG15 is the floor (operator decision, 2026-07-22). pgrx 0.19.1 supports up to pg19.
DEFAULT_MAJORS=(15 16 17 18)
MAJORS=("${@:-${DEFAULT_MAJORS[@]}}")

cd "$(dirname "$0")/.."
# shellcheck source=scripts/docker-core-context.sh
source ./scripts/docker-core-context.sh

# A daemon-global run identity. `$$` alone is NOT unique here: separate runner containers
# sharing one daemon have their own PID namespaces, so two concurrent matrices can draw the
# same number and then collide on container names and on each other's cleanup.
REVISION="$(git rev-parse --short HEAD 2>/dev/null || echo nogit)"

# WHAT WAS BUILT, not what is committed. This script builds from the WORKING TREE, so on a dirty
# tree "revision X" names something that was never committed -- a green matrix attributed to a
# commit that was not tested. Measured 2026-07-27: a run mid-refactor printed
# `matrix green: 18 (revision 185b645)` while wire.rs and typmod.rs were uncommitted.
#
# Every other runner here already pairs its sha with this -- the bench scripts print `tree dirty`
# beside `source sha`. This one printed the sha alone.
#
# The DOCKER LABEL deliberately keeps the bare revision: `run-bench-pg.sh` compares it for exact
# equality against `git rev-parse --short HEAD` and prints its own `tree dirty` line, so a suffix
# there would make it refuse every image while telling nobody anything new.
if [ -n "$(git status --porcelain 2>/dev/null)" ]; then
    TREE_STATE=", tree DIRTY -- built from uncommitted changes, NOT from ${REVISION}"
else
    TREE_STATE=""
fi
ENTROPY="$(od -An -N4 -tx4 /dev/urandom | tr -d ' \n')"
RUN_ID="${REVISION}-$$-${ENTROPY}"
LABEL="kamu-money-pg.run=${RUN_ID}"
IIDFILE=""

# `docker run --rm` cleans up on the container's own exit, but not if THIS script is killed
# while one is running -- the --rm is the daemon's promise about the container, not about ours.
# Cleanup is scoped to THIS run's label rather than to a name prefix or an image, so a
# concurrent matrix belonging to another org is unreachable from here.
cleanup() {
  local ids
  ids=$(docker ps -aq --filter "label=${LABEL}" 2>/dev/null || true)
  if [ -n "$ids" ]; then
    # shellcheck disable=SC2086 # word splitting is the point; ids is a list
    docker rm -f $ids >/dev/null 2>&1 || true
  fi
  [ -n "$IIDFILE" ] && rm -f "$IIDFILE" 2>/dev/null
  return 0
}
trap cleanup EXIT INT TERM HUP

failed=()

# One major, start to finish. Extracted so the sequential and parallel paths below run exactly the
# same steps -- a parallel mode that quietly does something else is worse than no parallel mode.
one_major() {
  local pg="$1"
  echo "=============================== PG${pg} ==============================="

  local iidfile image_id
  iidfile="$(mktemp)"
  # --iidfile records the image ID this build actually produced. The -t tag is a convenience
  # for a human; it is deliberately not what gets run below.
  if ! docker build "${KMONEY_CORE_DOCKER_ARGS[@]}" \
        -f kamu-money-pg/Dockerfile --build-arg "PG_MAJOR=${pg}" \
        --label "${LABEL}" --label "kamu-money-pg.revision=${REVISION}" \
        --iidfile "${iidfile}" -t "kamu-money-pg:pg${pg}" . ; then
    echo "PG${pg}: BUILD FAILED"; rm -f "${iidfile}"; return 1
  fi
  image_id="$(cat "${iidfile}")"
  rm -f "${iidfile}"
  echo "PG${pg}: testing image ${image_id}"

  # Run the ID, not the tag. Between the build above and this line, another runner on this
  # shared daemon may have moved `kamu-money-pg:pg${pg}` to its own revision.
  if ! docker run --rm --label "${LABEL}" \
        --name "kamu-money-pg-${RUN_ID}-pg${pg}" "${image_id}"; then
    echo "PG${pg}: TESTS FAILED"; return 1
  fi
  echo "PG${pg}: OK"
}

# MATRIX_JOBS>1 overlaps the majors. They are genuinely independent -- separate images, separate
# containers, names and labels already scoped per run -- and the sequential cost is four full
# from-source extension builds one after another.
#
# NOT the default, and bounded, because this daemon is shared across several organisations'
# runners: taking every core is a decision an operator makes, not one a test script makes for them.
#
# OUTPUT IS REPLAYED IN MAJOR ORDER, never interleaved. This log is release evidence; four builds
# writing over each other would be unreadable, and unreadable evidence gets skimmed rather than
# read. The sequential path keeps streaming live so the ordinary case still shows progress.
MATRIX_JOBS="${MATRIX_JOBS:-1}"

# VALIDATED, because a failed `[` inside an `if` condition is EXEMPT from `set -e`. A non-numeric
# just-anti-example: the next line quotes the BROKEN call to say what it produces.
# value here (`MATRIX_JOBS=jobs=4`, which is what `just test-pg "15 16" jobs=4` produces, since just
# takes arguments positionally) would make the comparison below error "integer expected", return
# non-zero, and silently select the PARALLEL branch with its throttle disabled. Measured on the
# 2026-07-25 release-check run, where exactly that happened one level up.
case "$MATRIX_JOBS" in
  ''|*[!0-9]*)
    echo "test-matrix: MATRIX_JOBS must be a positive integer, got '$MATRIX_JOBS'" >&2
    echo "test-matrix: via just, arguments are POSITIONAL -- 'just test-pg \"15 16\" 4'." >&2
    exit 2 ;;
esac
[ "$MATRIX_JOBS" -ge 1 ] || { echo "test-matrix: MATRIX_JOBS must be >= 1" >&2; exit 2; }

if [ "$MATRIX_JOBS" -le 1 ]; then
  for pg in "${MAJORS[@]}"; do
    one_major "$pg" || failed+=("pg${pg}")
  done
else
  echo "matrix: running ${#MAJORS[@]} majors with up to ${MATRIX_JOBS} concurrent (MATRIX_JOBS)"
  LOGDIR="$(mktemp -d)"
  declare -A PID_OF=()
  for pg in "${MAJORS[@]}"; do
    while [ "$(jobs -rp | wc -l)" -ge "$MATRIX_JOBS" ]; do wait -n || true; done
    one_major "$pg" > "${LOGDIR}/pg${pg}.log" 2>&1 &
    PID_OF[$pg]=$!
  done
  for pg in "${MAJORS[@]}"; do
    wait "${PID_OF[$pg]}" || failed+=("pg${pg}")
    cat "${LOGDIR}/pg${pg}.log"
  done
  rm -rf "$LOGDIR"
fi

echo
if [ ${#failed[@]} -eq 0 ]; then
  echo "matrix green: ${MAJORS[*]} (revision ${REVISION}${TREE_STATE})"
else
  # Report every failure rather than dying on the first: a version-specific break is exactly
  # what a matrix exists to find, and stopping early hides the rest of the range.
  echo "matrix FAILED: ${failed[*]} (revision ${REVISION}${TREE_STATE})"
  exit 1
fi
