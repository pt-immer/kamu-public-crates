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

# `file|anchor|probe` per region the gate must reach. The probe is inserted as a doc comment
# directly above the anchor, so it lands in that item's documentation and inside whatever `#[cfg]`
# encloses it. `|` because an anchor contains `:`.
REGIONS=(
    "kamu-money-pg/src/safe/mixed.rs|/// Return the stable payload hash folded to \`int4\`.|kmoney_probe_private"
    "kamu-money-pg/src/safe/mixed.rs|    /// Equality is currency-aware and remains a non-indexed predicate.|kmoney_probe_pg_test"
    "kamu-money-pg/src/lib.rs|#[cfg(feature = \"boundary-probe\")]|kmoney_probe_boundary"
)

WORK="$(mktemp -d)"
# RESTORE FROM A COPY, never `git checkout --`, which restores to HEAD and would silently discard
# whatever else the working tree was carrying.
restore() {
    local saved target
    for saved in "$WORK"/*.orig; do
        [ -e "$saved" ] || continue
        target="$(cat "${saved%.orig}.path")"
        cp "$saved" "$target"
    done
    return 0
}
cleanup() {
    restore
    rm -rf "$WORK"
    return 0
}
trap cleanup EXIT INT TERM HUP

index=0
for region in "${REGIONS[@]}"; do
    file="${region%%|*}"
    rest="${region#*|}"
    anchor="${rest%|*}"
    probe="${rest##*|}"

    if ! grep -qxF -- "${anchor}" "${file}"; then
        bad "anchor absent from ${file}, so its region would be probed nowhere: ${anchor}"
        continue
    fi
    # ONE pristine copy per FILE, not per region: two regions share `mixed.rs`, and a
    # per-region copy would save the second one after the first probe was already planted,
    # then restore that stale copy last and leave a probe behind.
    key="${file//\//_}"
    if [ ! -e "${WORK}/${key}.orig" ]; then
        cp "${file}" "${WORK}/${key}.orig"
        printf '%s' "${file}" >"${WORK}/${key}.path"
    fi
    index=$((index + 1))

    ANCHOR="${anchor}" PROBE="${probe}" python3 - "${file}" <<'PY'
import os
import sys

path = sys.argv[1]
anchor = os.environ["ANCHOR"]
probe = os.environ["PROBE"]
with open(path, encoding="utf-8") as handle:
    lines = handle.readlines()
out = []
planted = False
for line in lines:
    if not planted and line.rstrip("\n") == anchor:
        indent = line[: len(line) - len(line.lstrip())]
        out.append(f"{indent}/// See [`{probe}`].\n")
        planted = True
    out.append(line)
if not planted:
    raise SystemExit(f"doc-gate-selftest: anchor not matched in {path}")
with open(path, "w", encoding="utf-8") as handle:
    handle.write("".join(out))
PY
done

if [ "${index}" -eq 0 ]; then
    bad "no region was probed, so a passing run would prove nothing"
else
    report="$(just doc-pg 2>&1 || true)"
    for region in "${REGIONS[@]}"; do
        probe="${region##*|}"
        if grep -qF -- "unresolved link to \`${probe}\`" <<<"${report}"; then
            ok "doc-pg reports ${probe}"
        else
            bad "doc-pg did NOT report ${probe}, so that region is outside the gate"
        fi
    done
    if just doc-pg >/dev/null 2>&1; then
        bad "doc-pg exited 0 with ${index} broken links planted"
    else
        ok "doc-pg exits non-zero on a broken intra-doc link"
    fi
fi

restore

# Without this a gate that failed on EVERYTHING would satisfy every control above.
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
