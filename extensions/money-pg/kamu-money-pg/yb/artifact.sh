#!/usr/bin/env bash
# Resolve the `kmoney` extension triplet in ONE directory, coherently. Sourced, not executed.
#
#   source kamu-money-pg/yb/artifact.sh
#   yb_resolve_artifacts kamu-money-pg/yb/out
#   # -> YB_ART_SO / YB_ART_CTL / YB_ART_SQL / YB_ART_VERSION / YB_ART_DIR / YB_ART_VERIFIED
#
# WHY THIS EXISTS. Every consumer used to write `find "$ART" -name 'kmoney*.so' | head -1`, three
# times, once per file. That is wrong in three independent ways, and each one can produce a green
# run against something nobody built:
#
#   1. `find` has no defined traversal order. `head -1` is therefore "whichever the filesystem
#      happened to hand back first" -- not "the newest", and not "the only".
#   2. The search was RECURSIVE over a directory that deliberately accumulates the stock-PG15
#      reference output and one subdirectory per run. A file under `ref/` or `regress-*/` was
#      always eligible.
#   3. The three searches were INDEPENDENT. Nothing tied the `.so` to the control file to the SQL
#      script, so a triplet could be assembled from two different builds -- and a mismatched
#      triplet installs cleanly. `CREATE EXTENSION` reads the control file for a version and runs
#      the script that names it; neither of them checks the shared library it binds.
#
# So: exact names, ONE directory, exactly one install script, the control file's `default_version`
# must be the version in that script's filename, and every hash in ARTIFACT-MANIFEST.txt must match
# -- which is what turns a stale decoy from a coin flip into a failure. The manifest is REQUIRED,
# not checked-if-present: see the comment at the check itself for why "coherent names" is not
# provenance. `YB_ART_ALLOW_UNVERIFIED=1` downgrades it for a developer run and says so in every
# log line, and sets `YB_ART_VERIFIED=no` so no caller can record that run as evidence by accident.
#
# `set -euo pipefail` is deliberately NOT set here: this file is sourced into scripts that already
# set it, and shell options set from a sourced file change the caller's shell.

# shellcheck disable=SC2034 # set here for the SOURCING script; unused within this file by design
YB_ART_DIR=""
# shellcheck disable=SC2034 # ditto
YB_ART_SO=""
# shellcheck disable=SC2034 # ditto
YB_ART_CTL=""
# shellcheck disable=SC2034 # ditto
YB_ART_SQL=""
# shellcheck disable=SC2034 # ditto
YB_ART_VERSION=""
# "yes" once every byte matched ARTIFACT-MANIFEST.txt; "no" only on the explicitly-requested
# non-evidence path. A caller that cares whether it is holding verified bytes reads this flag
# rather than assuming the resolve implied a check.
# shellcheck disable=SC2034 # ditto
YB_ART_VERIFIED=""

# The manifest the YB Dockerfile writes beside the artifact. It is `sha256sum`'s own format, so
# verification is `sha256sum -c` rather than a hand-rolled comparison loop.
YB_ART_MANIFEST_NAME="ARTIFACT-MANIFEST.txt"

