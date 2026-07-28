#!/usr/bin/env bash
# Dump a schema holding `kmoney`, restore it into a CLEAN cluster, and prove nothing moved.
#
#   kamu-money-pg/yb/run-yb-restore.sh [yb-image] [artifact-dir]
#
# WHY THIS EXISTS. Restore readiness was documented as unrehearsed and deferred "until a version
# migration". An external review disagreed with the second half and was right: a rolling VERSION
# upgrade can wait for a migration to be planned, but restore is needed the moment the first
# production value exists -- and the first time anybody runs it must not be during an incident.
#
# WHAT THIS REPOSITORY CAN AND CANNOT ANSWER, stated up front so the result is not over-read.
# `kmoney` is a library and an extension, not a deployment system, so the operator-owned half --
# RPO, RTO, backup cadence, storage, who runs it -- belongs to the platform and is deliberately
# not simulated here. What IS a property of the extension, and therefore this repository's to
# prove:
#
#   * a dump of a schema using `kmoney` reproduces the extension through `CREATE EXTENSION`, and
#     restores against a clean catalog;
#   * the 18-byte payloads survive byte-for-byte, at the domain edges as well as in the middle;
#   * typmod-pinned columns come back still pinned, and still refusing the wrong currency;
#   * totals computed after the restore agree with totals computed before it;
#   * and a destination WITHOUT the extension files fails loudly at `CREATE EXTENSION` rather than
#     restoring a table whose column type does not exist.
#
# That last one is the property PostgreSQL's own documentation describes: a dump represents an
# extension as `CREATE EXTENSION`, and does NOT dump its member objects individually -- so the
# extension's files have to exist at the destination when the restore reaches that statement. It
# is also the sharpest argument for the node image: "restore into a cluster on digest D" is one
# question, where "did somebody install the extension on the restore target first" is a runbook
# step that gets skipped at 3am.
#
# TWO SINGLE-NODE INSTANCES, NOT TWO CLUSTERS. The property under test is catalog and payload
# round-trip, which is not a distribution question -- run-yb-cluster.sh already covers every-node
# behaviour, and two 3-node clusters would cost six containers to re-prove it.
set -euo pipefail
cd "$(dirname "$0")/../.."   # repo root

# ONE WRITER AT A TIME, TAKEN BEFORE ANYTHING SHARED IS TOUCHED. This script reads and writes under
# ${KMONEY_RUN_ROOT:-kamu-money-pg/yb/out}, which with that variable unset is the single tree
# every other suite also uses; a 2026-07-26 review found several entry points reaching those paths
# before -- or entirely without -- taking the lock, so a stray run could overwrite the artefact
# triplet a release was in the middle of hashing. Setting KMONEY_RUN_ROOT gives a run its own tree,
# which removes the contention rather than serialising it; the lock stays for the shared default.
# Re-entrant: a suite started by `release-check` inherits the descriptor and proceeds.
# shellcheck source=kamu-money-pg/yb/workspace-lock.sh
source ./kamu-money-pg/yb/workspace-lock.sh
workspace_lock "$(basename "$0")" || exit 1

YB_IMAGE="${1:-$(./kamu-money-pg/yb/yb-image.sh)}"
ART="${2:-${KMONEY_RUN_ROOT:-kamu-money-pg/yb/out}}"

# shellcheck source=kamu-money-pg/yb/install.sh
source ./kamu-money-pg/yb/install.sh

RUN_ID="kmoney-restore-$$-$(od -An -N4 -tx4 /dev/urandom | tr -d ' \n')"
SRC="$RUN_ID-src"
DST="$RUN_ID-dst"
BARE="$RUN_ID-bare"
WORK="kamu-money-pg/yb/out/restore-$RUN_ID"

# Every container belongs to this script, by LABEL -- the daemon is shared across several
# organisations' runners, so cleanup can never be scoped by name prefix or image. EXIT alone is
# not enough: a kill during a readiness wait would orphan three YugabyteDB nodes.
cleanup() {
    docker rm -f "$SRC" "$DST" "$BARE" >/dev/null 2>&1 || true
    return 0
}
trap cleanup EXIT INT TERM HUP
mkdir -p "$WORK"

fail=0
ok()  { printf '  \033[32mok\033[0m    %s\n' "$1"; }
bad() { printf '  \033[31mFAIL\033[0m  %s\n' "$1"; fail=$((fail + 1)); }

