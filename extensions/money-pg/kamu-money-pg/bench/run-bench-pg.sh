#!/usr/bin/env bash
# Run kamu-money-pg/bench/sql-cost.sql on a stock PostgreSQL with `kmoney` installed. NEVER a gate.
#
#   kamu-money-pg/bench/run-bench-pg.sh [major]      # default 18
#
# WHY THIS EXISTS. E20's SQL figures were measured with code that was deliberately discarded,
# which left `specs.md` §1 saying "reproduce before trusting" directly above an entry nobody could
# reproduce. This is the fixture that closes that. It reports; it never fails on a number.
#
# NO THRESHOLD, ON PURPOSE -- the same rule `just bench-yb` states: a limit invented before there
# is a baseline to regress against either never fires or fires on somebody else's hardware. If
# these are ever to gate, a baseline on known hardware comes first, and that is a decision.
#
# THE OUTPUT IS THE ARTEFACT. Environment identity is printed WITH the numbers, because a timing
# without the machine it came from is not reproducible even in principle, and the host is the one
# piece of context that cannot be recovered from the repository later.
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

# shellcheck source=kamu-money-pg/bench/numa.sh
source "$(dirname "$0")/numa.sh"
# Empty when unpinned; the `+` form keeps `set -u` happy with an empty array.
read -r -a NUMA_ARGS <<< "$(numa_docker_args)"

PG="${1:-18}"
# PID AND ENTROPY AS WELL AS THE TIMESTAMP. Second resolution alone means two runs started in the
# same second overwrite one transcript, and the workspace lock only serialises the YugabyteDB
# runners -- stock-PostgreSQL runs can genuinely collide. The release directory already names
# itself this way, for the same reason: the file name is how a reader tells two runs apart.
OUT="kamu-money-pg/yb/out/bench-pg${PG}-$(date -u +%Y%m%dT%H%M%SZ)-$$-$(od -An -N2 -tx1 /dev/urandom | tr -d ' \n').log"
mkdir -p "$(dirname "$OUT")"

RUN_ID="kmoney-bench-$$-$(od -An -N4 -tx4 /dev/urandom | tr -d ' \n')"
NAME="$RUN_ID"
# The daemon is shared across several organisations' runners: this container belongs to this
# script, by label, and the trap covers the signals as well as EXIT.
cleanup() { docker rm -f "$NAME" >/dev/null 2>&1 || true; return 0; }
trap cleanup EXIT INT TERM HUP

# Built by the ordinary test matrix, so the extension under measurement is the one the tests ran.
#
# RESOLVED TO AN IMAGE ID ONCE, AND THE ID IS WHAT RUNS. The tag is mutable and this daemon is
# shared across several organisations' runners: this script used to `docker image inspect` the tag
# for the ID it printed into the log and then `docker run` the TAG, so a retag between those two
# lines would label a measurement with one image's ID while timing another's code. Even without a
# race it never checked that the image was built from the source it was about to name.
# `test-matrix.sh` already fixes exactly this class with `--iidfile` and runs the ID.
TAG="kamu-money-pg:pg${PG}"
if ! IMAGE="$(docker image inspect --format '{{ .Id }}' "$TAG" 2>/dev/null)"; then
    echo "bench-pg: $TAG is not built. Run 'just test-pg $PG' first -- this measures the same" >&2
    echo "bench-pg: image the test matrix builds, so the numbers describe tested code." >&2
    exit 2
fi

# AND THE IMAGE MUST BE THIS SOURCE. `test-matrix.sh` stamps every image it builds with
# `kamu-money-pg.revision`. Without this check the log's `source sha` line is the sha of whatever
# is checked out NOW, attached to timings from whenever that image happened to be built -- which
# is a provenance claim the file cannot support, in the fixture that exists because E20's figures
# could not be reproduced.
REVISION="$(git rev-parse HEAD 2>/dev/null || echo UNKNOWN)"
# `--short`, because that is what `test-matrix.sh` stamps. Comparing against the full sha made
# this refuse EVERY image on its first run -- caught by exercising it rather than by reading it,
# which is the argument for exercising it.
REVISION_SHORT="$(git rev-parse --short HEAD 2>/dev/null || echo nogit)"
IMAGE_REVISION="$(docker image inspect --format '{{ index .Config.Labels "kamu-money-pg.revision" }}' "$IMAGE" 2>/dev/null || true)"
if [ "$IMAGE_REVISION" != "$REVISION_SHORT" ]; then
    echo "bench-pg: REFUSING -- $TAG was built from a different revision." >&2
    echo "bench-pg:   image built from  ${IMAGE_REVISION:-<unlabelled>}" >&2
    echo "bench-pg:   checkout is at    $REVISION_SHORT" >&2
    echo "bench-pg: the log would name this revision beside timings from that one. Rebuild with:" >&2
    echo "bench-pg:   just test-pg $PG" >&2
    echo "bench-pg: (set BENCH_ALLOW_STALE_IMAGE=1 to measure the old image deliberately; the" >&2
    echo "bench-pg:  log then records which revision was actually measured.)" >&2
    [ "${BENCH_ALLOW_STALE_IMAGE:-0}" = "1" ] || exit 2
    echo "bench-pg: proceeding anyway (BENCH_ALLOW_STALE_IMAGE=1)" >&2
