#!/usr/bin/env bash
# Controls for the buildx cache-export pre-flight check.
#
#   scripts/require-cache-exporter-selftest.sh
#
# The check exists to turn a mid-build BuildKit driver error into a diagnostic with the remedy. Its
# first version did the opposite: `DRIVER="$(docker buildx inspect | awk ...)"` under
# `set -euo pipefail` aborted AT THE ASSIGNMENT when the probe failed, so every diagnostic below it
# was unreachable and the caller saw a bare non-zero exit. That failed a green YugabyteDB CI job
# whose builder simply was not answering yet, while the four PostgreSQL jobs -- same check, same
# workflow, same commit -- passed.
#
# So the property under test is not only "does it refuse the wrong driver" but "what does it do
# when it CANNOT TELL". `docker` is stubbed, so none of this needs a daemon or a builder.
set -euo pipefail
cd "$(dirname "$0")/.." # lane root

# Invoked through `bash` exactly as the callers do, so this exercises the real entry point rather
# than a more forgiving one.
CHECK="bash ./scripts/require-cache-exporter.sh"

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

# Install a `docker` that runs `body` and exits `code`, then invoke the check the way its callers
# do -- under `set -euo pipefail`, which is what made the original failure silent.
expect() {
    local label="$1" want_status="$2" want_text="$3" body="$4" code="$5" status
    mkdir -p "$WORK/bin"
    {
        printf '#!/bin/sh\n'
        printf '%s\n' "$body"
        printf 'exit %s\n' "$code"
    } > "$WORK/bin/docker"
    chmod +x "$WORK/bin/docker"

    PATH="$WORK/bin:$PATH" bash -c "set -euo pipefail; $CHECK selftest" \
        >"$WORK/out" 2>"$WORK/err" && status=0 || status=$?

    if [ "$status" -ne "$want_status" ]; then
        bad "$label (exit $status, wanted $want_status): $(cat "$WORK/err")"
    elif [ -n "$want_text" ] && ! grep -q "$want_text" "$WORK/err"; then
        bad "$label (stderr did not mention '$want_text': $(cat "$WORK/err"))"
    else
        ok "$label"
    fi
}

echo "require-cache-exporter-selftest: controls for the buildx driver pre-flight"

# THE ONE THAT BROKE CI. A probe that cannot answer must not be the thing that fails the build, and
# it must report what it saw rather than exiting mute.
expect "an unreachable daemon proceeds rather than failing the build" 0 "could not inspect" \
    'echo "cannot connect to the Docker daemon" >&2' 1
expect "an unreachable daemon repeats what the probe said" 0 "cannot connect" \
    'echo "cannot connect to the Docker daemon" >&2' 1
expect "output naming no driver proceeds" 0 "names no driver" \
    'echo "Name: builder"' 0

# The case the check exists for.
expect "the stock docker driver is refused, with the remedy" 2 "docker buildx create" \
    'printf "Name: default\nDriver: docker\n"' 0

# Drivers that can export a cache must pass silently.
for driver in docker-container kubernetes remote; do
    expect "the $driver driver is accepted" 0 "" \
        "printf 'Name: b\nDriver: $driver\n'" 0
done

echo
if [ "$fail" -ne 0 ]; then
    printf 'require-cache-exporter-selftest: \033[31m%d failed\033[0m, %d passed\n' "$fail" "$pass"
    exit 1
fi
printf 'require-cache-exporter-selftest: \033[32mall %d controls passed\033[0m\n' "$pass"
