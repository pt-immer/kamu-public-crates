#!/usr/bin/env bash
# Reproduce, from scratch, why `kamu-money-pg` cannot run inside YugabyteDB -- and FAIL if it no
# longer can.
#
#   just yb-probe            (or: ./kamu-money-pg/yb-compat-probe.sh [yb-image] [builder-image])
#
# Two independent blockers, each asserted. See kamu-money-core's DESIGN.md E15. Exit status is the result:
#
#   0  both blockers reproduced; E15 still describes reality
#   1  a blocker is gone; E15 must be re-examined, not re-asserted
#   2  the probe could not run (missing builder image)
#
# WHAT "REPRODUCE" MEANS HERE, because an earlier version of this script did not earn the word.
# It grepped for a missing #include and printed a previously-captured loader error as literal
# text, then called both of them reproductions. Grepping a header is evidence that a header is
# missing; it is not evidence that compilation fails. Echoing an error message is not evidence
# of anything at all. Both blockers below now actually run:
#
#   Blocker one: YugabyteDB's own server headers are compiled, by a real compiler, and the
#                compiler's real error is shown.
#   Blocker two: a shared object is really built against a modern glibc, really copied into the
#                running YugabyteDB container, and really LOADed, and the loader's real refusal
#                is shown.
#
# The YugabyteDB image ships NO compiler (measured: no cc, gcc or clang on PATH), so the
# compile must happen in a builder image against headers extracted from the YugabyteDB one.
# That is itself part of the finding: this image cannot build an extension in place either.
set -euo pipefail

# E15 is a DATED measurement about one third-party image, which is exactly the case a mutable
# tag cannot carry (review-3 N9): if the tag is repointed, this stops being evidence about the
# thing E15 measured. The version differs from E16's on purpose -- E15 measured 2025.2.4.1-b4 --
# but it is resolved to a digest all the same.
YB_IMAGE="${1:-$(./kamu-money-pg/yb/yb-image.sh yugabytedb/yugabyte:2025.2.4.1-b4)}"
# Reused because it already carries build-essential and a modern glibc. Any Debian-with-gcc
# image works; this one avoids a pull on a machine that has run the matrix.
BUILDER_IMAGE="${2:-kamu-money-pg:pg18}"

RUN_ID="yb-probe-$$-$(od -An -N4 -tx4 /dev/urandom | tr -d ' \n')"
NAME="${RUN_ID}"
WORK="$(mktemp -d)"

# --- assertion machinery -----------------------------------------------------------------
#
# WHY THIS EXISTS. The previous version really did run both blockers -- and then reported
# success no matter what they said. It wrapped the compiler in `|| true`, read the loader probe
# through `head` under `set +e`, compared nothing to anything, and closed by printing "Both
# blockers were executed" before exiting 0. That is a transcript generator, not a probe: the day
# YugabyteDB ships the missing header or a compatible loader, it would still print the old
# conclusion and still pass.
#
# The direction of the assertion is the unusual part and the reason it is spelled out here.
# Every check below demands that something FAIL. A blocker that stops reproducing is not good
# news to be swallowed -- it means kamu-money-core's DESIGN.md E15 is describing a world that no longer exists, and
# the adapter-only decision it justifies has to be revisited. So "the compile succeeded" exits
# non-zero, on purpose.
BLOCKERS_CONFIRMED=0
BLOCKERS_LOST=""

# assert_blocker <label> <status> <output> <signature>
#   Requires a non-zero status AND the expected error text. Either alone is too weak: a
#   container that dies for an unrelated reason gives a non-zero status, and a signature can
#   appear in a warning on an otherwise successful build.
assert_blocker() {
  label="$1"; status="$2"; output="$3"; signature="$4"
  if [ "$status" -ne 0 ] && printf '%s' "$output" | grep -q -- "$signature"; then
    echo "CONFIRMED: ${label}"
    echo "           (exit ${status}, matched '${signature}')"
    BLOCKERS_CONFIRMED=$((BLOCKERS_CONFIRMED + 1))
    return 0
  fi
  echo "NOT REPRODUCED: ${label}"
  echo "           expected a non-zero exit AND '${signature}'; got exit ${status}"
  BLOCKERS_LOST="${BLOCKERS_LOST}
  - ${label}"
  return 0
}

