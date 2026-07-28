#!/usr/bin/env bash
# Controls for workspace-lock.sh -- the thing that stops two release checks from writing each
# other's evidence.
#
#   kamu-money-pg/yb/workspace-lock-selftest.sh
#
# WHY THIS EXISTS. A lock whose mutual exclusion has been checked once, by hand, is a lock that
# works until someone changes how it is sourced. Both properties here are easy to break in a way
# that LOOKS fine: a lock that never excludes lets two runs silently overwrite each other's
# artifacts, so a suite reads bytes some other run built, and a lock that is not re-entrant
# deadlocks `release-check` against its own suites -- which reads as a hung gate rather than as a
# bug in the lock.
#
# NO DOCKER AND NO DATABASE, so it stays in `just check`.
set -euo pipefail
cd "$(dirname "$0")/../.."   # repo root

LOCKLIB="$PWD/kamu-money-pg/yb/workspace-lock.sh"
WORK="$(mktemp -d)"
cleanup() { rm -rf "$WORK"; return 0; }
trap cleanup EXIT INT TERM HUP

# HERMETIC, IN BOTH DIRECTIONS, AND IT WAS NEITHER.
#
# `KMONEY_LOCK_DIR` points every caller below at a fixture directory instead of the workspace's
# real lock. Without it this self-test CONTENDS FOR THE LOCK IT IS TESTING: run on its own it
# passes, run nested inside anything that legitimately holds the lock it fails, because the
# "first caller acquires" control cannot acquire.
#
# `KMONEY_WORKSPACE_LOCK_FD` must be unset for the same reason from the other side. It is exported
# by whoever already holds the lock, and it makes `workspace_lock` return early -- correct in
# production, fatal here, because the exclusion controls would then measure a function that
# returns 0 without contending and report that a second caller acquired a held lock. (It would
# now be REFUSED rather than honoured, since the descriptor would not be open in these children --
# but a control that passes for the wrong reason is still not a control.)
#
# Measured 2026-07-26: `release-check` runs `check-all`, which runs this. Three of six controls
# failed, `check-all` exited non-zero, and the gate's own `set -e` defect let the run continue
# and seal a PASS. A self-test that only works when run alone is not a self-test.
export KMONEY_LOCK_DIR="$WORK/lockdir"
unset KMONEY_WORKSPACE_LOCK_FD
mkdir -p "$KMONEY_LOCK_DIR"

pass=0
fail=0
ok()  { printf '  \033[32mok\033[0m    %s\n' "$1"; pass=$((pass + 1)); }
bad() { printf '  \033[31mFAIL\033[0m  %s\n' "$1"; fail=$((fail + 1)); }

echo "workspace-lock-selftest: controls for kamu-money-pg/yb/workspace-lock.sh"
echo

