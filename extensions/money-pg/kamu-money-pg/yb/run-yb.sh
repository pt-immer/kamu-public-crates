#!/usr/bin/env bash
# Run kamu-money-pg's ABI battery on a fresh YugabyteDB, capturing deterministic output
# for the byte-exact A/B against stock PG15.
#
#   kamu-money-pg/yb/run-yb.sh [yb-image] [artifact-dir] [out-file]
#
# Prereq: kamu-money-pg/yb/out/{kmoney.so,kmoney.control,kmoney--*.sql} built by
#   docker build -f kamu-money-pg/yb/Dockerfile --target artifact -o kamu-money-pg/yb/out .
set -euo pipefail
cd "$(dirname "$0")/../.."   # repo root

# This script writes under ${KMONEY_RUN_ROOT:-kamu-money-pg/yb/out}. With KMONEY_RUN_ROOT unset --
# the default -- that is one tree shared with every other suite, so this is one writer among
# several that must not overlap: a release check reads the very files a hand-started run of this
# script would overwrite. Setting KMONEY_RUN_ROOT gives a run its own tree and the contention
# stops existing rather than being serialised. The lock is re-entrant, so being invoked BY
# release-check is free.
# shellcheck source=kamu-money-pg/yb/workspace-lock.sh
source "$(dirname "$0")/workspace-lock.sh"
workspace_lock "$(basename "$0")" || exit 1

# Default resolves the mutable tag to an immutable digest rather than passing the tag through
# (review-3 N9). Every Justfile path already passes a resolved "$YB_REF"; this closes the
# hand-invocation route, which was the only one left that could straddle a retag.
YB_IMAGE="${1:-$(./kamu-money-pg/yb/yb-image.sh)}"
ART="${2:-kamu-money-pg/yb/out}"
# Under out/, matching what the Justfile passes and what .gitignore covers (review-3 N10). The
# old default wrote to kamu-money-pg/yb/out-yb.txt -- one directory up, NOT ignored, so a hand run
# left an untracked battery output sitting in the tree.
OUT="${3:-kamu-money-pg/yb/out/out-yb.txt}"
SQLFILE="${4:-kamu-money-pg/yb/abi_battery.sql}"

# Baked or copied, verified by hash either way -- one implementation, shared with the cluster
# harnesses. install.sh sources artifact.sh, which resolves the triplet by exact name against the
# build's manifest (three independent recursive `find | head -1`s could pair a new control file
# with an old library, and a mismatched triplet installs cleanly).
# shellcheck source=kamu-money-pg/yb/install.sh
source ./kamu-money-pg/yb/install.sh

# Node resource caps, the same ones cluster.sh uses. See node-limits.sh for why a container cannot
# be trusted to size itself.
# shellcheck source=kamu-money-pg/yb/node-limits.sh
source ./kamu-money-pg/yb/node-limits.sh

RUN_ID="money-yb-$$-$(od -An -N4 -tx4 /dev/urandom | tr -d ' \n')"
NAME="$RUN_ID"
# EXIT is not enough: a kill during the readiness wait would orphan the container,
# which the shared dockerd cannot afford. Trap the signals too.
cleanup() { docker rm -f "$NAME" >/dev/null 2>&1 || true; return 0; }
trap cleanup EXIT INT TERM HUP

echo "starting YB ($YB_IMAGE) as $NAME ..."
docker run -d --name "$NAME" --label "kamu-money-pg.ybtest=$RUN_ID" \
  --memory "$YB_NODE_MEM" --memory-swap "$YB_NODE_MEM" \
  "$YB_IMAGE" bin/yugabyted start --background=false \
    --tserver_flags="memory_limit_hard_bytes=$YB_TSERVER_MEM_BYTES" \
    --master_flags="memory_limit_hard_bytes=$YB_MASTER_MEM_BYTES" >/dev/null

