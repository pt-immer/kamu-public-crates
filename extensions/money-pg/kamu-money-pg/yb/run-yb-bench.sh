#!/usr/bin/env bash
# What `kmoney` costs on YugabyteDB, measured rather than assumed.
#
#   kamu-money-pg/yb/run-yb-bench.sh [yb-image] [artifact-dir] [rows]
#
# P2.1 of the readiness plan (gap G7): there was no performance data for the YugabyteDB path at
# all. `kmoney` is an 18-byte pass-by-reference type, so every function result is a palloc, and
# that cost was described as "plausibly fine and entirely unmeasured". This measures it.
#
# NO PASS/FAIL THRESHOLD, DELIBERATELY. A first measurement has nothing to regress against, and a
# number invented today to be a limit tomorrow is worse than no limit: it would either be so loose
# it never fires or so tight it fires on an unrelated machine. What this produces is a recorded
# baseline. A threshold belongs in a second run, against these numbers, on comparable hardware.
#
# WHAT IT COMPARES. The three realistic ways to store an amount:
#   kmoney            -- 18 bytes, currency included, arithmetic in the backend
#   text              -- the canonical form, what the phase-4 driver adapters use
#   numeric(36,18)    -- what a schema does today, and it needs a currency column beside it
# The numeric column is measured WITH its companion currency column, because comparing an 18-byte
# self-describing value against a bare numeric flatters kmoney by pricing only half the schema.
#
# READ THE NUMBERS AS RATIOS, NOT ABSOLUTES. This runs a 3-node cluster inside one docker daemon on
# one machine; the absolute throughput says more about that than about YugabyteDB.
set -euo pipefail
cd "$(dirname "$0")/../.."   # repo root

# ONE WRITER AT A TIME, TAKEN BEFORE ANYTHING SHARED IS TOUCHED. This script reads or writes the
# fixed scratch paths under kamu-money-pg/yb/out/, which every other suite also uses; a 2026-07-26
# review found several entry points reaching those paths before -- or entirely without -- taking
# the lock, so a stray run could overwrite the artefact triplet a release was in the middle of
# hashing. Re-entrant: a suite started by `release-check` inherits the descriptor and proceeds.
# shellcheck source=kamu-money-pg/yb/workspace-lock.sh
source ./kamu-money-pg/yb/workspace-lock.sh
workspace_lock "$(basename "$0")" || exit 1

YB_IMAGE="${1:-$(./kamu-money-pg/yb/yb-image.sh)}"
ART="${2:-kamu-money-pg/yb/out}"
ROWS="${3:-20000}"
N=3

# shellcheck source=kamu-money-pg/yb/cluster.sh
source ./kamu-money-pg/yb/cluster.sh

yb_cluster_up "$N" "$YB_IMAGE"
yb_install_extension_on_all "$ART"
yb_sql 0 -c 'CREATE EXTENSION kmoney' >/dev/null

OUT="kamu-money-pg/yb/out/bench"
mkdir -p "$OUT"
REPORT="$OUT/report.txt"
: > "$REPORT"

say() { printf '%s\n' "$*" | tee -a "$REPORT"; }

# Milliseconds for one SQL statement, from psql's own `\timing` rather than the shell's clock: the
# shell would be timing docker exec, ysqlsh startup and the network as well, which on a 20k-row
# insert is noise but on a scalar SELECT would be most of the measurement.
#
# Fed on STDIN, not through `-c`. `psql -c` takes SQL, and a backslash meta-command mixed into the
# same string does not reliably run -- measured: the first version of this script produced no
# timing at all, and `set -o pipefail` then turned the empty grep into a bare exit 1 with no
# message. Hence also the `|| true` and the explicit empty check: a benchmark that cannot measure
# something must say so in its own report, not die halfway through it.
timed() {
    local node="$1" sql="$2" out
    out=$(printf '\\timing on\n%s\n' "$sql" \
        | docker exec -i "${YB_NODES[$node]}" bash -c \
            "exec bin/ysqlsh -h ${YB_HOSTS[$node]} -U yugabyte -X -q -t -A 2>&1" \
        | grep -oE 'Time: [0-9.]+ ms' | tail -1 | grep -oE '^[0-9.]+|[0-9.]+' | head -1 || true)
    printf '%s' "${out:-n/a}"
}

say "kmoney on YugabyteDB -- baseline measurements"
say "image   $YB_IMAGE"
say "cluster $N nodes, RF=3, $(nproc) host cores"
say "rows    $ROWS per table"
say "server  $(yb_sql 0 -c 'SELECT version()' | tr -s ' ')"
say ""

echo "=== creating tables ==="
yb_sql 0 -c "CREATE TABLE b_kmoney  (id int PRIMARY KEY, amount kmoney('IDR')) SPLIT INTO 6 TABLETS" >/dev/null
yb_sql 0 -c "CREATE TABLE b_text    (id int PRIMARY KEY, amount text NOT NULL) SPLIT INTO 6 TABLETS" >/dev/null
yb_sql 0 -c "CREATE TABLE b_numeric (id int PRIMARY KEY, amount numeric(36,18) NOT NULL, currency char(3) NOT NULL) SPLIT INTO 6 TABLETS" >/dev/null

# One generator expression, three columns, so the three tables hold the SAME amounts and no table
# is measured on cheaper data than another.
GEN="generate_series(1, $ROWS) g"
VAL="(g % 1000000) || '.' || lpad((g % 100)::text, 2, '0') || '000000000000001'"

