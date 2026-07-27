#!/usr/bin/env bash
# Resolve the YugabyteDB image TAG to an IMMUTABLE reference, check it against the pinned digest,
# and print it on stdout. Everything else goes to stderr, because every caller consumes this as
# `YB_REF="$(yb-image.sh ...)"`.
#
# Why this exists: a tag is mutable. `yugabytedb/yugabyte:2025.2.5.1-b1` can be repointed at a
# new build at any time, and this repo runs against a SHARED docker daemon. The native YB proof
# is a claim about ONE image -- the artifact is compiled against that image's PG15 headers and
# glibc, and then loaded into that image's runtime -- so the artifact build, the stock-PG15
# reference build and the live runtime must all name the SAME identity. Resolving the tag three
# separate times can silently straddle a retag, which would make "byte-exact" a statement about
# no particular image.
#
# kamu-money-pg/test-matrix.sh already applies exactly this rule to the ordinary PG matrix: it builds
# with --iidfile and RUNS THE IMAGE ID, not the tag, so a concurrent retag cannot change what was
# tested. This is that rule for the YugabyteDB path.
#
# AND THE PIN. Resolving to a digest makes the identity stable within one run; it does not make it
# the identity anyone VALIDATED. YB-PINNED.txt records the digest the pgrx fork was derived and
# validated against.
#
# THIS FAILS CLOSED, IN BOTH DIRECTIONS. It used to refuse a tag that had MOVED OFF its pin while
# merely warning about a tag that had no pin at all -- so the one case where NOTHING had ever been
# validated was the one case that proceeded. The reasoning was that the two situations differ, and
# they do; what does not differ is that neither image is a validated one. Both refuse now, with
# different overrides, so that "which check did I just bypass?" has an answer:
#
#   Usage: yb-image.sh [tag]                 -> prints e.g. yugabytedb/yugabyte@sha256:...
#     YB_ALLOW_DRIFT=1     the tag IS pinned but has moved off the pinned digest -> warn, proceed
#     YB_ALLOW_UNPINNED=1  the tag has NO entry in YB-PINNED.txt at all          -> warn, proceed
#     YB_PULL=1            re-resolve from the REGISTRY instead of trusting the local cache
#     YB_PINFILE=<path>    use a different pin file (the self-test drives a fixture through it)
#
# `YB_ALLOW_DRIFT=1` is RUNBOOK.md §2's adoption path for a moved build of a known tag.
# `YB_ALLOW_UNPINNED=1` is the adoption path for a tag this repo has never seen. Two variables on
# purpose: an operator who set one should not silently get the other.
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
TAG="${1:-yugabytedb/yugabyte:2025.2.5.1-b1}"
PINFILE="${YB_PINFILE:-$HERE/YB-PINNED.txt}"

# TWO DIFFERENT QUESTIONS, AND THEY USED TO HAVE ONE ANSWER.
#   "is the image I already have the validated one?" -- cheap, offline, asked by every yb-* run
#   "has the tag moved in the registry since?"       -- a network round trip, an adoption question
# The old code pulled only when the tag was absent locally, and `just yb-pin-check` then reported
# the result as what the tag "resolves to now" -- a registry claim answered from a cache. Ask
# explicitly instead.
if [ "${YB_PULL:-0}" = "1" ]; then
    echo "yb-image: pulling $TAG to re-resolve it from the registry" >&2
    docker pull -q "$TAG" >/dev/null
elif ! docker image inspect "$TAG" >/dev/null 2>&1; then
    # Absent locally: there is no cache to trust, so this pull is not a registry question answered
    # from stale data -- it is the only way to have the image at all.
    echo "yb-image: $TAG is not present locally; pulling it once" >&2
    docker pull -q "$TAG" >/dev/null
else
    echo "yb-image: using the LOCAL $TAG (YB_PULL=1 re-resolves it from the registry)" >&2
fi

# RepoDigests is the registry-side immutable name, which is what a rebuild elsewhere can also
# resolve. Prefer it so the recorded identity is meaningful off this machine.
DIGEST="$(docker image inspect --format '{{ index .RepoDigests 0 }}' "$TAG" 2>/dev/null || true)"