# One node, started and waited for. Prints the address it is listening on.
node_up() {
    local name="$1" image="$2" host="" ready=0
    docker run -d --name "$name" --label "kamu-money-pg.ybtest=$RUN_ID" \
        "$image" bin/yugabyted start --background=false >/dev/null
    local _
    # READINESS IS A QUERY THAT ANSWERED, NOT AN ADDRESS THAT RESOLVED. The guard below used to
    # be `[ -n "$host" ]`, and `hostname -i` succeeds within a second of the container starting
    # -- so a node whose YSQL never came up at all still returned its address as "ready", four
    # minutes later, and the real failure surfaced as a confusing error from the first query
    # after it. The loop already broke only on a successful `SELECT 1`; nothing recorded that.
    for _ in $(seq 1 120); do
        host="$(docker exec "$name" hostname -i 2>/dev/null | awk '{print $1}')" || true
        if [ -n "${host:-}" ] && docker exec "$name" bin/ysqlsh -h "$host" -U yugabyte \
                -c 'SELECT 1' >/dev/null 2>&1; then
            ready=1
            break
        fi
        sleep 2
    done
    if [ "$ready" != 1 ]; then
        echo "restore: $name never answered a query (last address: ${host:-none})" >&2
        return 1
    fi
    printf '%s\n' "$host"
}

# Streams merged INSIDE the container -- `docker exec` multiplexes stdout and stderr separately,
# so a host-side `2>&1` cannot order them, and these outputs are read as results.
sql_on() {
    local name="$1" host="$2" sql="$3"
    docker exec "$name" bash -c \
        'exec bin/ysqlsh -h "$1" -U yugabyte -X -q -t -A -v ON_ERROR_STOP=1 -c "$2" 2>&1' \
        ysqlsh "$host" "$sql"
}

echo "run-yb-restore: image $YB_IMAGE"
echo

# --- 1. a source instance, with a representative consuming schema -------------------------------
SRC_HOST="$(node_up "$SRC" "$YB_IMAGE")"
yb_ensure_extension "$SRC" "$ART"
echo "restore: source ready at $SRC_HOST (kmoney $YB_INSTALL_MODE)"

# A CONSUMING schema, not a toy: a typmod-pinned column (which the dump must carry as a type
# modifier, not merely as `kmoney`), an unpinned one, and a NOT NULL constraint. Values include
# both domain edges and a currency the pinned column must refuse, so the restored constraint can
# be tested rather than assumed.
sql_on "$SRC" "$SRC_HOST" "
CREATE EXTENSION kmoney;
CREATE TABLE account (
    id      bigint PRIMARY KEY,
    balance kmoney('USD') NOT NULL,
    fee     kmoney
);
INSERT INTO account VALUES
    (1, 'USD 0.00',   'USD 0.000000000000000001'),
    (2, 'USD 10.50',  'IDR 16000.00'),
    (3, 'USD 999999999999999999.999999999999999999',  NULL),
    (4, 'USD -999999999999999999.999999999999999999', 'USD -0.000000000000000001');
" >/dev/null

# THE FINGERPRINT, taken before the dump and re-taken after the restore. Hashing `kmoney_send`'s
# output rather than the text form is deliberate: text goes through the output function, so a
# renderer that changed identically on both sides would agree while the STORED BYTES had moved.
# The 18-byte payload is what the dump has to preserve.
fingerprint_of() {
    local name="$1" host="$2"
    sql_on "$name" "$host" "
        SELECT id || '|' || encode(kmoney_send(balance), 'hex')
                  || '|' || coalesce(encode(kmoney_send(fee), 'hex'), 'NULL')
        FROM account ORDER BY id;"
}
BEFORE="$WORK/fingerprint-before.txt"
fingerprint_of "$SRC" "$SRC_HOST" > "$BEFORE"

# NOT an independent total, and this comment used to claim it was. Both sides run the same
# `sum(balance)` expression, so this compares one code path with itself across the restore. That
# is still worth having -- it catches an aggregate that stopped working on the destination, which
# a row-by-row comparison would not -- but the INDEPENDENT storage oracle is the byte fingerprint
# above: `kmoney_send` renders the raw 18-byte payload, and `diff` compares the bytes without
# asking any kmoney code what they mean.
TOTAL_BEFORE="$(sql_on "$SRC" "$SRC_HOST" "SELECT sum(balance)::text FROM account")"
TYPMOD_BEFORE="$(sql_on "$SRC" "$SRC_HOST" "
    SELECT format_type(atttypid, atttypmod) FROM pg_attribute
    WHERE attrelid = 'account'::regclass AND attname = 'balance'")"
