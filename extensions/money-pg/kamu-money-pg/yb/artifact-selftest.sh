#!/usr/bin/env bash
# Controls for the artifact resolver used by YugabyteDB suites.
#
#   kamu-money-pg/yb/artifact-selftest.sh
#
# Controls cover mixed-build triplets and missing manifests so the resolver cannot certify
# unverified bytes.
#
# These name, version, and hash checks need no Docker or database.
set -euo pipefail
cd "$(dirname "$0")/../.."   # repo root

# shellcheck source=kamu-money-pg/yb/artifact.sh
source ./kamu-money-pg/yb/artifact.sh

WORK="$(mktemp -d)"
cleanup() { rm -rf "$WORK"; return 0; }
trap cleanup EXIT INT TERM HUP

pass=0
fail=0
ok()  { printf '  \033[32mok\033[0m    %s\n' "$1"; pass=$((pass + 1)); }
bad() { printf '  \033[31mFAIL\033[0m  %s\n' "$1"; fail=$((fail + 1)); }

# A complete, coherent, manifested triplet. `$1` is the directory; `$2` the version (default 0.1.0).
fixture() {
    local dir="$1" version="${2:-0.1.0}"
    mkdir -p "$dir"
    printf 'ELF-not-really\n'                       > "$dir/kmoney.so"
    printf "default_version = '%s'\n" "$version"    > "$dir/kmoney.control"
    printf 'CREATE TYPE kmoney;\n'                  > "$dir/kmoney--$version.sql"
    (cd "$dir" && sha256sum kmoney.so kmoney.control "kmoney--$version.sql" \
        > "$YB_ART_MANIFEST_NAME")
}

# The resolver sets globals; isolate each case in a subshell.
resolve() {
    local dir="$1"
    (
        set +e
        yb_resolve_artifacts "$dir" > "$WORK/out" 2> "$WORK/err"
        rc=$?
        printf '%s\n' "${YB_ART_VERIFIED:-<unset>}" > "$WORK/verified"
        exit "$rc"
    )
}

expect_accept() {
    local desc="$1" dir="$2" want_verified="$3" rc=0
    resolve "$dir" || rc=$?
    if [ "$rc" -ne 0 ]; then
        bad "$desc -- REFUSED (exit $rc): $(head -1 "$WORK/err")"
    elif [ "$(cat "$WORK/verified")" != "$want_verified" ]; then
        bad "$desc -- accepted, but YB_ART_VERIFIED='$(cat "$WORK/verified")', expected '$want_verified'"
    else
        ok "$desc"
    fi
}

# Assert the refusal message as well as the status so each control reaches its intended rule.
expect_refuse() {
    local desc="$1" dir="$2" want="$3" rc=0
    resolve "$dir" || rc=$?
    if [ "$rc" -eq 0 ]; then
        bad "$desc -- ACCEPTED, and should not have"
    elif ! grep -qF -- "$want" "$WORK/err"; then
        bad "$desc -- refused for the wrong reason (wanted '$want'):"
        sed 's/^/          /' "$WORK/err" >&2
    else
        ok "$desc"
    fi
}

echo "artifact-selftest: controls for kamu-money-pg/yb/artifact.sh"
echo

# --- accepted triplet --------------------------------------------------------------------------
fixture "$WORK/good"
expect_accept "a coherent, manifested triplet resolves and is marked verified" "$WORK/good" "yes"

# --- provenance --------------------------------------------------------------------------------
# Coherent names do not establish provenance; the manifest is mandatory.
fixture "$WORK/nomanifest"
rm "$WORK/nomanifest/$YB_ART_MANIFEST_NAME"
expect_refuse "a triplet with NO manifest is refused, not warned about" \
    "$WORK/nomanifest" "have no provenance"

# Set, call, then unset: a subshell would lose the pass/fail tally, while a function-prefix
# assignment persists in Bash and would arm later cases.
# shellcheck disable=SC2034 # read indirectly by expect_accept
YB_ART_ALLOW_UNVERIFIED=1
expect_accept "YB_ART_ALLOW_UNVERIFIED=1 downgrades it, and sets YB_ART_VERIFIED=no" \
    "$WORK/nomanifest" "no"
unset YB_ART_ALLOW_UNVERIFIED

fixture "$WORK/tampered"
printf 'ELF-substituted\n' > "$WORK/tampered/kmoney.so"
expect_refuse "one changed byte fails the manifest" "$WORK/tampered" "MANIFEST MISMATCH"

# --- triplet coherence -------------------------------------------------------------------------
fixture "$WORK/skew"
printf "default_version = '0.2.0'\n" > "$WORK/skew/kmoney.control"
(cd "$WORK/skew" && sha256sum kmoney.so kmoney.control kmoney--0.1.0.sql > "$YB_ART_MANIFEST_NAME")
expect_refuse "control default_version disagreeing with the script filename is refused" \
    "$WORK/skew" "INCOHERENT TRIPLET"

fixture "$WORK/ambiguous"
printf 'CREATE TYPE kmoney;\n' > "$WORK/ambiguous/kmoney--0.2.0.sql"
expect_refuse "two install scripts are ambiguous, and the harness will not choose" \
    "$WORK/ambiguous" "install scripts, so the version is ambiguous"

# --- completeness -------------------------------------------------------------------------------
fixture "$WORK/noso"; rm "$WORK/noso/kmoney.so"
expect_refuse "a missing kmoney.so is named" "$WORK/noso" "kmoney.so"

fixture "$WORK/noctl"; rm "$WORK/noctl/kmoney.control"
expect_refuse "a missing kmoney.control is named" "$WORK/noctl" "kmoney.control"

fixture "$WORK/nosql"; rm "$WORK/nosql/kmoney--0.1.0.sql"
expect_refuse "a missing install script is named" "$WORK/nosql" "kmoney--<version>.sql"

expect_refuse "a directory that does not exist is refused" "$WORK/absent" "does not exist"

# --- the ORIGINAL defect: a recursive search that could reach into a run's subdirectory ----------
# `out/` deliberately accumulates `ref/` and one subdirectory per run. A valid triplet one level
# down must be invisible, or the resolver is back to certifying whichever file the filesystem
# handed back first.
mkdir -p "$WORK/nested"
fixture "$WORK/nested/ref"
expect_refuse "a valid triplet in a SUBDIRECTORY is not found (maxdepth 1)" \
    "$WORK/nested" "missing under"

echo
if [ "$fail" -ne 0 ]; then
    printf 'artifact-selftest: \033[31m%d failed\033[0m, %d passed\n' "$fail" "$pass"
    exit 1
fi
printf 'artifact-selftest: \033[32mall %d controls passed\033[0m\n' "$pass"
