#!/usr/bin/env bash
# Controls for the artifact selector the YugabyteDB image's copy-out step runs.
#
#   kamu-money-pg/yb/exactly-one-selftest.sh
#
# The selector's whole job is to REFUSE, and a refusal that never fires is indistinguishable from
# one that cannot. Inside a Docker build it cannot be falsified without a build argument invented
# to break it, so the branches are exercised here instead: no Docker, no database, no compiler.
#
# The two-match cases are the ones that matter. They are what a build tree looks like when it has
# accumulated more than one version's or more than one major's output, and picking either of them
# silently is the failure the selector exists to prevent.
set -euo pipefail
cd "$(dirname "$0")/../.." # lane root

SELECT=./kamu-money-pg/yb/exactly-one.sh

WORK="$(mktemp -d)"
cleanup() {
    rm -rf "$WORK"
    return 0
}
trap cleanup EXIT INT TERM HUP

pass=0
fail=0
ok() {
    printf '  \033[32mok\033[0m    %s\n' "$1"
    pass=$((pass + 1))
}
bad() {
    printf '  \033[31mFAIL\033[0m  %s\n' "$1"
    fail=$((fail + 1))
}

# Accepts, and prints the one path.
expect_one() {
    local label="$1" root="$2" pattern="$3" want="$4" got status
    got="$("$SELECT" "$root" "$pattern" 2>/dev/null)" && status=0 || status=$?
    if [ "$status" -ne 0 ]; then
        bad "$label (refused with status $status, should have accepted)"
    elif [ "$got" != "$want" ]; then
        bad "$label (printed '$got', wanted '$want')"
    else
        ok "$label"
    fi
}

# Refuses with the given status, and says why on stderr.
expect_refuse() {
    local label="$1" root="$2" pattern="$3" want_status="$4" want_text="$5" out status
    out="$("$SELECT" "$root" "$pattern" 2>&1 >/dev/null)" && status=0 || status=$?
    if [ "$status" -ne "$want_status" ]; then
        bad "$label (status $status, wanted $want_status)"
    elif ! printf '%s' "$out" | grep -q "$want_text"; then
        bad "$label (diagnostic did not mention '$want_text': $out)"
    else
        ok "$label"
    fi
}

echo "exactly-one-selftest: controls for the YugabyteDB copy-out artifact selector"

# --- the accepting case, which is also the positive control ------------------------------------
# Without it, every assertion below would still pass against a selector that refuses everything.
mkdir -p "$WORK/one/release/kmoney-pg15/lib"
: >"$WORK/one/release/kmoney-pg15/lib/kmoney.so"
expect_one "exactly one match is printed" \
    "$WORK/one" "kmoney*.so" "$WORK/one/release/kmoney-pg15/lib/kmoney.so"

# --- refusals ----------------------------------------------------------------------------------
mkdir -p "$WORK/none/release"
expect_refuse "no match is refused" "$WORK/none" "kmoney*.so" 1 "found 0"

# A stale artifact from an earlier version, beside the current one.
mkdir -p "$WORK/two/release/kmoney-pg15/share"
: >"$WORK/two/release/kmoney-pg15/share/kmoney--0.1.0.sql"
: >"$WORK/two/release/kmoney-pg15/share/kmoney--0.2.0.sql"
expect_refuse "two versions of the install script are refused" \
    "$WORK/two" "kmoney--*.sql" 1 "found 2"

# Two majors' staging directories, which is how a triplet could be assembled from two builds.
mkdir -p "$WORK/majors/release/kmoney-pg15/lib" "$WORK/majors/release/kmoney-pg18/lib"
: >"$WORK/majors/release/kmoney-pg15/lib/kmoney.so"
: >"$WORK/majors/release/kmoney-pg18/lib/kmoney.so"
expect_refuse "the same name under two majors is refused" \
    "$WORK/majors" "kmoney*.so" 1 "found 2"

# Both offending paths are named, so the diagnostic identifies the build to remove.
expect_refuse "the refusal names every match it found" \
    "$WORK/majors" "kmoney*.so" 1 "kmoney-pg18"

# --- usage refusals, distinguished from a wrong count by their status ---------------------------
expect_refuse "a root that does not exist is refused" "$WORK/absent" "kmoney*.so" 2 "no directory"

if ./kamu-money-pg/yb/exactly-one.sh "$WORK/one" >/dev/null 2>&1; then
    bad "a missing pattern argument is accepted"
else
    ok "a missing pattern argument is refused"
fi

echo
if [ "$fail" -ne 0 ]; then
    printf 'exactly-one-selftest: \033[31m%d failed\033[0m, %d passed\n' "$fail" "$pass"
    exit 1
fi
printf 'exactly-one-selftest: \033[32mall %d controls passed\033[0m\n' "$pass"