VERSION_BEFORE="$(sql_on "$SRC" "$SRC_HOST" "SELECT extversion FROM pg_extension WHERE extname='kmoney'")"

# --- 2. the dump --------------------------------------------------------------------------------
# Plain SQL, so the restore is `ysqlsh -f` and the artefact is readable evidence rather than a
# binary blob nobody inspects. `--no-owner`/`--no-privileges`: role grants are the operator's, and
# a dump that fails on a missing role would fail for a reason that has nothing to do with kmoney.
DUMP="$WORK/dump.sql"
docker exec "$SRC" bash -c \
    'exec postgres/bin/ysql_dump -h "$1" -U yugabyte --no-owner --no-privileges yugabyte 2>&1' \
    ysql_dump "$SRC_HOST" > "$DUMP"

if grep -q 'CREATE EXTENSION' "$DUMP"; then
    ok "the dump reproduces the extension through CREATE EXTENSION"
else
    bad "the dump contains no CREATE EXTENSION -- the restore target would have no kmoney type"
fi
# The member objects (the type, its functions, the aggregate) must NOT be dumped individually:
# they belong to the extension and come back with it. A dump that emitted them would collide with
# the extension's own script on restore.
if grep -qE '^CREATE (TYPE|FUNCTION) (public\.)?kmoney' "$DUMP"; then
    bad "the dump emits kmoney's member objects individually; they belong to the extension"
else
    ok "the dump does NOT emit member objects individually -- they come back with the extension"
fi
if grep -q "kmoney('USD')\|kmoney(USD)" "$DUMP"; then
    ok "the typmod-pinned column keeps its modifier in the dump"
else
    bad "the pinned column lost its type modifier in the dump: $(grep -m1 balance "$DUMP" || true)"
fi

# --- 3. restore into a CLEAN instance on the same image -----------------------------------------
DST_HOST="$(node_up "$DST" "$YB_IMAGE")"
yb_ensure_extension "$DST" "$ART"
echo "restore: destination ready at $DST_HOST (clean catalog, kmoney $YB_INSTALL_MODE)"

docker cp "$DUMP" "$DST:/tmp/dump.sql"
if docker exec "$DST" bash -c \
        'exec bin/ysqlsh -h "$1" -U yugabyte -X -q -v ON_ERROR_STOP=1 -f /tmp/dump.sql 2>&1' \
        ysqlsh "$DST_HOST" > "$WORK/restore.log"; then
    ok "the dump restored into a clean cluster with ON_ERROR_STOP=1"
else
    bad "the restore failed: $(tail -3 "$WORK/restore.log" | tr '\n' ' ')"
fi

# --- 4. nothing moved ---------------------------------------------------------------------------
AFTER="$WORK/fingerprint-after.txt"
fingerprint_of "$DST" "$DST_HOST" > "$AFTER"
if diff -q "$BEFORE" "$AFTER" >/dev/null; then
    ok "every 18-byte payload survived, domain edges included ($(wc -l < "$BEFORE") rows, byte-exact)"
else
    bad "payloads changed across the restore:"
    diff "$BEFORE" "$AFTER" | sed 's/^/          /' >&2
fi

TOTAL_AFTER="$(sql_on "$DST" "$DST_HOST" "SELECT sum(balance)::text FROM account")"
if [ "$TOTAL_BEFORE" = "$TOTAL_AFTER" ]; then
    ok "sum(balance) still aggregates to the same total ($TOTAL_AFTER) -- same query both sides"
else
    bad "sum(balance) was '$TOTAL_BEFORE', is now '$TOTAL_AFTER'"
fi

VERSION_AFTER="$(sql_on "$DST" "$DST_HOST" "SELECT extversion FROM pg_extension WHERE extname='kmoney'")"
if [ "$VERSION_BEFORE" = "$VERSION_AFTER" ]; then
    ok "the extension came back at the same version ($VERSION_AFTER)"
else
    bad "extension version was '$VERSION_BEFORE', is now '$VERSION_AFTER'"
fi

TYPMOD_AFTER="$(sql_on "$DST" "$DST_HOST" "
    SELECT format_type(atttypid, atttypmod) FROM pg_attribute
    WHERE attrelid = 'account'::regclass AND attname = 'balance'")"