if [ -z "$DIGEST" ] || [ "$DIGEST" = "<no value>" ]; then
    # A locally built/imported image has no RepoDigest. The image ID is equally immutable but
    # local-only, so say so on stderr rather than silently falling back to the mutable tag --
    # a reader of the evidence needs to know which kind of identity they are looking at.
    DIGEST="$(docker image inspect --format '{{ .Id }}' "$TAG")"
    echo "yb-image: $TAG has no RepoDigest (local image); using image ID $DIGEST" >&2
fi

# --- the pin check -----------------------------------------------------------------------------
# `if`, never `[ ... ] && printf`: under `set -e` a false test makes the whole && chain return 1,
# which would abort this function partway through the message it exists to print.
refuse() {
    local why="$1" validated="$2" advice="$3"
    {
        printf 'yb-image: REFUSING -- %s\n\n' "$why"
        printf '  tag        %s\n' "$TAG"
        printf '  now        %s\n' "$DIGEST"
        if [ -n "$validated" ]; then
            printf '  validated  %s\n' "$validated"
        fi
        printf '\n%s\n\n' "$advice"
        printf 'The procedure is written down in kamu-money-pg/yb/RUNBOOK.md.\n'
    } >&2
    exit 1
}

if [ ! -f "$PINFILE" ]; then
    if [ "${YB_ALLOW_UNPINNED:-0}" = "1" ]; then
        echo "yb-image: NO PIN FILE, ALLOWED (YB_ALLOW_UNPINNED=1). $PINFILE does not exist," >&2
        echo "yb-image:   so nothing here records what was validated. Bootstrap one per RUNBOOK §2." >&2
        echo "$DIGEST"
        exit 0
    fi
    refuse "there is no pin file at all." "" \
"$PINFILE does not exist, so nothing in this checkout records which YugabyteDB image the pgrx
fork was validated against. Restore it, or set YB_ALLOW_UNPINNED=1 if you are deliberately
bootstrapping a new one."
fi

PINNED="$(grep -v '^[[:space:]]*#' "$PINFILE" | grep -F "$TAG	" | head -1 | cut -f2 || true)"

if [ -z "$PINNED" ]; then
    if [ "${YB_ALLOW_UNPINNED:-0}" = "1" ]; then
        echo "yb-image: UNPINNED TAG ALLOWED (YB_ALLOW_UNPINNED=1). $TAG has no entry in" >&2
        echo "yb-image:   $PINFILE -- nothing has validated the adaptation against it." >&2
        echo "yb-image: follow yb/RUNBOOK.md §2 and record a pin before this run counts as evidence." >&2
    else
        refuse "the YugabyteDB tag is not recorded in the pin file." "" \
"Nothing has validated the pgrx fork against this image. An unvalidated image can COMPILE and
still be the wrong adaptation, which is the failure this pin exists to catch -- so an unknown
tag is refused for the same reason a moved one is.

To adopt it deliberately:  YB_ALLOW_UNPINNED=1 just yb-ab $TAG"
    fi
elif [ "$PINNED" != "$DIGEST" ]; then
    if [ "${YB_ALLOW_DRIFT:-0}" = "1" ]; then
        echo "yb-image: DRIFT ALLOWED (YB_ALLOW_DRIFT=1). $TAG now resolves to" >&2
        echo "yb-image:   $DIGEST" >&2
        echo "yb-image: but the adaptation was validated against" >&2
        echo "yb-image:   $PINNED" >&2
        echo "yb-image: follow yb/RUNBOOK.md before recording a new pin." >&2
    else
        refuse "the YugabyteDB tag has moved off the validated digest." "$PINNED" \
"The adaptations are facts about YugabyteDB's headers, and one that no longer matches can still
COMPILE -- it is then silently the wrong adaptation. So a new image is adopted deliberately,
never by a docker pull.

To run against the new image without recording it yet:  YB_ALLOW_DRIFT=1 just yb-ab $TAG"
    fi
else
    echo "yb-image: $TAG is at the validated digest" >&2
fi

echo "$DIGEST"