fi

{
    echo "kmoney SQL cost benchmark -- NOT A GATE, no pass/fail threshold"
    echo
    echo "  measured at    $(date -u +%Y-%m-%dT%H:%M:%SZ)"
    echo "  source sha     $REVISION"
    echo "  tree dirty     $([ -n "$(git status --porcelain -- . 2>/dev/null)" ] && echo yes || echo no)"
    echo "  image tag      $TAG (a mutable convenience name; the ID below is what ran)"
    echo "  image id       $IMAGE"
    # Printed even though it is checked above, because the check has a deliberate override and a
    # reader of the log should not have to know whether it was used.
    echo "  image built at ${IMAGE_REVISION:-<unlabelled>}"
    # A STABLE, NON-IDENTIFYING HOST FINGERPRINT. What a reader of two transcripts needs is
    # "was this the same machine?", never "which machine was it?" -- and a CPU model plus kernel
    # plus core count is a fingerprint of somebody's infrastructure, which does not belong in an
    # artefact that gets pasted into a design record. The digest answers the comparability
    # question and answers nothing else.
    echo "  host id        $(printf '%s|%s|%s' "$(uname -sm)" "$(grep -m1 'model name' /proc/cpuinfo 2>/dev/null | cut -d: -f2-)" "$(nproc 2>/dev/null)" | sha256sum | cut -c1-12)"
    echo "  host cores     $(nproc 2>/dev/null || echo unknown)"
    echo "  load average   $(cut -d' ' -f1-3 /proc/loadavg 2>/dev/null || echo unknown)"
    echo "  numa           $(numa_describe)"
    echo
    echo "  A LOADED HOST INFLATES EVERY ROW. If the load average above is not near zero, treat"
    echo "  these as an ordering rather than as magnitudes, and re-run on a quiet machine before"
    echo "  quoting any single figure."
    echo
} | tee "$OUT"

# The image's CMD is `cargo pgrx test`, which builds, runs the suite and tears its server down --
# so it is started idle here and driven by exec instead. `in-container.sh` does the same install
# the test matrix performs and then leaves a server up.
docker run -d --name "$NAME" --label "kamu-money-pg.bench=$RUN_ID" \
    ${NUMA_ARGS[@]+"${NUMA_ARGS[@]}"} \
    --entrypoint /bin/sh "$IMAGE" -c 'sleep infinity' >/dev/null

# THE PIN IS A CLAIM UNTIL THIS RUNS. docker stores `--cpuset-cpus` faithfully and the kernel
# may discard it: cgroup v2 intersects a child cpuset with its parent slice's effective set, so
# under a system.slice confined to one node the request silently resolves to the OTHER socket.
# Measured on this host. Refuse rather than measure one socket and label it the other.
numa_verify "$NAME" || exit 4

docker cp kamu-money-pg/bench/sql-cost.sql    "$NAME:/tmp/sql-cost.sql"
docker cp kamu-money-pg/bench/in-container.sh "$NAME:/tmp/in-container.sh"

echo "bench-pg: installing the extension (same build the test matrix runs) ..." | tee -a "$OUT"
# Streams merged INSIDE the container: `docker exec` multiplexes stdout and stderr separately, so
# a host-side `2>&1` cannot order them -- and this output is read as a table.
docker exec "$NAME" bash -c 'exec bash /tmp/in-container.sh "$1" 2>&1' bench "$PG" \
    | tee -a "$OUT"

echo | tee -a "$OUT"
echo "bench-pg: full output retained at $OUT" | tee -a "$OUT"