if [ "$TYPMOD_BEFORE" = "$TYPMOD_AFTER" ]; then
    ok "the pinned column's typmod survived ($TYPMOD_AFTER)"
else
    bad "typmod was '$TYPMOD_BEFORE', is now '$TYPMOD_AFTER'"
fi

# A typmod that came back as TEXT but not as a CONSTRAINT would pass the check above and still be
# broken. Prove it still refuses.
#
# STATUS AND OUTPUT CAPTURED SEPARATELY, not piped into `grep`. Written as
# `sql_on ... | grep -qi error`, this reported FAILURE on a CORRECT refusal: under
# `set -o pipefail` a refusing ysqlsh (`ON_ERROR_STOP=1`) makes the whole pipeline non-zero
# whatever grep decides, so the `if` took its else branch precisely when the type was doing its
# job. The negative control has to distinguish "refused" from "the command could not run", and a
# pipeline collapses both into one status.
set +e
typmod_out="$(sql_on "$DST" "$DST_HOST" "INSERT INTO account VALUES (99, 'IDR 1.00', NULL)" 2>&1)"
typmod_rc=$?
set -e
if [ "$typmod_rc" -eq 0 ]; then
    bad "the restored kmoney('USD') column accepted an IDR value -- the typmod is decorative"
elif printf '%s' "$typmod_out" | grep -qi 'error'; then
    ok "the restored pinned column still REFUSES the wrong currency: $(printf '%s' "$typmod_out" | grep -i error | head -1)"
else
    bad "the insert failed, but not with an error message: $(printf '%s' "$typmod_out" | head -2 | tr '\n' ' ')"
fi

# --- 5. the negative control: a destination without the extension files -------------------------
# PostgreSQL represents the extension as `CREATE EXTENSION`, so the files must exist at the
# destination when the restore reaches that line. If this ever restored "successfully" onto a bare
# image, the dump would not be carrying the extension at all.
BARE_HOST="$(node_up "$BARE" "$YB_IMAGE")"
docker exec "$BARE" rm -f "$YB_LIB"
docker cp "$DUMP" "$BARE:/tmp/dump.sql"
if docker exec "$BARE" bash -c \
        'exec bin/ysqlsh -h "$1" -U yugabyte -X -q -v ON_ERROR_STOP=1 -f /tmp/dump.sql 2>&1' \
        ysqlsh "$BARE_HOST" > "$WORK/restore-bare.log"; then
    bad "the dump restored onto a node with NO kmoney.so -- so it is not carrying the extension"
# THE LOADER FAILURE, NAMED. This used to accept any failed restore whose log matched the bare
# token `kmoney` -- which a syntax error, a type error or a permissions error mentioning the type
# would satisfy just as well. The control would then have reported "fails loudly for the right
# reason" about a failure it had not actually identified, which is the one thing a negative
# control must not do. Both halves must appear on ONE line: a loader-shaped phrase, and the
# extension it failed to load.
elif LOADER_LINE="$(grep -iE 'could not (load library|access file|open extension control file)' \
        "$WORK/restore-bare.log" | grep -i kmoney | head -1)" && [ -n "$LOADER_LINE" ]; then
    ok "restoring onto a node without the library fails at load: $(printf '%s' "$LOADER_LINE" | tr -s ' ')"
elif grep -qi kmoney "$WORK/restore-bare.log"; then
    bad "the restore failed mentioning kmoney but not with a loader error, so the missing library was not shown to be the cause: $(grep -iE 'ERROR' "$WORK/restore-bare.log" | head -1)"
else
    bad "the restore failed on the bare node, but not for a missing-library reason: $(tail -2 "$WORK/restore-bare.log" | tr '\n' ' ')"
fi

echo
if [ "$fail" -eq 0 ]; then
    echo "run-yb-restore: OK -- a schema holding kmoney dumps and restores into a clean cluster"
    echo "                with byte-identical payloads, live typmods and agreeing totals, and a"
    echo "                destination missing the library fails loudly rather than half-restoring."
    echo
    echo "                NOT COVERED, and not claimed: RPO/RTO, backup cadence, storage, and who"
    echo "                runs it. Those are the platform's, not this library's -- see RUNBOOK.md."
else
    echo "run-yb-restore: FAILED -- $fail probe(s)" >&2
    exit 1
fi