say "INSERT ... SELECT of $ROWS rows (ms)"
t_k=$(timed 0 "INSERT INTO b_kmoney  SELECT g, ('IDR ' || $VAL)::kmoney FROM $GEN;")
t_t=$(timed 0 "INSERT INTO b_text    SELECT g, 'IDR ' || $VAL FROM $GEN;")
t_n=$(timed 0 "INSERT INTO b_numeric SELECT g, ($VAL)::numeric(36,18), 'IDR' FROM $GEN;")
say "  kmoney          ${t_k}"
say "  text            ${t_t}"
say "  numeric+char(3) ${t_n}"
say ""

say "full scan, projecting the amount as text (ms)"
s_k=$(timed 0 "SELECT count(amount::text) FROM b_kmoney;")
s_t=$(timed 0 "SELECT count(amount) FROM b_text;")
s_n=$(timed 0 "SELECT count(currency || ' ' || amount::text) FROM b_numeric;")
say "  kmoney          ${s_k}"
say "  text            ${s_t}"
say "  numeric+char(3) ${s_n}"
say ""

say "point lookup by primary key, 200 consecutive ids (ms)"
p_k=$(timed 0 "SELECT count(amount::text) FROM b_kmoney  WHERE id BETWEEN 1000 AND 1199;")
p_t=$(timed 0 "SELECT count(amount) FROM b_text WHERE id BETWEEN 1000 AND 1199;")
p_n=$(timed 0 "SELECT count(currency || ' ' || amount::text) FROM b_numeric WHERE id BETWEEN 1000 AND 1199;")
say "  kmoney          ${p_k}"
say "  text            ${p_t}"
say "  numeric+char(3) ${p_n}"
say ""

say "aggregate over the whole table (ms)"
# THE PALLOC QUESTION, ASKED DIRECTLY. kmoney_sum is variadic over an array_agg, so this pays one
# palloc per result across $ROWS values -- the cost the readiness plan flagged as unmeasured. The
# numeric side is PostgreSQL's own sum() and is the fastest thing on offer; that is the point of
# putting it here.
a_k=$(timed 0 "SELECT kmoney_sum(VARIADIC array_agg(amount))::text FROM b_kmoney;")
a_n=$(timed 0 "SELECT sum(amount) FROM b_numeric;")
say "  kmoney_sum over array_agg   ${a_k}"
say "  numeric sum() aggregate     ${a_n}"
say "  (kmoney has NO sum aggregate by design -- R2-F4 -- so this is not the same operation:"
say "   the numeric side streams, the kmoney side materialises an array first.)"
say ""

say "in-backend arithmetic, $ROWS additions (ms)"
r_k=$(timed 0 "SELECT count(amount + 'IDR 0.01'::kmoney) FROM b_kmoney;")
r_n=$(timed 0 "SELECT count(amount + 0.01) FROM b_numeric;")
say "  kmoney +        ${r_k}"
say "  numeric +       ${r_n}"
say ""

say "on-disk size of $ROWS rows (bytes, table only)"
z_k=$(yb_sql 0 -c "SELECT sum(pg_column_size(amount)) FROM b_kmoney" | tr -d ' ')
z_t=$(yb_sql 0 -c "SELECT sum(pg_column_size(amount)) FROM b_text" | tr -d ' ')
z_n=$(yb_sql 0 -c "SELECT sum(pg_column_size(amount) + pg_column_size(currency)) FROM b_numeric" | tr -d ' ')
say "  kmoney          ${z_k}"
say "  text            ${z_t}"
say "  numeric+char(3) ${z_n}"
say ""

# Correctness is asserted even here. A benchmark that measured a type which had started returning
# wrong answers would report excellent numbers, and this is the cheapest possible guard against
# publishing them.
#
# THE COMPARISON GOES THROUGH kmoney's OWN PARSER, not through string equality. The two render
# differently by design and always did: kmoney emits the canonical form (significant digits, floored
# at the settlement exponent) while numeric(36,18) pads to its declared scale, so the identical
# total prints as `IDR 200019900.0000000000002` on one side and `200019900.000000000000200000` on
# the other. A naive string compare called that a disagreement on this script's first run. Feeding
# numeric's total back through `::kmoney` asks the right question -- *is it the same money* -- and
# uses the one codec both paths already share to answer it.
sum_k="$(yb_sql 0 -c "SELECT kmoney_sum(VARIADIC array_agg(amount))::text FROM b_kmoney")"
sum_n="$(yb_sql 0 -c "SELECT sum(amount)::text FROM b_numeric")"
agree="$(yb_sql 0 -c "SELECT (SELECT kmoney_sum(VARIADIC array_agg(amount))::text FROM b_kmoney)
                          = ('IDR ' || (SELECT sum(amount)::text FROM b_numeric))::kmoney::text" | tr -d ' ')"
say "cross-check: kmoney total  ${sum_k}"
say "             numeric total ${sum_n}"
if [ "$agree" = "t" ]; then
    say "             AGREE (numeric's total, re-read through kmoney, is the same money) -- so the"
    say "             numbers above came from a type still returning the right answers"
else
    say "             DISAGREE -- do not use these numbers; kmoney and numeric no longer agree"
    echo "run-yb-bench: FAILED -- the correctness cross-check disagreed" >&2
    exit 1
fi

echo
echo "=== report written to $REPORT ==="
cat "$REPORT"