# EXIT alone is not enough: it does not fire if the shell is killed while blocked in the
# readiness loop below, which is where this script spends nearly all of its time. A container
# that outlives its script is exactly the dangling this repo has already paid for once.
cleanup() {
  docker rm -f "$NAME" >/dev/null 2>&1 || true
  rm -rf "$WORK" 2>/dev/null || true
  return 0
}
trap cleanup EXIT INT TERM HUP

if ! docker image inspect "$BUILDER_IMAGE" >/dev/null 2>&1; then
  echo "builder image '$BUILDER_IMAGE' is not present."
  echo "Run 'just test-pg 18' first, or pass a Debian-with-gcc image as the second argument."
  exit 2
fi

echo "=== 1. What PostgreSQL is YugabyteDB, actually? ==============================="
docker run -d --name "$NAME" --label "kamu-money-pg.probe=${RUN_ID}" \
  "$YB_IMAGE" bin/yugabyted start --background=false >/dev/null
# yugabyted binds YSQL to the node's advertised address, never to loopback, so the host has to
# be discovered. Connecting to 127.0.0.1 fails with ECONNREFUSED and looks like a slow start.
HOST=""
for _ in $(seq 1 120); do
  HOST="$(docker exec "$NAME" hostname -i 2>/dev/null | awk '{print $1}')" || true
  [ -n "${HOST:-}" ] && docker exec "$NAME" bin/ysqlsh -h "$HOST" -U yugabyte -c 'SELECT 1' \
      >/dev/null 2>&1 && break
  sleep 2
done
docker exec "$NAME" bin/ysqlsh -h "$HOST" -U yugabyte -X -t -c 'SELECT version();'

echo
echo "=== 2. BLOCKER ONE: the shipped headers really do not compile ================="
echo "YugabyteDB ships pg_config, server headers and the extension directories, so building"
echo "against it looks possible. Below, a real compiler is pointed at those real headers."
echo
echo "--- the include that starts it ---"
docker exec "$NAME" sh -c \
  'grep -n "ybc_util" /home/yugabyte/postgres/include/server/utils/elog.h | head -3'
echo "--- is that header anywhere in the image? ---"
FOUND="$(docker exec "$NAME" sh -c 'find / -name ybc_util.h 2>/dev/null | head -3')"
echo "${FOUND:-NOT FOUND ANYWHERE IN THE IMAGE - there is nothing to include}"

echo "--- and the image ships no compiler to try it with ---"
docker exec "$NAME" sh -c 'for c in cc gcc clang; do command -v $c || echo "$c: absent"; done'

echo
echo "--- extracting YugabyteDB's server headers and compiling against them for real ---"
docker cp "$NAME:/home/yugabyte/postgres/include/server" "$WORK/server" >/dev/null
printf '#include "postgres.h"\nint probe(void) { return 0; }\n' > "$WORK/probe.c"

# `cc` is the LAST command in the container, so its status becomes the container's status and
# `docker run`'s. Nothing is piped: a pipeline here would report the last stage's status, which
# is how the first version of this script printed "exit status: 0" directly underneath a fatal
# compiler error. Redirect stderr into the captured output, take `$?` on the very next line.
set +e
CC_OUTPUT="$(docker run --rm --label "kamu-money-pg.probe=${RUN_ID}" \
  -v "$WORK:/probe:ro" --entrypoint sh "$BUILDER_IMAGE" -c \
  'cc -c -I/probe/server -o /tmp/probe.o /probe/probe.c 2>&1')"
CC_STATUS=$?
set -e

printf '%s\n' "$CC_OUTPUT" | head -20
echo
assert_blocker "blocker one: YugabyteDB's own server headers do not compile" \
  "$CC_STATUS" "$CC_OUTPUT" "ybc_util.h"

echo
echo "=== 3. BLOCKER TWO: a foreign-built .so really will not load ==================="
docker exec "$NAME" sh -c 'ldd --version 2>/dev/null | head -1'
echo "--- building a .so against the builder's modern glibc ---"
# `gettid()` was added to glibc in 2.30. Referencing it is the smallest honest way to produce
# a library that REQUIRES a newer glibc than this image has, which is exactly the condition a
# real kmoney.so hits. The point is the loader's verdict, not this function's body.
cat > "$WORK/needs_new_glibc.c" <<'EOF'
#define _GNU_SOURCE
#include <unistd.h>
long probe_tid(void) { return (long) gettid(); }
EOF
docker run --rm --label "kamu-money-pg.probe=${RUN_ID}" \
  -v "$WORK:/probe" --entrypoint sh "$BUILDER_IMAGE" -c \
  'cc -shared -fPIC -o /probe/needs_new_glibc.so /probe/needs_new_glibc.c && \
   objdump -T /probe/needs_new_glibc.so | grep -o "GLIBC_2\.[0-9]*" | sort -u | tail -3'

