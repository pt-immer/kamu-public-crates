#!/usr/bin/env bash
# Controls for the NUMA pin verifier in kamu-money-pg/bench/numa.sh.
#
#   kamu-money-pg/bench/numa-selftest.sh
#
# Controls prove that `numa_verify` requires both CPU and memory masks.
#
# The comparison is a pure function precisely so these controls need neither a two-socket machine
# nor a container that fails in the right way. Nothing here reads /sys, /proc or docker.
#
# NO HOST TOPOLOGY IN THIS FILE. The masks below are fixtures chosen to exercise the parser -- they
# are not this machine's, and a reader should not be able to infer one from the other.
set -euo pipefail
cd "$(dirname "$0")/../.."   # repo root

# shellcheck source=kamu-money-pg/bench/numa.sh
source "$PWD/kamu-money-pg/bench/numa.sh"

pass=0
fail=0
ok()  { printf '  \033[32mok\033[0m    %s\n' "$1"; pass=$((pass + 1)); }
bad() { printf '  \033[31mFAIL\033[0m  %s\n' "$1"; fail=$((fail + 1)); }

echo "numa-selftest: controls for kamu-money-pg/bench/numa.sh"
echo

# --- canonicalisation ---------------------------------------------------------------------------
# /sys prints `0-3`, /proc may print `0-3` or `0,1,2,3`, and docker echoes whatever it was given.
# A string comparison between those reports mismatches that are not mismatches -- which trains
# whoever sees it to pass the check by loosening it.
check_expand() {
    local got; got="$(_numa_expand "$1")"
    if [ "$got" = "$2" ]; then
        ok "'$1' canonicalises to '$2'"
    else
        bad "'$1' canonicalised to '$got', expected '$2'"
    fi
}
check_expand "0-3"        "0,1,2,3"
check_expand "0,1,2,3"    "0,1,2,3"
check_expand "3,1,0,2"    "0,1,2,3"
check_expand "0-1,4-5"    "0,1,4,5"
check_expand "7"          "7"
check_expand ""           ""

# --- both masks are compared, and each mismatch is distinguishable -------------------------------
rc=0; numa_masks_agree "0-3" "1" "0,1,2,3" "1" || rc=$?
if [ "$rc" -eq 0 ]; then
    ok "identical sets written two ways agree"
else
    bad "equivalent masks were reported as a mismatch (rc=$rc)"
fi

# CPU-only mismatch.
rc=0; numa_masks_agree "0-3" "1" "8-11" "1" || rc=$?
if [ "$rc" -eq 1 ]; then
    ok "CPUs on the wrong node are refused even when the memory node is right"
elif [ "$rc" -eq 0 ]; then
    bad "a CPU-only mismatch was reported as a verified pin"
else
    bad "a CPU-only mismatch was reported as a MEMORY mismatch (rc=$rc), so the message would lie"
fi

# Memory-only mismatch.
rc=0; numa_masks_agree "0-3" "1" "0-3" "0" || rc=$?
if [ "$rc" -eq 2 ]; then
    ok "memory on the wrong node is refused even when the CPU set is right"
else
    bad "a memory-only mismatch returned $rc, expected 2"
fi

# Both masks wrong.
rc=0; numa_masks_agree "0-3" "1" "8-11" "0" || rc=$?
if [ "$rc" -ne 0 ]; then
    ok "both masks wrong is refused"
else
    bad "a container pinned to neither the requested CPUs nor its memory was verified"
fi

# A subset is not the requested node.
rc=0; numa_masks_agree "0-3" "1" "0-1" "1" || rc=$?
if [ "$rc" -eq 1 ]; then
    ok "a subset of the node's CPUs is refused, not accepted as close enough"
else
    bad "a partial CPU set was accepted (rc=$rc)"
fi

# --- unpinned stays a no-op ----------------------------------------------------------------------
# A fixture that silently pinned itself would produce numbers nobody can reproduce elsewhere, so
# unset must mean unset -- including in the line the transcript prints.
rc=0
( unset BENCH_NUMA_NODE; numa_verify "no-such-container" ) || rc=$?
if [ "$rc" -eq 0 ]; then
    ok "with BENCH_NUMA_NODE unset, numa_verify is a no-op rather than a refusal"
else
    bad "an unpinned run was refused by the pin verifier"
fi
desc="$(unset BENCH_NUMA_NODE; numa_describe)"
case "$desc" in
    *"not pinned"*) ok "numa_describe says 'not pinned' when nothing was requested" ;;
    *)              bad "numa_describe claimed a pin with no node requested: $desc" ;;
esac

echo
if [ "$fail" -ne 0 ]; then
    printf 'numa-selftest: \033[31m%d failed\033[0m, %d passed\n' "$fail" "$pass"
    exit 1
fi
printf 'numa-selftest: \033[32mall %d controls passed\033[0m\n' "$pass"
