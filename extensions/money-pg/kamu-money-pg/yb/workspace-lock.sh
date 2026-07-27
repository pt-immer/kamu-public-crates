#!/usr/bin/env bash
# ONE WRITER AT A TIME for this workspace's scratch space.
#
#   source ./kamu-money-pg/yb/workspace-lock.sh
#   workspace_lock "release-check"
#
# WHY THIS EXISTS. Containers and networks in this repo are well isolated: every suite invents a
# run ID, brings up its own private network and labels everything it creates, so two suites cannot
# see each other's nodes. The FILESYSTEM side had no such property. Every path below now resolves
# under ${KMONEY_RUN_ROOT:-kamu-money-pg/yb/out}, and with that variable UNSET -- the default --
# they are as fixed as they ever were:
#
#   <root>/{kmoney.so,kmoney.control,kmoney--*.sql}   the extracted triplet
#   <root>/out-yb.txt                                 the YB battery output
#   <root>/ref/out-pg15.txt                           the stock-PG15 reference
#   <root>/regress-yb, regress-cluster-n*, suite-n*.log, concurrent, release-suites
#
# So two release checks in one checkout could overwrite the extracted triplet between the point it
# is hashed and the point the manifest records it -- binding node image A to artifact hashes from
# node image B -- or diff one run's battery output against another run's reference, or replay a
# sibling's suite log into a transcript that calls itself the ordered release evidence. pgrx's
# generated SQL is not reproducible, so that substitution is possible even when both runs start
# from the same revision. Nothing here would have raised.
#
# WHAT THIS LOCK STILL PROTECTS, AND WHAT IT NO LONGER HAS TO. The paragraph that stood here chose
# a lock INSTEAD of run-scoped paths, called run-scoping the better end state, and said it would be
# worth the change once this workspace lived somewhere with real CI. That happened: it is now
# extensions/money-pg in kamu-public-crates. Every path above resolves from KMONEY_RUN_ROOT, so a
# caller that sets it writes a tree nobody else touches.
#
# The lock did NOT become redundant, because run-scoping never covered the case it was chosen for:
# a stray `just yb-ab` started BY HAND, with KMONEY_RUN_ROOT unset, writes the same `out-yb.txt` a
# release check is reading. The boundary is therefore:
#
#   run-scoped, no lock needed   every path above, whenever KMONEY_RUN_ROOT is set
#   still shared, still locked   those same paths under the DEFAULT root -- anything started by
#                                hand, or by a recipe that did not set the variable
#
# CI needs neither: each job gets a fresh runner with its own checkout, so there is no second
# writer to exclude. What remains here is a developer-workstation guard, which is what it was
# actually defending all along. The cost is unchanged and still deliberate -- a second release
# check sharing the default root is refused rather than proceeding independently.
#
# SOURCED, NEVER EXECUTED. A lock is held by an open file descriptor for as long as the holding
# process lives; a subprocess that takes one and exits has released it before its caller does any
# work. `flock -n` fails immediately rather than blocking: a gate that silently waits 27 minutes
# on another run looks exactly like a gate that has hung.

# Where the lock lives: resolved from THIS file, not from the caller's cwd, so a script that has
# already `cd`'d somewhere still finds the one lock.
# IS THE INHERITED LOCK REAL? Answers from the kernel, not from an environment variable.
#
# `KMONEY_WORKSPACE_LOCK_FD` names a descriptor number. This checks that the number is open in
# THIS process and that it refers to the very lock file we were about to take -- which is exactly
# the property "a descendant of the holder" means, and the only one that cannot be produced by
# typing a variable on a command line. `/proc/$$/fd/N` is a symlink to the open file's path, so
# the comparison is against the file, not against the caller's word for it.
_workspace_lock_inherited() {
    local want="$1" fd="${KMONEY_WORKSPACE_LOCK_FD:-}" got
    case "$fd" in ''|*[!0-9]*) return 1 ;; esac
    [ -e "/proc/$$/fd/$fd" ] || return 1
    got="$(readlink -f "/proc/$$/fd/$fd" 2>/dev/null)" || return 1
    want="$(readlink -f "$want" 2>/dev/null)" || return 1
    [ -n "$got" ] && [ -n "$want" ] && [ "$got" = "$want" ]
}

