#!/usr/bin/env bash
# Mutual-exclusion, re-entrancy, and lifecycle controls for workspace-lock.sh.
#
#   kamu-money-pg/yb/workspace-lock-selftest.sh
#
# No Docker or database is required.
set -euo pipefail
cd "$(dirname "$0")/../.."   # repo root

LOCKLIB="$PWD/kamu-money-pg/yb/workspace-lock.sh"
WORK="$(mktemp -d)"
cleanup() { rm -rf "$WORK"; return 0; }
trap cleanup EXIT INT TERM HUP

# Use a fixture lock directory and discard any inherited lock descriptor so this suite tests its
# own first acquisition and contention.
export KMONEY_LOCK_DIR="$WORK/lockdir"
unset KMONEY_WORKSPACE_LOCK_FD
mkdir -p "$KMONEY_LOCK_DIR"

pass=0
fail=0
ok()  { printf '  \033[32mok\033[0m    %s\n' "$1"; pass=$((pass + 1)); }
bad() { printf '  \033[31mFAIL\033[0m  %s\n' "$1"; fail=$((fail + 1)); }

echo "workspace-lock-selftest: controls for kamu-money-pg/yb/workspace-lock.sh"
echo

# Holder process used by exclusion and inheritance controls.
cat > "$WORK/holder.sh" <<EOF
#!/usr/bin/env bash
set -euo pipefail
source "$LOCKLIB"
workspace_lock "selftest-holder" || exit 1
# Its own pid, recorded by itself. Under setsid this is also the process GROUP id, and \$! in the
# caller would be setsid's pid rather than this one.
echo \$\$ > "$WORK/pgid"
# A real descendant inherits the holder's open descriptor, matching a suite started by
# "release-check". The re-entrancy control reads this child's result.
rc=0
bash -c "source '$LOCKLIB'; workspace_lock 'selftest-real-child'" >/dev/null 2>&1 || rc=\$?
echo "\$rc" > "$WORK/child.rc"
echo ready > "$WORK/ready"
# A child inherits the descriptor, making process-versus-group termination observable.
sleep 30
EOF
chmod +x "$WORK/holder.sh"

# `setsid` so the holder leads its own process group and the group kill below has something to
# aim at. Without job control -- and this script is not interactive -- a plain `&` would leave it
# in this script's group, where a group kill would take the self-test down with it.
setsid "$WORK/holder.sh" &
# Wait for the lock to actually be held rather than assuming a sleep is long enough.
for _ in $(seq 1 100); do [ -f "$WORK/ready" ] && break; sleep 0.1; done
HOLDER_PID="$(cat "$WORK/pgid" 2>/dev/null || echo 0)"

if [ ! -f "$WORK/ready" ]; then
    bad "the first caller never acquired the lock at all"
