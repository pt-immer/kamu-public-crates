#!/usr/bin/env bash
# Negative controls for the battery oracle.
#
# `assert-battery.sh` makes `yb-ab`'s equality meaningful, so every assertion must reject a
# corresponding mutation.
#
# This test reads `assert-battery.sh --list` and derives one mutation per table row. Adding an
# assertion therefore adds its control automatically.
#
# Each mutation deletes every matching line. The oracle must reject it for that assertion's own
# reason; a broad pattern that deletes the whole file therefore fails this control.
#
# The control is pure grep and runs in every `yb-ab`.
#
# Usage: assert-battery-selftest.sh [known-good-output]
set -euo pipefail
cd "$(dirname "$0")/../.."   # repo root

# Lock before touching the shared default run root. A distinct `KMONEY_RUN_ROOT` isolates a run;
# descendants of the release gate inherit the descriptor and re-enter.
# shellcheck source=kamu-money-pg/yb/workspace-lock.sh
source ./kamu-money-pg/yb/workspace-lock.sh
workspace_lock "$(basename "$0")" || exit 1

SRC="${1:-${KMONEY_RUN_ROOT:-kamu-money-pg/yb/out}/out-yb.txt}"
ASSERT=./kamu-money-pg/yb/assert-battery.sh

if [ ! -s "$SRC" ]; then
    echo "battery-selftest: SKIP — no battery output at $SRC (run \`just yb-native\` first)" >&2
    exit 0
fi

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT INT TERM HUP

fails=0
controlled=0

# Every case must FAIL, and its message must name the expected reason. A case failing for the
# WRONG reason is reported as a broken control -- counting it as "worked" is how a control set
# rots while still looking green.
expect_fail() {
    local label="$1" file="$2" status="$3" want="$4" out
    if out="$($ASSERT "$file" "$label" "$status" 2>&1)"; then
        echo "  BAD: [$label] PASSED but should have failed"; fails=$((fails + 1)); return
    fi
    case "$out" in
        *"$want"*) return 0 ;;
        *) echo "  BAD: [$label] failed for the WRONG reason"
           echo "       wanted: $want"
           echo "       got:    $out"
           fails=$((fails + 1)) ;;
    esac
}

echo "battery-selftest: positive control"
if $ASSERT "$SRC" selftest-positive 0 >/dev/null 2>&1; then
    echo "  ok:  a real battery output passes all assertions"
else
    echo "  BAD: a real battery output was REJECTED — the oracle is too strict:"
    $ASSERT "$SRC" selftest-positive 0 2>&1 | sed 's/^/       /'
    fails=$((fails + 1))
fi

echo "battery-selftest: structural controls"
: > "$WORK/empty.txt"
expect_fail empty      "$WORK/empty.txt" 0 "missing or empty"
expect_fail status     "$SRC"            2 "client exited 2"
head -40 "$SRC"   > "$WORK/trunc.txt"
expect_fail truncated  "$WORK/trunc.txt" 0 "BATTERY COMPLETE"
cat "$SRC" "$SRC" > "$WORK/dup.txt"
expect_fail duplicated "$WORK/dup.txt"   0 "found 2"

# The client-status parameter is required. Prove the requirement is real, or a
# later "convenience" default would silently reinstate the assumption that nothing broke.
if $ASSERT "$SRC" nostatus >/dev/null 2>&1; then
    echo "  BAD: [no-status] the oracle accepted a MISSING client status"; fails=$((fails + 1))
fi

echo "battery-selftest: table-driven controls (one per assertion)"
while IFS= read -r row; do
    [ -n "$row" ] || continue
    mode="${row%%\%\%\%*}"; rest="${row#*%%%}"
    pat="${rest%%\%\%\%*}";  rest="${rest#*%%%}"
    desc="${rest#*%%%}"
    controlled=$((controlled + 1))
    case "$mode" in
        E) grep -Ev -- "$pat" "$SRC" > "$WORK/m.txt" || true ;;
        *) grep -vF -- "$pat" "$SRC" > "$WORK/m.txt" || true ;;
    esac
    # The label MUST NOT contain the description. `assert-battery.sh` echoes its label back in
    # the failure message, so passing the description as both label and expected-reason makes
    # the wrong-reason check match the label and pass unconditionally — it would verify nothing
    # for every table row, which is the same defect (a control that cannot fail) that this whole
    # script exists to prevent.
    expect_fail "ctl-$controlled" "$WORK/m.txt" 0 "$desc"
done < <($ASSERT --list)

# Derived controls cover each present assertion; the floor also detects removal.
# Lowering this number is a deliberate act that should be argued for in the diff.
MIN_ASSERTIONS=20
if [ "$controlled" -lt "$MIN_ASSERTIONS" ]; then
    echo "  BAD: the assertion table has shrunk to $controlled (floor is $MIN_ASSERTIONS) — an assertion was removed"
    fails=$((fails + 1))
fi

total=$((controlled + 5))
if [ "$fails" -eq 0 ]; then
    echo "battery-selftest: OK — $controlled table assertions + 5 structural controls all bite ($total checks)"
else
    echo "battery-selftest: FAIL — $fails control(s) misbehaved; the oracle is NOT trustworthy" >&2
    exit 1
fi
