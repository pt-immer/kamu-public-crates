#!/usr/bin/env bash
# NEGATIVE CONTROL for run-suite.sh: prove the oracle still rejects things.
#
#   kamu-money-pg/tests/pg_regress/selftest.sh
#
# run-suite.sh decides whether `kmoney` behaves correctly on YugabyteDB. Nothing decides whether
# run-suite.sh still decides anything. An oracle that has quietly rotted into always-passing --
# a normalizer that eats too much, a diff whose exit status is swallowed, a completeness check
# that counts zero as fine -- certifies whatever the system currently does, and it does so
# silently, which is the worst available failure mode for a money type.
#
# So this feeds the runner a FAKE CLIENT whose output is under this script's control, and checks
# that a correct output passes and that each realistic corruption is rejected FOR ITS OWN REASON.
# It is the same rule yb/assert-battery-selftest.sh applies to the ABI battery's oracle.
#
# NEEDS NO DATABASE AND NO DOCKER, so it runs in `just check` beside the offline tests.
set -euo pipefail

SUITE="$(cd "$(dirname "$0")" && pwd)"
TMP="$(mktemp -d)"
cleanup() { rm -rf "$TMP"; }
trap cleanup EXIT INT TERM HUP

# The sandbox is a copy of the real suite, so the goldens under test are the real goldens. Two
# cases are enough to cover both shapes the oracle has to police: plain values (04-sum) and error
# text whose location prefix must be normalized away (02-text).
mkdir -p "$TMP/sql" "$TMP/expected"
cp "$SUITE/run-suite.sh" "$TMP/"
for c in 02-text 04-sum; do
    cp "$SUITE/sql/$c.sql"      "$TMP/sql/"
    cp "$SUITE/expected/$c.out" "$TMP/expected/"
done

# The fake client. It ignores the SQL on stdin and replays a fixture, after PREPENDING a psql
# error-location prefix to every ERROR line -- the form `psql -f` emits, which the goldens do not
# carry because a client reading stdin omits it. That makes the run exercise the normalizer rather
# than tiptoe around it. FAKE_STATUS lets a probe make the client die.
cat > "$TMP/fake-client" <<'FAKE'
#!/usr/bin/env bash
case="${FAKE_CASE:?}"
sed -E "s|^ERROR:|psql:<stdin>:${FAKE_LINE:-77}: ERROR:|" < "$FAKE_FIXTURE_DIR/$case.fixture"
exit "${FAKE_STATUS:-0}"
FAKE
chmod +x "$TMP/fake-client"
export FAKE_FIXTURE_DIR="$TMP/fixtures"
mkdir -p "$FAKE_FIXTURE_DIR"

pass=0
fail=0
ok()  { printf '  \033[32mok\033[0m    %s\n' "$1"; pass=$((pass+1)); }
bad() { printf '  \033[31mFAIL\033[0m  %s\n' "$1"; fail=$((fail+1)); }

# Run the runner over ONE case with the fixture currently in place. Echoes its output so the
# probes below can insist on a specific REASON, not merely a nonzero exit.
run_case() {
    local case="$1"
    FAKE_CASE="$case" "$TMP/run-suite.sh" \
        --client "$TMP/fake-client" --label selftest --outdir "$TMP/results" "$case" 2>&1
}

# `expect_reject <case> <regex> <description>` -- the runner must fail AND say why.
# A probe that only checked "it failed" would be satisfied by the runner crashing on a typo, which
# is how a broken oracle passes its own selftest.
expect_reject() {
    local case="$1" why="$2" desc="$3" out
    set +e
    out="$(run_case "$case")"
    local rc=$?
    set -e
    if [ "$rc" -eq 0 ]; then
        bad "$desc -- the runner ACCEPTED it"
    elif printf '%s' "$out" | grep -qE "$why"; then
        ok "$desc"
    else
        bad "$desc -- rejected, but for the wrong reason: $(printf '%s' "$out" | grep -E 'FAIL' | head -1)"
    fi
}

expect_accept() {
    local case="$1" desc="$2" out
    set +e
    out="$(run_case "$case")"
    local rc=$?
    set -e
    if [ "$rc" -eq 0 ]; then
        ok "$desc"
    else
        bad "$desc -- the runner REJECTED a correct output: $(printf '%s' "$out" | grep -E 'FAIL' | head -1)"
    fi
}

golden() { cp "$SUITE/expected/$1.out" "$FAKE_FIXTURE_DIR/$1.fixture"; }

echo "selftest: probing run-suite.sh's oracle"

# --- the positive control ----------------------------------------------------------------------
# Without this, every rejection below could be the runner rejecting everything.
golden 02-text; golden 04-sum
expect_accept 02-text "a correct output is ACCEPTED (the positive control)"
expect_accept 04-sum  "a correct output is ACCEPTED, second case"

