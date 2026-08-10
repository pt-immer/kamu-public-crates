#!/usr/bin/env bash
# Refuse early unless the SELECTED buildx builder can export a cache.
#
#   bash scripts/require-cache-exporter.sh <caller>
#
# `docker buildx build` uses whichever builder is currently selected, not a container one, and only
# the docker-container driver can export a cache. Naming `buildx` in the command says nothing about
# that. On a machine whose `default` builder is the stock docker driver -- which is every machine
# where no other builder has been created -- `--cache-to type=local` aborts with a driver error
# after the context has already been sent.
#
# CI supplies a container builder through `docker/setup-buildx-action`, so this only fires for a
# developer or a non-GitHub runner who set KMONEY_BUILD_CACHE_DIR. It refuses with the command that
# fixes it, rather than leaving a BuildKit error to be decoded.
set -euo pipefail

CALLER="${1:-cache export}"

DRIVER="$(docker buildx inspect 2>/dev/null | awk -F': *' '/^Driver:/ { print $2; exit }')"

if [ "$DRIVER" = "docker-container" ]; then
    exit 0
fi

echo "$CALLER: KMONEY_BUILD_CACHE_DIR is set, so this build exports a BuildKit cache, but the" >&2
echo "$CALLER: selected buildx builder uses the '${DRIVER:-unknown}' driver, and only" >&2
echo "$CALLER: docker-container can export one." >&2
echo "$CALLER:" >&2
echo "$CALLER:   docker buildx create --name kmoney --driver docker-container --use" >&2
echo "$CALLER:" >&2
echo "$CALLER: or leave KMONEY_BUILD_CACHE_DIR unset, which is the ordinary local arrangement --" >&2
echo "$CALLER: the daemon already holds these layers." >&2
exit 2