# A holder that acquires, announces, and stays alive long enough to be contended with.
cat > "$WORK/holder.sh" <<EOF
#!/usr/bin/env bash
set -euo pipefail
source "$LOCKLIB"
workspace_lock "selftest-holder" || exit 1
# Its own pid, recorded by itself. Under setsid this is also the process GROUP id, and \$! in the
# caller would be setsid's pid rather than this one.
echo \$\$ > "$WORK/pgid"
# A REAL DESCENDANT, taking the lock the way a suite started by \`release-check\` does: started by
# the holder, so it inherits the open descriptor. The re-entrancy control reads this file rather
# than starting an unrelated shell with an environment variable set, which is what it used to do
# and which proved nothing.
rc=0
bash -c "source '$LOCKLIB'; workspace_lock 'selftest-real-child'" >/dev/null 2>&1 || rc=\$?
echo "\$rc" > "$WORK/child.rc"
echo ready > "$WORK/ready"
# A CHILD, on purpose: it inherits the lock descriptor, which is what makes the difference
# between killing the script and killing the group observable below.
# Released by exit, which closes the descriptor. Never by an explicit unlock: an explicit one is
# skipped when the process is killed, which is exactly when a stale lock hurts most.
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

    # The refusal must NAME the holder. "Something else is running" sends the reader to `ps`;
    # the pid and the start time answer the question they are about to ask.
    if grep -q selftest-holder "$WORK/err" && grep -q "$HOLDER_PID" "$WORK/err"; then
        ok "the refusal names what holds the lock, and its pid"
    else
        bad "the refusal did not identify the holder: $(tr '\n' ' ' < "$WORK/err" | head -c 160)"
    fi

    # --- re-entrancy, WITH A REAL DESCENDANT --------------------------------------------------
    # A descendant of the holder must proceed. `release-check` takes the lock and then runs the
    # suites, which take it too when started on their own; without this they would block on their
    # own parent forever and the gate would look hung.
    #
    # THIS CONTROL USED TO BE A LIE, AND IT CERTIFIED THE BUG IT WAS MEANT TO CATCH. It ran
    #
    #     KMONEY_WORKSPACE_LOCK=1 bash -c "... workspace_lock ..."
    #
    # -- an unrelated shell with a variable set by hand, no descriptor, no ancestry, no lock. It
    # passed because the lock trusted that variable, so the control asserted that anyone who sets
    # an environment variable may bypass the single-writer property. A 2026-07-26 review's own
    # control put it plainly: `lock-control returned-success-without-lock=yes`.
    #
    # The holder now writes a child's result to a file. The child is started BY the holder, so it
    # inherits the descriptor the way a suite started by `release-check` does, and no variable can
    # substitute for that.
    for _ in $(seq 1 100); do [ -f "$WORK/child.rc" ] && break; sleep 0.1; done
    if [ "$(cat "$WORK/child.rc" 2>/dev/null || echo missing)" = 0 ]; then
        ok "a real descendant of the holder proceeds instead of deadlocking on its own parent"
    else
        bad "an actual child of the holder was refused, so release-check would block on its own suites"
    fi

    # --- the forged variable ------------------------------------------------------------------
    # The other half of the same contract: an unrelated process that merely SAYS it inherited the
    # lock must be refused. This is the exact command the old re-entrancy control ran and called
    # correct.
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

    # --- EVERY PUBLIC ENTRY POINT, WHILE THE LOCK IS HELD -------------------------------------
    # The property this lock claims is not "two release checks exclude each other". It is "nothing
    # writes the fixed scratch paths while somebody else is". A 2026-07-26 review showed those are
    # different: `release-check` took the lock correctly, and `yb-build` took none at all, while
    # `yb-ab` and `yb-native` wrote the shared artefact triplet and THEN reached a locked runner --
    # so the refusal arrived after the overwrite. Timed around artifact extraction that binds one
    # node image beside another build's hashes, and pgrx's generated SQL is not reproducible, so
    # the substitution is not merely a theoretical byte difference.
    #
    # REFUSAL IS NOT ENOUGH; IT MUST COME FIRST. So this checks two things per entry point: a
    # non-zero exit, and that nothing under the fixture run root changed while it ran.
    #
    # `docker` and `cargo` are STUBBED to record-and-fail. An entry point that gets past the lock
    # will reach one of them, and the stub turns a twenty-minute image build inside `just check`
    # into a recorded line -- so a regression here is loud and cheap instead of loud and expensive.
    mkdir -p "$WORK/bin"
    for tool in docker cargo; do
        printf '#!/bin/sh\nprintf "%%s %%s\\n" "%s" "$*" >> "%s"\nexit 1\n' \
            "$tool" "$WORK/tool.calls" > "$WORK/bin/$tool"
        chmod +x "$WORK/bin/$tool"
    done
    : > "$WORK/tool.calls"

    # HERMETIC: THE PROBED ENTRY POINTS AND THE OBSERVED TREE ARE BOTH THE FIXTURE.
    #
    # This used to point at the real `kamu-money-pg/yb/out`, write a canary into it, and snapshot
    # it. That made the selftest observe every other run on the machine: a legitimate concurrent
    # `gate-pg-release` writing there changed the snapshot mid-probe, and this reported
    #
    #     'just _yb-ab-ref sha256:000...000' changed shared state before being refused
    #
    # blaming whichever entry point happened to be under test. A guard that fails for something
    # its subject did not do teaches people to re-run it until it passes, which is the end of it
    # as a guard. It also littered the real tree with a canary file.
    #
    # `KMONEY_RUN_ROOT` is what makes the redirect possible, and it is exported so the entry
    # points below inherit it: every artifact-dir default in the suites resolves from it, so a
    # probed script that DID write before refusing would write here, where the snapshot is
    # watching. Nothing outside this fixture is read or written.
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
# The lock is an open file descriptor, so the kernel drops it when the last holder dies. Nothing
# unlocks explicitly, on purpose: an explicit unlock is the one that gets skipped when a run is
# killed with -9, which is precisely when a stale lock does the most damage.
#
# "THE LAST HOLDER", NOT "THE PROCESS THAT LOCKED IT", and the difference is load-bearing. The
# descriptor is inherited across fork and exec, so every child of the holder holds it too. Killing
# only the top-level script therefore does NOT free the workspace while a suite it started is
# still running -- which is the correct answer, because that suite is still writing the files the
# lock protects, but it is not the answer one would guess. This control was written expecting the
# guess, failed, and is kept in the shape that documents the real semantics.
# No `wait`: setsid detached the holder, so it is not a child of this shell. Poll for its death.
kill "$HOLDER_PID" 2>/dev/null || true
for _ in $(seq 1 100); do kill -0 "$HOLDER_PID" 2>/dev/null || break; sleep 0.1; done
if bash -c "source '$LOCKLIB'; workspace_lock 'selftest-orphan'" 2>/dev/null; then
    bad "the lock freed while a child of the dead holder was still running and still writing"
else
    ok "killing only the top-level holder does NOT free the lock while its children live"
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