# --- the normalizer does exactly its job, and no more -------------------------------------------
# Both halves matter. The prefix must be stripped whatever line number it carries -- AND a golden
# with no prefix at all (which is what a stdin client produces, and what these goldens are) must
# still match output that has one. That asymmetry is not hypothetical: the suite's first run on
# YugabyteDB failed 9 of 11 cases on it, with byte-identical message text underneath.
golden 02-text
FAKE_LINE=4242 expect_accept 02-text "a psql location prefix is stripped whatever line it names"

# The cursor position moves when the .sql file is merely re-indented, so it is normalized away --
# but ONLY as a trailing ` at character N`. This checks the normalizer is that narrow: appending a
# position must still pass, while the same phrase in the MIDDLE of a message must not.
golden 02-text
sed -i 's/, in "ZWL 1.00"$/, in "ZWL 1.00" at character 8/' "$FAKE_FIXTURE_DIR/02-text.fixture"
expect_accept 02-text "a trailing 'at character N' is stripped (it is a byte offset into the .sql text)"

golden 02-text
sed -i 's/invalid money literal/invalid money literal at character 8 oops/' "$FAKE_FIXTURE_DIR/02-text.fixture"
expect_reject 02-text 'output differs' "'at character N' INSIDE a message is NOT stripped"

# --- value corruptions --------------------------------------------------------------------------
golden 04-sum
sed -i 's/^USD 11\.00$/USD 11.01/' "$FAKE_FIXTURE_DIR/04-sum.fixture"
expect_reject 04-sum 'output differs' "one cent changed in a money value is REJECTED"

golden 04-sum
sed -i 's/^empty_sum_is_null=true$/empty_sum_is_null=false/' "$FAKE_FIXTURE_DIR/04-sum.fixture"
expect_reject 04-sum 'output differs' "a boolean assertion flipped is REJECTED"

# --- error-text corruptions ---------------------------------------------------------------------
# The refusal MESSAGES are part of the contract: they are what an application matches on, and the
# #[pg_test(error = ...)] attributes pin them character for character.
golden 02-text

sed -i 's/is not an ISO 4217 code/is not an ISO4217 code/' "$FAKE_FIXTURE_DIR/02-text.fixture" || true
sed -i 's/invalid money literal/invalid money literals/' "$FAKE_FIXTURE_DIR/02-text.fixture"
expect_reject 02-text 'output differs' "a single letter changed in a refusal message is REJECTED"

golden 02-text
# An error that stops being raised must also fail the oracle.
before=$(wc -l < "$FAKE_FIXTURE_DIR/02-text.fixture")
sed -i '/^ERROR:  kmoney: invalid money literal/d' "$FAKE_FIXTURE_DIR/02-text.fixture"
# The mutation itself is asserted. A `sed` whose pattern has gone stale deletes nothing, the
# fixture stays correct, the runner accepts it -- and the probe reports that the oracle failed to
# reject a corruption that was never applied. Measured: this exact probe went red when the goldens
# dropped their `CLIENT:` prefix, and only the mutation check says which of the two is broken.
[ "$(wc -l < "$FAKE_FIXTURE_DIR/02-text.fixture")" -lt "$before" ] \
    || bad "the delete-a-refusal mutation matched nothing -- this probe is stale, not the oracle"
expect_reject 02-text 'output differs' "a refusal that stopped being raised is REJECTED"

# --- completeness --------------------------------------------------------------------------------
golden 04-sum
sed -i '/^== CASE COMPLETE: 04-sum ==$/d' "$FAKE_FIXTURE_DIR/04-sum.fixture"
expect_reject 04-sum 'found 0' "a run that died before the end is REJECTED"

golden 04-sum
cat "$SUITE/expected/04-sum.out" >> "$FAKE_FIXTURE_DIR/04-sum.fixture"
expect_reject 04-sum 'found 2' "a file holding two half-runs is REJECTED"

: > "$FAKE_FIXTURE_DIR/04-sum.fixture"
expect_reject 04-sum 'found 0' "an empty output is REJECTED"

# --- the client's own status ----------------------------------------------------------------------
# Under ON_ERROR_STOP=0 an EXPECTED SQL error does not set the exit status, so a nonzero one is
# structural -- could not connect, backend died -- and must not be waved through just because the
# bytes happen to match.
golden 04-sum
FAKE_STATUS=2 expect_reject 04-sum 'client exited 2' "a perfect output from a client that DIED is REJECTED"

# --- a case with no golden -------------------------------------------------------------------------
# Not a pass and not a skip. This is the rule the readiness plan insists on and the one
# native-driver-test.sh learned the hard way.
golden 04-sum
mv "$TMP/expected/04-sum.out" "$TMP/expected/04-sum.out.hidden"
expect_reject 04-sum 'no golden|proves nothing' "a case with NO golden is REJECTED rather than skipped"
mv "$TMP/expected/04-sum.out.hidden" "$TMP/expected/04-sum.out"

echo
if [ "$fail" -eq 0 ]; then
    echo "selftest: OK -- $pass/$pass probes; the suite oracle still accepts only correct output"
else
    echo "selftest: FAILED -- $fail of $((pass+fail)) probes. run-suite.sh cannot be trusted to certify anything until this is green." >&2
    exit 1
fi