yb_resolve_artifacts() {
    local dir="${1:-${KMONEY_RUN_ROOT:-kamu-money-pg/yb/out}}"

    if [ ! -d "$dir" ]; then
        echo "artifact: $dir does not exist -- run 'just yb-build' first" >&2
        return 2
    fi

    local so="$dir/kmoney.so"
    local ctl="$dir/kmoney.control"

    local missing=()
    [ -f "$so" ]  || missing+=("kmoney.so")
    [ -f "$ctl" ] || missing+=("kmoney.control")

    # -maxdepth 1: never a recursive search of a shared output tree. The version is the only part
    # of any name that is not known in advance, which is why this one entry is a glob at all.
    local sqls=()
    while IFS= read -r line; do
        [ -n "$line" ] && sqls+=("$line")
    done < <(find "$dir" -maxdepth 1 -type f -name 'kmoney--*.sql' | sort)

    [ "${#sqls[@]}" -eq 0 ] && missing+=("kmoney--<version>.sql")

    if [ "${#missing[@]}" -gt 0 ]; then
        echo "artifact: missing under $dir: ${missing[*]}" >&2
        echo "artifact: build it with 'just yb-build' (writes the triplet plus $YB_ART_MANIFEST_NAME)" >&2
        return 2
    fi

    # More than one install script is not a version choice this harness is entitled to make. A
    # release runs against ONE extension version, and picking silently is how a run ends up
    # certifying a version nobody asked about.
    if [ "${#sqls[@]}" -gt 1 ]; then
        echo "artifact: $dir holds ${#sqls[@]} install scripts, so the version is ambiguous:" >&2
        printf 'artifact:   %s\n' "${sqls[@]}" >&2
        echo "artifact: build into an empty directory -- this harness will not choose for you." >&2
        return 2
    fi

    local sql="${sqls[0]}"

    # The version stated by the FILENAME and the version stated INSIDE the control file must be the
    # same string. `CREATE EXTENSION` uses default_version to decide which script to run, so a
    # disagreement means the script that runs is not the script this harness checked.
    local file_version control_version
    file_version="$(basename "$sql")"
    file_version="${file_version#kmoney--}"
    file_version="${file_version%.sql}"
    control_version="$(sed -n "s/^[[:space:]]*default_version[[:space:]]*=[[:space:]]*'\([^']*\)'.*/\1/p" "$ctl" | head -1)"

    if [ -z "$control_version" ]; then
        echo "artifact: $ctl declares no default_version" >&2
        return 2
    fi
    if [ "$control_version" != "$file_version" ]; then
        echo "artifact: INCOHERENT TRIPLET -- the control file and the install script disagree." >&2
        echo "artifact:   $ctl declares default_version = '$control_version'" >&2
        echo "artifact:   $sql is version '$file_version'" >&2
        echo "artifact: these came from different builds. Rebuild into an empty directory." >&2
        return 2
    fi

    # MANDATORY. This warned and carried on until 2026-07-26, which meant the same call could
    # return release evidence or unverified bytes depending on how the directory came to exist --
    # and the caller could not tell, because the difference was one line on stderr in a log nobody
    # reads on the happy path. Coherent NAMES are not provenance: names and versions match by
    # construction for any triplet copied out of any build, including a stale one left in `out/`
    # by a previous revision, which is precisely the substitution the manifest exists to catch.
    #
    # The escape hatch is loud, must be asked for by name, and MARKS THE RESULT. A gate that can be
    # silently downgraded is not a gate; one that can be downgraded only by an environment variable
    # that then stamps "UNVERIFIED" through the run is a convenience with a receipt.
    local manifest="$dir/$YB_ART_MANIFEST_NAME"
    YB_ART_VERIFIED="no"
    if [ -f "$manifest" ]; then
        if ! (cd "$dir" && sha256sum --quiet -c "$YB_ART_MANIFEST_NAME") >/dev/null 2>&1; then
            echo "artifact: MANIFEST MISMATCH under $dir -- these are not the files the build produced." >&2
            (cd "$dir" && sha256sum -c "$YB_ART_MANIFEST_NAME") >&2 || true
            return 2
        fi
        YB_ART_VERIFIED="yes"
        echo "artifact: $dir verified against $YB_ART_MANIFEST_NAME (kmoney $control_version)"
    elif [ "${YB_ART_ALLOW_UNVERIFIED:-0}" = "1" ]; then
        echo "artifact: *** UNVERIFIED ARTIFACT ($dir has no $YB_ART_MANIFEST_NAME) ***" >&2
        echo "artifact: *** proceeding only because YB_ART_ALLOW_UNVERIFIED=1. This run is NOT ***" >&2
        echo "artifact: *** release evidence and must not be recorded as any.                 ***" >&2
    else
        echo "artifact: $dir has no $YB_ART_MANIFEST_NAME -- these bytes have no provenance." >&2
        echo "artifact: names and version are coherent, which proves nothing: a stale triplet left" >&2
        echo "artifact: by an earlier revision is coherent too. The manifest is what ties these" >&2
        echo "artifact: exact bytes to the build that produced them." >&2
        echo "artifact:" >&2
        echo "artifact:   rebuild (writes the manifest):  just yb-build" >&2
        echo "artifact:   or, for a NON-EVIDENCE run:     YB_ART_ALLOW_UNVERIFIED=1 <command>" >&2
        return 2
    fi

    YB_ART_DIR="$dir"
    YB_ART_SO="$so"
    YB_ART_CTL="$ctl"
    YB_ART_SQL="$sql"
    YB_ART_VERSION="$control_version"
}
