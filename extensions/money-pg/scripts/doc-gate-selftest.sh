#!/usr/bin/env bash
# Controls for the lane's rustdoc gate.
#
#   scripts/doc-gate-selftest.sh
#
# `doc-pg` claims to fail on a broken intra-doc link. Three separate settings decide whether it
# can: the deny that turns a rustdoc warning into an exit code, `--document-private-items`, and
# the feature list that decides which modules the compiler hands rustdoc at all. Two earlier
# attempts asserted those settings by reading the recipe and the Cargo configuration, and both
# were wrong about spellings that look correct and deny nothing -- `["-D warnings"]` as one array
# element reaches rustdoc as a single argv token, and an ambient `RUSTDOCFLAGS` replaces a
# configured one outright.
#
# So this asserts the OUTCOME. A probe is planted in each region the gate must reach, `doc-pg` is
# run once, and its report must name every one of them. No spelling of the flags satisfies that
# without actually denying, and no region can be missing from the input set.
#
# It EDITS TRACKED SOURCE to do that, so it is written to leave nothing behind and to refuse
# rather than build on wreckage: it will not start if a probe from an earlier run is still
# present, and it fails loudly if one is still present after it restores.
set -euo pipefail
cd "$(dirname "$0")/.." # lane root

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

# Every probe name carries this, so one grep answers "is any of this script's damage still here".
MARKER="kmoney_probe_"

# `file|anchor|probe` per region the gate must reach. The probe is inserted as a doc comment
# directly above the anchor, so it lands in that item's documentation and inside whatever `#[cfg]`
# encloses it. `|` because an anchor contains `:`.
REGIONS=(
    "kamu-money-pg/src/safe/mixed.rs|/// Return the stable payload hash folded to \`int4\`.|${MARKER}private"
    "kamu-money-pg/src/safe/mixed.rs|    /// Equality is currency-aware and remains a non-indexed predicate.|${MARKER}pg_test"
    "kamu-money-pg/src/lib.rs|#[cfg(feature = \"boundary-probe\")]|${MARKER}boundary"
)

WORK="$(mktemp -d)"
SAVED=()

# The originals are mirrored under $WORK by path, so restoring needs no sidecar file that could
# go missing between two writes. One copy per FILE, not per region: two regions share `mixed.rs`,
# and a per-region copy would save the second one after the first probe was already planted.
save() {
    local file="$1"
    local mirrored="${WORK}/${file}"
    [ -e "${mirrored}" ] && return 0
    mkdir -p "$(dirname "${mirrored}")"
    cp "${file}" "${mirrored}"
    SAVED+=("${file}")
    return 0
}

# No `set -e` abort inside a trap: a restore that stops halfway is how a probe reaches a commit.
restore() {
    local file
    for file in "${SAVED[@]+"${SAVED[@]}"}"; do
        cp "${WORK}/${file}" "${file}" || printf 'doc-gate-selftest: FAILED to restore %s\n' "${file}" >&2
    done
    return 0
}

# A trapped signal whose handler merely RETURNS lets bash resume the script, which would report
# every control below as passed and exit 0. The signal paths therefore exit.
on_signal() {
    restore
    rm -rf "${WORK}"
    printf '\ndoc-gate-selftest: interrupted; source restored\n' >&2
    exit 130
}
trap 'restore; rm -rf "${WORK}"' EXIT
trap on_signal INT TERM HUP

# A SIGKILL, an out-of-memory kill or a cancelled CI job runs no trap at all, so an earlier run
# can have left a probe in tracked source. Saving that as the "original" would write it back
# permanently, and every later run would fail its own final control while blaming the doc gate.
leftover=0
for region in "${REGIONS[@]}"; do
    file="${region%%|*}"
    if grep -qF -- "${MARKER}" "${file}"; then
        printf 'doc-gate-selftest: %s still carries a probe from an interrupted run.\n' "${file}" >&2
        leftover=1
    fi
done
if [ "${leftover}" -ne 0 ]; then
    printf 'doc-gate-selftest: restore it (git checkout -- <file>) before running this again.\n' >&2
    exit 1
fi

planted=0
for region in "${REGIONS[@]}"; do
    file="${region%%|*}"
    rest="${region#*|}"
    anchor="${rest%|*}"
    probe="${rest##*|}"

    if ! grep -qxF -- "${anchor}" "${file}"; then
        bad "anchor absent from ${file}, so its region would be probed nowhere: ${anchor}"
        continue
    fi
    save "${file}"
    planted=$((planted + 1))

    # Inserts one doc line above the first line equal to the anchor, at the anchor's own
    # indentation, and refuses when the anchor is not there: a plant that silently did nothing
    # would leave doc-pg passing for the wrong reason.
    ANCHOR="${anchor}" PROBE="${probe}" awk '
        BEGIN { done = 0 }
        !done && $0 == ENVIRON["ANCHOR"] {
            match($0, /^[ \t]*/)
            printf "%s/// See [`%s`].\n", substr($0, 1, RLENGTH), ENVIRON["PROBE"]
            done = 1
        }
        { print }
        END { if (!done) exit 3 }
    ' "${file}" > "${file}.planted" || {
        rm -f "${file}.planted"
        bad "anchor not matched in ${file}"
        exit 1
    }
    mv "${file}.planted" "${file}"
done

if [ "${planted}" -eq 0 ]; then
    bad "no region was probed, so a passing run would prove nothing"
else
    # ONE run yields both the report and the status. Running `doc-pg` twice to get them
    # separately costs a second full rustdoc pass and proves nothing the first did not.
    if report="$(just doc-pg 2>&1)"; then
        bad "doc-pg exited 0 with ${planted} broken links planted"
    else
        ok "doc-pg exits non-zero on a broken intra-doc link"
    fi
    for region in "${REGIONS[@]}"; do
        probe="${region##*|}"
        if grep -qF -- "unresolved link to \`${probe}\`" <<<"${report}"; then
            ok "doc-pg reports ${probe}"
        else
            bad "doc-pg did NOT report ${probe}, so that region is outside the gate"
        fi
    done
fi

restore
for region in "${REGIONS[@]}"; do
    file="${region%%|*}"
    if grep -qF -- "${MARKER}" "${file}"; then
        bad "a probe survived the restore in ${file}; revert it before committing"
    fi
done

# Without this a gate that failed on EVERYTHING would satisfy every control above. It doubles as
# the lane's ordinary doc build, which is why `gate-offline` composes this recipe rather than
# `doc-pg` beside it.
if just doc-pg >/dev/null 2>&1; then
    ok "doc-pg exits 0 once the probes are removed"
else
    bad "doc-pg fails on the unmodified tree, so the controls above prove nothing"
fi

printf '\ndoc-gate-selftest: '
if [ "${fail}" -eq 0 ]; then
    printf '\033[32mall %d controls passed\033[0m\n' "${pass}"
else
    printf '\033[31m%d of %d controls failed\033[0m\n' "${fail}" "$((pass + fail))"
    exit 1
fi