else
    ok "the first caller acquires the lock"

    # --- exclusion --------------------------------------------------------------------------
    if bash -c "source '$LOCKLIB'; workspace_lock 'selftest-second'" > "$WORK/out" 2> "$WORK/err"; then
        bad "a SECOND caller acquired a lock the first one is holding"
    elif grep -q REFUSING "$WORK/err"; then
        ok "a second caller is refused while the first still holds it"
    else
        bad "the second caller failed, but not with a refusal a human can act on"
    fi

    # The refusal identifies the holder and PID.
    if grep -q selftest-holder "$WORK/err" && grep -q "$HOLDER_PID" "$WORK/err"; then
        ok "the refusal names what holds the lock, and its pid"
    else
        bad "the refusal did not identify the holder: $(tr '\n' ' ' < "$WORK/err" | head -c 160)"
    fi

    # --- inherited re-entrancy ----------------------------------------------------------------
    # `release-check` children inherit the real descriptor and must not deadlock on their parent.
    for _ in $(seq 1 100); do [ -f "$WORK/child.rc" ] && break; sleep 0.1; done
    if [ "$(cat "$WORK/child.rc" 2>/dev/null || echo missing)" = 0 ]; then
        ok "a real descendant of the holder proceeds instead of deadlocking on its own parent"
    else
        bad "an actual child of the holder was refused, so release-check would block on its own suites"
    fi

    # --- forged descriptors -------------------------------------------------------------------
    # An unrelated process cannot claim inheritance through an environment variable alone.
    if KMONEY_WORKSPACE_LOCK_FD=1 \
       bash -c "source '$LOCKLIB'; workspace_lock 'selftest-forged'" >/dev/null 2>"$WORK/forged.err"; then
        bad "a process with no lock acquired one by setting KMONEY_WORKSPACE_LOCK_FD by hand"
    elif grep -q 'not an open handle' "$WORK/forged.err"; then
        ok "a forged KMONEY_WORKSPACE_LOCK_FD is refused -- the descriptor is checked, not the claim"
    else
        bad "the forged variable was refused for some other reason: $(tr '\n' ' ' < "$WORK/forged.err" | head -c 160)"
    fi

    # A descriptor that IS open, on the wrong file. Catches a check that only asks "is fd N open?"
    # -- fd 2 always is.
    if KMONEY_WORKSPACE_LOCK_FD=2 \
       bash -c "source '$LOCKLIB'; workspace_lock 'selftest-wrongfd'" >/dev/null 2>/dev/null; then
        bad "an open descriptor on an unrelated file was accepted as the workspace lock"
    else
        ok "an open descriptor pointing somewhere else is refused, not merely a closed one"
    fi

    # --- public entry points ------------------------------------------------------------------
    # Every writer must refuse before changing shared state. Stubs record any accidental
    # docker/cargo call without starting expensive work.
    mkdir -p "$WORK/bin"
    for tool in docker cargo; do
        printf '#!/bin/sh\nprintf "%%s %%s\\n" "%s" "$*" >> "%s"\nexit 1\n' \
            "$tool" "$WORK/tool.calls" > "$WORK/bin/$tool"
        chmod +x "$WORK/bin/$tool"
    done
    : > "$WORK/tool.calls"

    # Route all artifact defaults into the fixture tree.
    export KMONEY_RUN_ROOT="$WORK/shared"
    SHARED="$KMONEY_RUN_ROOT"
    mkdir -p "$SHARED"
    # A canary, so the snapshot has something to notice even though the fixture starts empty.
    CANARY="$SHARED/.workspace-lock-selftest-canary"
    printf 'written by workspace-lock-selftest; safe to delete\n' > "$CANARY"
    # Names, sizes and mtimes -- enough to see a write, cheap enough to run per entry point even
    # when out/ holds a release log and a 20 MB .so.
    snapshot() { find "$SHARED" -type f -printf '%p %s %T@\n' 2>/dev/null | sort; }
    BEFORE_SHARED="$(snapshot)"

    # Every public way into the shared paths. Private `_yb-ab-ref` is included because
    # `release-check` calls it directly, so it is an entry point in practice.
    ENTRIES=(
        "just yb-build"
        "just yb-native"
        "just yb-ab"
        "just _yb-ab-ref sha256:0000000000000000000000000000000000000000000000000000000000000000"
        "./kamu-money-pg/yb/run-yb.sh"
        "./kamu-money-pg/yb/run-yb-regress.sh"
        "./kamu-money-pg/yb/run-yb-cluster.sh"
        "./kamu-money-pg/yb/run-yb-concurrent.sh"
        "./kamu-money-pg/yb/run-yb-readreplica.sh"
        "./kamu-money-pg/yb/run-yb-restore.sh"
        "./kamu-money-pg/yb/run-yb-resilience.sh"
        "./kamu-money-pg/yb/run-yb-soak.sh"
        "./kamu-money-pg/yb/run-yb-bench.sh"
        "./kamu-money-pg/yb/assert-battery-selftest.sh"
        "./kamu-money-pg/bench/run-bench-pg.sh"
        "./kamu-money-pg/bench/run-bench-boundary.sh"
        "./kamu-money-pg/bench/run-bench-sql-yb.sh"
        "./kamu-money-pg/bench/run-bench-boundary-yb.sh"
    )
    entry_fail=0
    for entry in "${ENTRIES[@]}"; do
        erc=0
        PATH="$WORK/bin:$PATH" timeout 60 bash -c "$entry" >/dev/null 2>"$WORK/entry.err" || erc=$?
        if [ "$erc" -eq 0 ]; then
            bad "'$entry' RAN TO COMPLETION while another run held the workspace lock"
            entry_fail=1
        elif [ "$(snapshot)" != "$BEFORE_SHARED" ]; then
            bad "'$entry' changed shared state before being refused"
            entry_fail=1
            BEFORE_SHARED="$(snapshot)"
        fi
    done
    if [ -s "$WORK/tool.calls" ]; then
        bad "an entry point reached docker/cargo despite the lock: $(head -1 "$WORK/tool.calls")"
        entry_fail=1
    fi
    if [ "$entry_fail" -eq 0 ]; then
        ok "all ${#ENTRIES[@]} public entry points refuse, and none touches shared state first"
    fi
    rm -f "$CANARY"
fi

# --- release on death -------------------------------------------------------------------------
# The kernel releases the lock when the last inherited descriptor closes. Killing only the
# parent must not free it while a child still writes; killing the whole group must free it.
# No `wait`: setsid detached the holder, so it is not a child of this shell. Poll for its death.
kill "$HOLDER_PID" 2>/dev/null || true
for _ in $(seq 1 100); do kill -0 "$HOLDER_PID" 2>/dev/null || break; sleep 0.1; done
if bash -c "source '$LOCKLIB'; workspace_lock 'selftest-orphan'" 2>/dev/null; then
    bad "the lock freed while a child of the dead holder was still running and still writing"
else
    ok "killing only the top-level holder does not free the lock while its children live"
fi

# The whole process group, which is what a Ctrl-C or a runner teardown actually sends.
kill -- -"$HOLDER_PID" 2>/dev/null || true
for _ in $(seq 1 100); do
    bash -c "source '$LOCKLIB'; workspace_lock 'selftest-probe'" 2>/dev/null && break
    sleep 0.1
done
if bash -c "source '$LOCKLIB'; workspace_lock 'selftest-after'" 2>/dev/null; then
    ok "once the whole group is gone the lock is free -- no stale file wedges the workspace"
else
    bad "the lock survived its entire process group, so a killed run wedges the checkout"
fi

echo
if [ "$fail" -ne 0 ]; then
    printf 'workspace-lock-selftest: \033[31m%d failed\033[0m, %d passed\n' "$fail" "$pass"
    exit 1
fi
printf 'workspace-lock-selftest: \033[32mall %d controls passed\033[0m\n' "$pass"