# yugabyted binds YSQL to the node's advertised address, not loopback -- discover it.
# READINESS IS A QUERY THAT ANSWERED, NOT AN ADDRESS THAT RESOLVED. The guard after this loop
# used to be `[ -n "$HOST" ]`, and `hostname -i` succeeds within a second of the container
# starting -- so a node whose YSQL never came up at all still reported ready, four minutes later,
# and the real failure surfaced as a confusing error from whatever ran next. The loop already
# broke only on a successful `SELECT 1`; nothing recorded that it had.
HOST=""
READY=0
for _ in $(seq 1 120); do
  HOST="$(docker exec "$NAME" hostname -i 2>/dev/null | awk '{print $1}')" || true
  if [ -n "${HOST:-}" ] && docker exec "$NAME" bin/ysqlsh -h "$HOST" -U yugabyte -c 'SELECT 1' \
      >/dev/null 2>&1; then
    READY=1
    break
  fi
  sleep 2
done
[ "$READY" = 1 ] || { echo "YB never answered a query (last address: ${HOST:-none})" >&2; exit 3; }
echo "YB ready at $HOST: $(docker exec "$NAME" bin/ysqlsh -h "$HOST" -U yugabyte -X -t -c 'SELECT version();' | tr -s ' ')"

yb_ensure_extension "$NAME" "$ART"
docker cp "$SQLFILE" "$NAME:/tmp/probe.sql"
echo "kmoney present on YB ($YB_INSTALL_MODE, sha256 $YB_INSTALL_SHA); running $(basename "$SQLFILE") ..."

# ON_ERROR_STOP=0: the battery deliberately provokes errors whose TEXT must match
# A/B. Capture stdout+stderr verbatim.
#
# The client's exit status is CAPTURED, not erased. The previous unconditional `|| true` threw
# away the one signal that distinguishes "the battery ran and some probes errored as designed"
# from "ysqlsh could not connect / the file was missing / the backend died" -- under
# ON_ERROR_STOP=0 an expected SQL error does NOT set the status, so a nonzero one is structural
# (review F4).
#
# THE STREAMS ARE MERGED INSIDE THE CONTAINER, and that is not a style choice. `docker exec`
# carries stdout and stderr as two SEPARATELY MULTIPLEXED channels over one connection, so a
# host-side `2>&1` merges two streams that have already lost their relative order in transit --
# it can only interleave whatever arrived, whenever it arrived. This script merged on the host
# until 2026-07-26, and it produced exactly the failure that predicts: the 2026-07-25 release run
# failed `yb-ab` with a single diff hunk in which one expected ERROR line and the `== 4. ... ==`
# section marker that should follow it had swapped places. Both engines had independently passed
# all 20 oracle assertions; re-running against the same commit, artifacts and image digest passed
# byte-exactly. A gate that fails once in N on stream scheduling is not a gate -- the next person
# re-runs it, it goes green, and the re-run teaches everyone that red means "try again".
#
# `run-yb-regress.sh` already knew this and merges inside the container (it measured the same
# defect putting expected-error lines one `\echo` section late, in 2 of 11 cases). The fix here is
# that pattern, not a new idea: one `bash -c` in the container, `2>&1` applied by that shell to
# ysqlsh's own two file descriptors, so ordering is settled at the source and the host only ever
# sees one already-ordered byte stream.
#
# `exec` inside the container replaces that bash with ysqlsh, so `docker exec` still reports
# ysqlsh's own exit status. (The house rule against `exec` is about a shell whose TRAP owns a
# resource -- this script's trap is out here, and out here nothing is exec'd.)
set +e
docker exec "$NAME" bash -c \
  'exec bin/ysqlsh -h "$1" -U yugabyte -X -v ON_ERROR_STOP=0 -f /tmp/probe.sql 2>&1' \
  ysqlsh "$HOST" > "$OUT"
CLIENT_STATUS=$?
set -e

echo "=== battery output -> $OUT ==="
cat "$OUT"

# Fail closed. `yb-ab` only proves the two outputs are IDENTICAL; identical nonsense would pass.
# This asserts a real run reached the end and produced its named results and refusals, so that
# equality means something.
./kamu-money-pg/yb/assert-battery.sh "$OUT" yb "$CLIENT_STATUS"
