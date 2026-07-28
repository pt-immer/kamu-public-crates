#!/usr/bin/env bash
# Negative and positive controls for yb-image.sh -- the gate that decides whether a YugabyteDB
# image is one anybody validated.
#
#   kamu-money-pg/yb/yb-image-selftest.sh
#
# WHY THIS EXISTS. yb-image.sh used to REFUSE a tag that had moved off its pinned digest while
# merely WARNING about a tag with no pin at all -- so the one case where nothing had ever been
# validated was the one case that proceeded, and `just yb-pin-check` printed "the tag still
# resolves to the validated digest" over the top of it. A gate whose refusal paths are never
# exercised is a gate nobody knows the shape of, so each path is driven here.
#
# NO NETWORK, AND NO YUGABYTEDB IMAGE. The fixture is a `FROM scratch` image built locally, which
# is why the identity comes back as an image ID rather than a RepoDigest -- exactly the local-image
# branch yb-image.sh reports on stderr. That keeps this runnable in a fast gate instead of behind a
# 1.6GB pull.
#
# The tag and the temp directory belong to this script's trap, not to whoever remembers.
set -euo pipefail
cd "$(dirname "$0")/../.."   # repo root

SCRIPT=./kamu-money-pg/yb/yb-image.sh
TAG="kmoney-yb-image-selftest:probe"
WORK="$(mktemp -d)"

cleanup() {
    docker rmi -f "$TAG" >/dev/null 2>&1 || true
    rm -rf "$WORK"
    return 0
}
trap cleanup EXIT INT TERM HUP

printf 'FROM scratch\n' > "$WORK/Dockerfile"
docker build -q -t "$TAG" "$WORK" >/dev/null
ID="$(docker image inspect --format '{{ .Id }}' "$TAG")"

pass=0
fail=0
ok()  { printf '  \033[32mok\033[0m    %s\n' "$1"; pass=$((pass + 1)); }
bad() { printf '  \033[31mFAIL\033[0m  %s\n' "$1"; fail=$((fail + 1)); }

# stdout and stderr are kept APART, because which stream a message lands on is itself part of the
# contract: every caller captures stdout as the image identity.
run() {
    local rc=0
    set +e
    "$SCRIPT" "$TAG" > "$WORK/out" 2> "$WORK/err"
    rc=$?
    set -e
    return "$rc"
}

pin_digest() { printf '%s\t%s\n' "$TAG" "$1" > "$WORK/pin"; export YB_PINFILE="$WORK/pin"; }

expect_accept() {
    local desc="$1" rc=0
    run || rc=$?
    if [ "$rc" -ne 0 ]; then
        bad "$desc -- REFUSED (exit $rc): $(head -1 "$WORK/err")"
    elif [ "$(cat "$WORK/out")" != "$ID" ]; then
        bad "$desc -- accepted but printed '$(cat "$WORK/out")', expected $ID"
    else
        ok "$desc"
    fi
}

expect_refuse() {
    local desc="$1" want="$2" rc=0
    run || rc=$?
    if [ "$rc" -eq 0 ]; then
        bad "$desc -- it ACCEPTED the image (stdout '$(cat "$WORK/out")')"
    elif ! grep -q "$want" "$WORK/err"; then
        bad "$desc -- refused, but not for the stated reason: $(head -2 "$WORK/err" | tr '\n' ' ')"
    elif [ -s "$WORK/out" ]; then
        bad "$desc -- refused but still printed an identity on stdout: $(cat "$WORK/out")"
    else
        ok "$desc"
    fi
}

echo "=== yb-image.sh controls (fixture: locally built $TAG) ==="

# --- the accepting path, so the refusals below are not vacuous ---------------------------------
unset YB_ALLOW_DRIFT YB_ALLOW_UNPINNED YB_PULL
pin_digest "$ID"
expect_accept "a tag at its pinned digest is accepted, and stdout carries only the identity"

# --- moved off the pin -------------------------------------------------------------------------
pin_digest "sha256:0000000000000000000000000000000000000000000000000000000000000000"
expect_refuse "a tag that has MOVED OFF its pinned digest is refused" "moved off the validated digest"

YB_ALLOW_DRIFT=1 expect_accept "YB_ALLOW_DRIFT=1 adopts a moved tag deliberately"

# THE TWO OVERRIDES ARE NOT INTERCHANGEABLE. If either rescued the other's case, an operator who
# reached for one would be bypassing a check they never considered.
YB_ALLOW_UNPINNED=1 expect_refuse \
    "YB_ALLOW_UNPINNED=1 does NOT rescue a moved pin" "moved off the validated digest"

# --- never pinned at all: THE CASE THAT USED TO PASS WITH A WARNING -----------------------------
printf '%s\t%s\n' "some-other-tag:1.0" "sha256:dead" > "$WORK/pin"
export YB_PINFILE="$WORK/pin"
expect_refuse "a tag with NO pin entry is refused -- this is the fail-open bug" "not recorded in the pin file"

YB_ALLOW_UNPINNED=1 expect_accept "YB_ALLOW_UNPINNED=1 adopts an unrecorded tag deliberately"

YB_ALLOW_DRIFT=1 expect_refuse \
    "YB_ALLOW_DRIFT=1 does NOT rescue an unrecorded tag" "not recorded in the pin file"

# --- no pin file at all ------------------------------------------------------------------------
export YB_PINFILE="$WORK/does-not-exist"
expect_refuse "a missing pin file is refused rather than read as permission" "no pin file at all"

YB_ALLOW_UNPINNED=1 expect_accept "YB_ALLOW_UNPINNED=1 bootstraps when there is no pin file yet"

echo
if [ "$fail" -eq 0 ]; then
    echo "yb-image-selftest: OK -- $pass controls, every refusal path bites and the two overrides are distinct"
else
    echo "yb-image-selftest: FAILED -- $fail of $((pass + fail)) controls" >&2
    exit 1
fi