echo "--- copying it into the running YugabyteDB container and making PG dlopen it ---"
docker cp "$WORK/needs_new_glibc.so" "$NAME:/tmp/needs_new_glibc.so" >/dev/null
# NOT `LOAD`: YugabyteDB answers that with "ERROR: LOAD not supported yet", which is a
# different finding and would leave this blocker untested while looking like a confirmation.
# `CREATE FUNCTION ... LANGUAGE C` is the path an extension actually takes, and it dlopens the
# library at creation -- before any symbol lookup or magic-block check, so a glibc mismatch
# surfaces first, which is precisely the ordering this blocker depends on.
#
# `-v ON_ERROR_STOP=1` is what makes the status meaningful: without it psql reports success
# after a failed statement, and the assertion below would rest on the message alone.
# Streams merged INSIDE the container. The host cannot order `docker exec`'s two multiplexed
# channels, and this capture is the evidence a blocker assertion is made from -- the loader's
# refusal must be readable in the order it was produced, not in the order it happened to arrive.
# The SQL goes in as a positional argument so its embedded single quotes never meet the container
# shell's quoting.
set +e
LOAD_OUTPUT="$(docker exec "$NAME" bash -c \
  'exec bin/ysqlsh -h "$1" -U yugabyte -X -v ON_ERROR_STOP=1 -c "$2" 2>&1' \
  ysqlsh "$HOST" \
  "CREATE FUNCTION probe_tid() RETURNS bigint AS '/tmp/needs_new_glibc.so', 'probe_tid' LANGUAGE C;")"
LOAD_STATUS=$?
set -e

printf '%s\n' "$LOAD_OUTPUT" | head -6
echo
# The signature is the loader's verdict, not the exact glibc version: which symbol version is
# missing depends on the builder image, and pinning "GLIBC_2.30" would turn a newer builder into
# a false "not reproduced". What must hold is that the dynamic loader refused a symbol version.
assert_blocker "blocker two: a foreign-built .so is refused by YugabyteDB's loader" \
  "$LOAD_STATUS" "$LOAD_OUTPUT" "GLIBC_"

echo
echo "--- for the record, what YugabyteDB says to LOAD itself (diagnostic, not asserted) ---"
# Deliberately NOT a blocker assertion. `LOAD not supported yet` is a different finding, and an
# earlier version of this probe mistook it for a confirmation of the glibc one -- which left
# blocker two untested while looking green.
# `head -3` makes the ordering matter even for a diagnostic: with the streams merged on the host,
# the three lines kept could be the three that arrived first rather than the three that were
# printed first, and the interesting one is not guaranteed to be among them.
set +e
docker exec "$NAME" bash -c \
  'exec bin/ysqlsh -h "$1" -U yugabyte -X -c "$2" 2>&1' \
  ysqlsh "$HOST" "LOAD '/tmp/needs_new_glibc.so';" | head -3
set -e

echo
echo "=== CONCLUSION ================================================================"
echo "Scope of what was just tested: the image named above, built the conventional way."
echo

if [ -n "$BLOCKERS_LOST" ]; then
  echo "PROBE FAILED: ${BLOCKERS_CONFIRMED} of 2 blockers reproduced."
  echo "These no longer reproduce:${BLOCKERS_LOST}"
  echo
  echo "This is not a broken script -- it is the finding changing underneath kamu-money-core's DESIGN.md E15."
  echo "E15 documents why kamu-money-pg cannot run on YugabyteDB and justifies serving it through"
  echo "the phase-4 text adapters instead. If a blocker is gone, that reasoning needs to be"
  echo "re-examined rather than re-asserted. Read the transcript above before editing E15."
  exit 1
fi

echo "Both blockers CONFIRMED: reproduced and asserted, not merely executed."
echo "kamu-money-pg is served on YugabyteDB by the phase-4 adapters instead, where money is stored"
echo "in a type YugabyteDB already has and every arithmetic operation stays in Rust."