workspace_lock() {
    local what="${1:-a workspace suite}"

    # `KMONEY_LOCK_DIR` is overridable ONLY so the self-test can drive the exclusion controls
    # against a fixture directory. Without it the self-test contends for the REAL lock, which
    # makes it fail whenever it runs nested inside something that legitimately holds it --
    # `release-check` does, and on 2026-07-26 that turned the whole gate red. Same escape hatch,
    #
    # RESOLVED BEFORE THE RE-ENTRANCY CHECK, because that check now compares the inherited
    # descriptor against this exact path rather than trusting a flag.
    local dir
    dir="${KMONEY_LOCK_DIR:-$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/out}"
    mkdir -p "$dir"
    local lock="$dir/.workspace.lock" holder="$dir/.workspace.holder"

    # RE-ENTRANT, AND THE INHERITANCE IS VERIFIED RATHER THAN ANNOUNCED. `release-check` takes the
    # lock and then invokes the suites, which take it too when run on their own; the descriptor is
    # inherited across exec, so the lock is genuinely still held and a descendant must proceed
    # rather than deadlock against its own parent.
    #
    # UNTIL 2026-07-26 THE TEST FOR THAT WAS `KMONEY_WORKSPACE_LOCK=1`, AND NOTHING ELSE. A review
    # ran the obvious control:
    #
    #     lock-control returned-success-without-lock=yes
    #
    # A mistyped export, an inherited variable from an unrelated tool, or a `-e` on a docker run
    # disabled the single-writer property outright -- and the self-test's own "descendant" control
    # was an unrelated `bash -c` with the variable set, so it certified the bypass as correct.
    #
    # The descriptor is the thing that actually holds the lock, so the descriptor is what gets
    # checked. A forged variable now names a number that is either not open or is open on some
    # other file, and both refuse.
    if [ -n "${KMONEY_WORKSPACE_LOCK_FD:-}" ]; then
        if _workspace_lock_inherited "$lock"; then
            return 0
        fi
        {
            printf 'workspace-lock: REFUSING to start %s -- KMONEY_WORKSPACE_LOCK_FD is set to\n' "$what"
            printf 'workspace-lock: %s, but that descriptor is not an open handle on\n' "${KMONEY_WORKSPACE_LOCK_FD}"
            printf 'workspace-lock: %s in this process.\n\n' "$lock"
            cat <<'EOF'
That variable means "an ancestor already holds the workspace lock and passed me its descriptor".
Only the descriptor proves it, and this process does not have it -- so either the variable was set
by hand, or it was inherited from a run that has since exited, or the lock directory moved
(KMONEY_LOCK_DIR). Proceeding would let two writers into the fixed scratch paths with nothing
raising, which is the failure this lock exists to make impossible.

Unset KMONEY_WORKSPACE_LOCK_FD and run this command directly.
EOF
        } >&2
        return 1
    fi

    # APPEND MODE, so that opening the lock file does not truncate it. With `>` a refused caller
    # would destroy the holder record in the act of trying to read it.
    exec {KMONEY_LOCK_FD}>>"$lock"

    if flock -n "$KMONEY_LOCK_FD"; then
        printf 'what     %s\npid      %s\nstarted  %s\n' \
            "$what" "$$" "$(date -u +%Y-%m-%dT%H:%M:%SZ)" > "$holder"
        # THE DESCRIPTOR NUMBER, exported, so nested `just` recipes and runner scripts can VERIFY
        # the inheritance rather than be told about it. bash does not set close-on-exec on a
        # `{var}>` descriptor, so the number is still open on the same file in every descendant --
        # which is what `_workspace_lock_inherited` checks and what makes the claim falsifiable.
        #
        # Not unset on the way out: the lock is released when this shell exits and closes the
        # descriptor, which is the only release that is correct under a kill.
        export KMONEY_WORKSPACE_LOCK_FD="$KMONEY_LOCK_FD"
        return 0
    fi

    {
        printf 'workspace-lock: REFUSING to start %s -- another run owns this checkout.\n\n' "$what"
        if [ -s "$holder" ]; then
            sed 's/^/  /' "$holder"
        else
            printf '  (no holder record; the lock is held but its owner did not write one)\n'
        fi
        cat <<'EOF'

Both runs are using the DEFAULT scratch root, kamu-money-pg/yb/out/ -- the extracted extension
triplet, the A/B battery output and its PG15 reference, the regress and cluster and concurrency
work directories, and the parallel suite logs. Two writers there do not fail loudly; they produce
one run overwriting output another is mid-way through reading.

Give this run its own tree instead of waiting:

    KMONEY_RUN_ROOT=kamu-money-pg/yb/out/runs/$(date -u +%Y%m%dT%H%M%SZ)-$$ <your command>

Every path listed above resolves from that variable, so a run that sets it shares nothing and is
not refused. Waiting for the other run to finish, or using a separate checkout, both still work.

If the pid above is gone and the lock is STILL held, look for its orphaned children: the lock is
an open file descriptor and every descendant inherits it, so a suite that outlived the script
which started it is still holding -- and is still writing these files, which is why it counts.
  ps -o pid,ppid,lstart,cmd -e | grep -E 'yugabyte|docker|cargo|run-yb'
EOF
    } >&2
    return 1
}
