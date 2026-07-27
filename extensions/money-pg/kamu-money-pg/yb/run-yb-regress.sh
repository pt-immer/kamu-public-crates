#!/usr/bin/env bash
# Run the ported case suite (kamu-money-pg/tests/pg_regress) against a live single-node YugabyteDB.
#
#   kamu-money-pg/yb/run-yb-regress.sh [yb-image] [artifact-dir]
#
# This is P0.1 of the readiness plan, and the single highest-value item in it. Before this, the
# whole YugabyteDB evidence surface was one ~112-line script; the 54 #[pg_test]s that encode this
# type's contract had never run there, because `cargo pgrx test` manages its own PostgreSQL and
# cannot be aimed at a YB backend. The suite speaks the wire protocol instead, so it runs here
# unchanged -- and the stock-PG15 reference (Dockerfile.pg15) runs the SAME cases against the SAME
# hand-authored goldens, which is what makes a failure here a divergence rather than a guess about
# whether the port is faithful.
#
# Prereq: kamu-money-pg/yb/out/{kmoney.so,kmoney.control,kmoney--*.sql} built by `just yb-build`.
set -euo pipefail
cd "$(dirname "$0")/../.."   # repo root

# This script writes fixed paths under kamu-money-pg/yb/out/, so it is one writer among several
# that must not overlap: a release check reads the very files a hand-started run of this script
# would overwrite. The lock is re-entrant, so being invoked BY release-check is free.
# shellcheck source=kamu-money-pg/yb/workspace-lock.sh
source "$(dirname "$0")/workspace-lock.sh"
workspace_lock "$(basename "$0")" || exit 1

YB_IMAGE="${1:-$(./kamu-money-pg/yb/yb-image.sh)}"
ART="${2:-kamu-money-pg/yb/out}"

# Baked or copied, verified by hash either way -- see install.sh, which sources artifact.sh for the
# coherent-triplet-by-exact-name rule.
# shellcheck source=kamu-money-pg/yb/install.sh
source ./kamu-money-pg/yb/install.sh

RUN_ID="kmoney-regress-$$-$(od -An -N4 -tx4 /dev/urandom | tr -d ' \n')"
NAME="$RUN_ID"
# EXIT alone is not enough: a kill during the readiness wait would orphan the container, which the
# shared dockerd cannot afford.
cleanup() { docker rm -f "$NAME" >/dev/null 2>&1 || true; return 0; }
trap cleanup EXIT INT TERM HUP

echo "starting YugabyteDB ($YB_IMAGE) as $NAME ..."
docker run -d --name "$NAME" --label "kamu-money-pg.ybtest=$RUN_ID" \
  --label "kamu-money-pg.revision=$(git rev-parse --short HEAD 2>/dev/null || echo nogit)" \
  "$YB_IMAGE" bin/yugabyted start --background=false >/dev/null

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
echo "kmoney present ($YB_INSTALL_MODE, sha256 $YB_INSTALL_SHA); running the case suite ..."

# The client is a generated wrapper rather than a `docker exec ...` string, for two reasons that
# are both load-bearing:
#
#  1. ysqlsh's stderr is merged into its stdout INSIDE the container. `docker exec` carries the two
#     as separately multiplexed streams, so a host-side `2>&1` cannot order them -- measured, this
#     put expected-error lines one `\echo` section late, intermittently, in 2 of 11 cases.
#  2. run-suite.sh word-splits --client to append its own flags, so the quoted `bash -c` script
#     could not survive being embedded in that string. A file has no quoting problem.
#
# --server-exec is how 09-wire's crafted BINARY COPY payloads get onto the SERVER's filesystem:
# kmoney_recv takes `internal`, so COPY (FORMAT BINARY) from a file is the only in-database route
# to it, and SQL cannot write arbitrary bytes to a file.
mkdir -p kamu-money-pg/yb/out
CLIENT_WRAPPER="kamu-money-pg/yb/out/client-$RUN_ID.sh"
cat > "$CLIENT_WRAPPER" <<EOF
#!/usr/bin/env bash
exec docker exec -i $NAME bash -c 'exec bin/ysqlsh -h $HOST -U yugabyte "\$@" 2>&1' ysqlsh "\$@"
EOF
chmod +x "$CLIENT_WRAPPER"
# The wrapper names this run, so it goes when the run does.
trap 'cleanup; rm -f "$CLIENT_WRAPPER"' EXIT INT TERM HUP

# NOT `exec`. An earlier revision ran the suite with `exec`, which REPLACES this shell -- and a
# replaced shell has no traps, so every failing run left a YugabyteDB container alive on a daemon
# shared across several organisations' runners. Three of them accumulated in one session before
# `just containers` noticed. `set -e` propagates the suite's exit status here just as well, and
# the trap still fires.
./kamu-money-pg/tests/pg_regress/run-suite.sh \
    --client "$CLIENT_WRAPPER" \
    --server-exec "docker exec -i $NAME bash" \
    --label yb-native \
    --outdir kamu-money-pg/yb/out/regress-yb
