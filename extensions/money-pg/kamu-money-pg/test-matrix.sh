#!/usr/bin/env bash
# Build and test kamu-money-pg against every supported PostgreSQL major, in containers.
#
#   ./kamu-money-pg/test-matrix.sh            # PG15..latest supported
#   ./kamu-money-pg/test-matrix.sh 17 18      # a subset
#
# Builds capture immutable image IDs because tags are mutable on the shared daemon.
set -euo pipefail

# Tested support spans PostgreSQL 15-18.
DEFAULT_MAJORS=(15 16 17 18)
MAJORS=("${@:-${DEFAULT_MAJORS[@]}}")

cd "$(dirname "$0")/.."
# shellcheck source=scripts/docker-core-context.sh
source ./scripts/docker-core-context.sh

# Include random entropy because PID namespaces can repeat on a shared daemon.
REVISION="$(git rev-parse --short HEAD 2>/dev/null || echo nogit)"

# Keep the image label as the bare revision; record dirty state only in output.
if [ -n "$(git status --porcelain 2>/dev/null)" ]; then
    TREE_STATE=", dirty tree built beyond ${REVISION}"
else
    TREE_STATE=""
fi
ENTROPY="$(od -An -N4 -tx4 /dev/urandom | tr -d ' \n')"
RUN_ID="${REVISION}-$$-${ENTROPY}"
LABEL="kamu-money-pg.run=${RUN_ID}"
IIDFILE=""

# Remove only containers carrying this run's label, including on interruption.
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

# One shared implementation for sequential and parallel execution.
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

# Optional parallelism is bounded by the operator; buffered logs replay in major order.
MATRIX_JOBS="${MATRIX_JOBS:-1}"

# Validate before numeric comparison because a failed conditional test bypasses `set -e`.
# just-anti-example: `just test-pg "15 16" jobs=4` passes a nonnumeric positional value.
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
  # Report every failed major.
  echo "matrix FAILED: ${failed[*]} (revision ${REVISION}${TREE_STATE})"
  exit 1
fi
