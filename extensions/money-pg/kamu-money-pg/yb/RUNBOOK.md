# `kmoney` on YugabyteDB — operator runbook

What to do when the YugabyteDB version changes, when the extension will not load, and what the
retreat is if the native path is ever blocked.

This exists because the risk it manages is **not closeable by testing**.

State it precisely, because an earlier version of this line overstated it and the overstatement got
quoted back at us in a review. **YugabyteDB does host pgrx extensions**: it vendors pgrx in-tree
(`src/postgres/third-party-extensions/pgrx`) and ships pgrx-based extensions of its own. What is
true is narrower and still decisive:

- `kmoney` is **not on Yugabyte's supported-extension list**, so no vendor will treat a break here
  as their defect;
- the three `yb-pg15` adaptations in our pgrx fork are **ours**, derived against one image's
  headers, and nobody upstream will notice when a release moves one of them;
- managed platforms that refuse third-party extensions cannot load it at all.

Whether to adopt the native path anyway — with written Yugabyte acceptance, or as an explicitly
owned self-managed exception, or not at all in favour of the text adapter in §4 — is an
organisational decision, not a technical one, and this file does not make it. What the machinery
below changes is *when we find out*: at build time, on our terms, instead of in production.

---

## 1. The standing condition

`kamu-money-pg` builds against **YugabyteDB's own PG15 fork headers**, inside the YugabyteDB image,
against a three-patch **fork of pgrx 0.19.1**:

<https://github.com/fluminis-scientiae-oraculum/pgrx-yugabytedb> — tag `v0.19.1-yb.1`, feature
`yb-pg15`, patched in by `[patch.crates-io]` in the workspace root `Cargo.toml`.

| # | What YugabyteDB does differently | What the fork does |
|---|---|---|
| 1 | YSQL is multi-threaded, so the process-global `CurrentMemoryContext` is a thread-local `YbCurrentMemoryContext` | one crate-root alias in `pgrx-pg-sys` |
| 2 | `index_build_range_scan` takes 14 parameters, not upstream's 11 | passes the three extra as `null, null, None`, at all 7 generated call sites |
| 3 | `BackgroundWorker` carries `bgw_oom_score_adj` | zeroes the field |

**The fork is upstream `v0.19.1` plus 33 lines, every one of them inside
`#[cfg(feature = "yb-pg15")]`.** Without the feature it compiles byte-identical to the release it
is based on — which is why it can be the dependency for *every* build here, including the
PostgreSQL 15–18 matrix that never enables it.

**Why it is a fork and not a build-time patch.** It used to be the latter: a script vendored pgrx
and rewrote it textually inside the image. `re.sub` and `str.replace` return the subject
**unchanged** when nothing matches, so a YugabyteDB release that renamed any of those symbols
produced a *successful build of an unadapted extension*, and the first symptom would have been
money read through the wrong memory-context global. A fork cannot fail that way: a patch that no
longer applies is a compile error.

**What a fork does NOT fix.** The exposure was never pgrx drift — it is YugabyteDB moving a symbol,
and that costs the same three edits either way. What changed is only *when you find out*.

Three independent things now stop that, and they answer different questions:

- **`yb-image.sh`** — *is this the image anyone validated?* It resolves the mutable tag to a digest
  and **fails closed in both directions**: a tag that has moved off its recorded digest is refused,
  and so is a tag with no entry in `YB-PINNED.txt` at all. The second half is new. It used to warn
  and proceed, so the one case where *nothing had ever been validated* was the one case that ran —
  and `just yb-pin-check` printed "the tag still resolves to the validated digest" over the top of
  it. Two separate overrides, because they bypass different checks:
  `YB_ALLOW_DRIFT=1` for a moved pin, `YB_ALLOW_UNPINNED=1` for an unrecorded tag. Neither rescues
  the other's case; `just yb-image-selftest` proves each refusal path and each override.
- **`probe-yb-abi.sh`** — *is the world still shaped the way the fork assumes?* It reads this
  image's headers and asserts all three symbols still have the expected shape, **before** the
  extension is compiled. Runs inside the build. The compiler cannot replace it: an adaptation can
  still compile while no longer being the *right* one — three arguments too many, or an alias
  shadowing a global that came back.
- **the compiler** — *did our change actually take?* The adaptations now live in a pgrx **fork**
  (`pgrx-yugabytedb`, tag `v0.19.1-yb.1`, feature `yb-pg15`) that the workspace root patches in,
  rather than in textual edits applied at build time. A patch that no longer applies is a build
  failure by construction, which is what retired the silent-no-op failure class below.

---

## 2. Adopting a new YugabyteDB version

Do this deliberately. Do not do it by letting a `docker pull` decide.

**Every command below takes the tag and actually uses it.** They did not, until 2026-07-25: steps 2
and 3 ran `just yb-build` / `just yb-ab` with no way to pass a tag, so the recipes resolved the
hard-coded default and the whole procedure tested *the image you already had* while reading as
though it had tested the new one. Every `yb-*` and `test-yb-*` recipe now takes `tag=` and forwards
it to `yb-image.sh`.

Pick the override by which check you are bypassing: `YB_ALLOW_UNPINNED=1` for a tag this repo has
never recorded, `YB_ALLOW_DRIFT=1` for a known tag whose build has moved.

```bash
NEW=yugabytedb/yugabyte:<new-tag>

# 1. See what the tag resolves to in the REGISTRY (not the local cache) and against the pin.
#    It refuses -- that is the point. Read the digest it prints.
YB_PULL=1 ./kamu-money-pg/yb/yb-image.sh "$NEW"

# 2. Build against the new image WITHOUT recording it yet. The ABI probe runs first, so if an
#    adapted symbol moved you find out here, in a build log, with the symbol named.
#
#    ARGUMENTS ARE POSITIONAL (just-anti-example on the next line -- it is the BROKEN form):
#    `just` has no `name=value` call syntax: `just yb-build tag=$NEW`
#    passes the literal string "tag=$NEW" and would resolve the DEFAULT image while reading as
#    though it had used yours. The recipes now refuse a malformed argument rather than proceed.
YB_ALLOW_UNPINNED=1 just yb-build "$NEW"

# 3. The full native gate against the new image: byte-exact A/B versus stock PG15, the ported
#    case suite, the 3-node cluster, concurrency, a read-replica placement, and a
#    dump/restore into a clean cluster. `4` overlaps the five suites after the A/B barrier.
YB_ALLOW_UNPINNED=1 just release-check 4 "$NEW"

# 4. ONLY if all of the above are green: record the new digest in kamu-money-pg/yb/YB-PINNED.txt,
#    add an evidence entry to specs.md, and commit the two together.
```

If step 2 or 3 fails, **the answer is not to loosen the probe.** Re-derive the adaptation in the
pgrx fork, or stay on the pinned version, or take the retreat in §4.

### 2b. Adopting a new pgrx, or changing the adaptation

The adaptation lives in a separate repository now, so a change there is its own release:

```bash
# In the pgrx-yugabytedb checkout: branch from the upstream tag you are moving to, apply the
# three yb-pg15 patches, and tag it. Crate names and version numbers DO NOT CHANGE --
# `[patch.crates-io]` matches on crate name, and cargo-pgrx refuses to build an extension whose
# pgrx version differs from the CLI's own. Distinguish releases by TAG only.
git checkout -b yugabytedb-<new-version> v<new-version>
#   ... apply the three patches, gated on `feature = "yb-pg15"` ...
git tag -a v<new-version>-yb.1 && git push origin yugabytedb-<new-version> v<new-version>-yb.1

# Back here: bump BOTH lines of [patch.crates-io] in the workspace root Cargo.toml to the new
# tag, bump the `pgrx =` version in kamu-money-pg/Cargo.toml and cargo-pgrx to match, then run
# the full gate. `just doctor` checks the cargo-pgrx/pgrx pairing.
just release-check
```

To test an **unpushed** fork revision, check it out at `vendor/pgrx-yugabytedb` and switch the two
`[patch.crates-io]` lines to `path = "vendor/pgrx-yugabytedb/pgrx"` and `.../pgrx-pg-sys`. That
path is gitignored and `exclude`d from the workspace, which is what stops cargo resolving the
fork's own `authors.workspace = true` against this root and dying in `cargo metadata`.

---

## 3. Diagnosing a failure

| Symptom | Almost certainly |
|---|---|
| `yb-image: REFUSING -- the YugabyteDB tag has moved off the validated digest` | Upstream retagged. Follow §2; do not set `YB_ALLOW_DRIFT=1` and forget. |
| `yb-image: REFUSING -- the YugabyteDB tag is not recorded in the pin file` | A tag nobody has validated. Follow §2 with `YB_ALLOW_UNPINNED=1`; do not reach for `YB_ALLOW_DRIFT=1`, which will not help and is a different bypass. |
| `probe-yb-abi: FAILED ... shim patch N` | A YugabyteDB release moved an adapted symbol. §2, and expect real work. |
| a **compile error** inside `pgrx` / `pgrx-pg-sys` | A pgrx upgrade moved the code the `yb-pg15` patches apply to. Re-derive the fork against that pgrx version (§2b). This replaced the old `shim N/3 FAILED: no ... call site matched` row: with a fork, a patch that no longer applies cannot succeed silently, so there is no such message any more. |
| `artifact: INCOHERENT TRIPLET` or `MANIFEST MISMATCH` | The `.so`, control file and install script in `yb/out/` did not come from one build. Rebuild into an empty directory; do not hand-pick files. |
| `could not access file "$libdir/kmoney"` on one node | That node is missing `kmoney.so`. Every node needs it at the same version — this is what `run-yb-cluster.sh`'s negative control pins. |
| `CREATE EXTENSION` works but a query on another node fails | Same as above: the DDL propagates, the shared library does not. |
| Suite green on stock PG15, red on YugabyteDB | A genuine divergence. The case name and the assertion label in the diff say which contract broke. |
| Suite red on **both** | The port or the extension, not YugabyteDB. Check `just test-pg 15` first. |

Useful:

```bash
just yb-pin-check                      # what is pinned vs what the REGISTRY resolves the tag to
just yb-image-selftest                 # prove the pin gate's refusal paths still bite
just yb-native                         # the ABI battery on a fresh single node
just test-yb-regress                   # the full ported suite on a single node
docker ps -a --filter 'label=kamu-money-pg.revision'   # anything this workspace left behind
```

---

## 4. The retreat: the text adapter

**If the native path is ever blocked — a YugabyteDB upgrade the shim cannot follow, a managed
service that will not load a third-party extension — there is already a tested way to run without
it, and it needs no extension at all.**

`kamu-money-core`'s `postgres` and `sqlx` features store and read money as the **canonical text
form** (`kamu_money_core::text`), in an ordinary `text` column. It is green against YugabyteDB
today: `just test-yb`.

What you give up, stated plainly so the decision is made with the trade in view:

- **No in-database arithmetic.** No `+`, `-`, `kmoney_sum`, `kmoney_div`, `kmoney_allocate`. Every
  computation moves into the application.
- **No column-level currency guarantee.** `kmoney('IDR')` refuses a USD value at INSERT; a `text`
  column takes anything, and a `CHECK` constraint is not the same thing because it runs after
  coercion rather than before parsing.
- **Wider rows and slower comparisons** — a variable-length string instead of a fixed 18 bytes.

What you keep, which is the part that matters: **the same codec, so the same values.** One
implementation of the text form serves both paths, which is exactly what the phase-4/phase-5
differential case (`02-text`, `the_native_type_and_the_text_storage_agree`) exists to pin. Data
written by one path is readable by the other. The retreat is a migration of *mechanism*, not of
meaning.

The other mitigation worth deciding at org level rather than here: **offering the shim upstream.**
Three symbols is a small patch. If pgrx or Yugabyte takes it, the standing condition substantially
closes. If neither does, that answer is itself useful.

---

## 5. Deploying, upgrading and rolling back

**The extension ships in the node image. Nothing is ever installed onto a running node.**

That one rule is the whole deployment model, and it is what makes the procedure independent of
cluster shape. Installing onto live nodes means enumerating every tserver — remembering that read
replicas are tservers too — and redoing it for every node that is ever replaced, autoscaled or
recovered. It does not survive a node dying at 3am.

```bash
just yb-node-image          # -> kmoney-yugabyte:<version>, prints base digest + image id
```

Deploy **by digest**. Then:

| Question | Answer |
|---|---|
| Is the extension on every node? | Is every node on digest `D`? — your orchestrator already knows |
| Primary or read replica? | Same image. No distinction. |
| RF3 or RF5? | Irrelevant. |
| Node replaced after a failure? | Boots correct by construction |
| Scale out? | Same |

This is the same identity discipline `yb-image.sh` already applies to the base image one level
down: a tag is mutable, a digest is not, and the artifact is a claim about one image.

### The part images do not solve

`CREATE EXTENSION` and `ALTER EXTENSION UPDATE` are **catalog** state — once per cluster, not per
node. So an upgrade has an order, and it is ordinary PostgreSQL extension discipline rather than
anything YugabyteDB-specific:

1. **Build and gate the new image first.** `just yb-node-image <new-tag>`, then
   `just release-check 4 <new-tag>`. The `.so` is compiled against one image's PG15 headers and
   glibc, so it must be built for the version it will run on.
2. **Roll the image everywhere, before touching the catalog.** During the roll, nodes run mixed
   versions; the new library must still serve the **current** catalog version. Binary first,
   catalog second — never both at once.
3. **`ALTER EXTENSION UPDATE`, once, on one node.** It is DDL; it reaches the rest.
4. **Recycle connections.** `module_pathname = 'kmoney'` means live backends keep the library they
   already loaded, so a pooled long-lived session will run old code against a new catalog until it
   is replaced.

**Rollback** is step 2 in reverse — redeploy the previous image digest — and it is safe only while
step 3 has not run. Once the catalog has moved, rolling the binary back means the old library
facing a newer catalog. Keep the previous digest deployable, and treat step 3 as the cutoff.

**Fault headroom during the roll.** RF3 tolerates one node loss, and a rolling upgrade *is* one
node down — so there is zero headroom for its duration. RF5 tolerates two. That is an argument for
RF5 on whichever cluster carries the money, independent of this extension.

**Backup/restore — rehearsed, and gated.** `just test-yb-restore` (in `release-check`) dumps a
schema holding `kmoney`, restores it into a clean cluster on the same image, and asserts that the
18-byte payloads survive byte-for-byte at the domain edges, that a `kmoney('USD')` column comes
back still *refusing* IDR rather than merely printing its modifier, that the extension returns at
the same version, and that totals agree across the round trip.

The operational rule it proves, rather than asserts: **the extension must exist in the target
cluster before the restore reaches `CREATE EXTENSION`.** PostgreSQL represents an extension in a
dump as that one statement and does *not* dump its member objects, so a destination without the
files fails there — the suite's negative control drives exactly that and requires it to fail
loudly. Restore into a cluster already running the node image, which is the whole argument for the
image: "is the restore target on digest `D`?" is a question your orchestrator answers, where
"did someone install the extension on the restore target first" is a runbook step that gets
skipped at 3am.

**Still yours, not this repository's:** RPO, RTO, backup cadence, where dumps live, and who runs
them. `kmoney` is a library and an extension; it can prove its own round trip and nothing about
your recovery objectives.

### An extension version bump

There is one version (`0.1.0`) and no `kmoney--0.1.0--0.2.0.sql` upgrade script, because there has
been nothing to upgrade from. The first bump must add one: PostgreSQL runs `old--new.sql` for
`ALTER EXTENSION UPDATE` and will refuse the update without it. A bump that changes the on-disk
payload is a different and much larger question — the 18-byte layout is load-bearing and pinned by
`01-layout`.

**This is the whole reason the extension version is not a release identity.** Bumping it is a
migration you owe a script for; releasing is not. Individual releases are named by a signed tag,
`v0.1.0-r.<n>.<short-hash>`, which nothing in the build reads — so a release costs no bump and no
commit. `just tag` mints it; AGENTS.md § *Release identity* has the reasoning.

## 6. What is NOT covered, and why

Honesty about the edges of the evidence is the point of this file.

- **A rolling version upgrade is not tested.** `run-yb-resilience.sh` covers node restart, node
  failure with writes continuing, and rejoin — but not a node-by-node upgrade from one YugabyteDB
  version to another. That needs a second image digest *and* a second from-source artifact build
  against that image's headers, because the `.so` is compiled against one fork's PG15 headers and
  glibc. Deliberately out of scope for now; if you are planning a rolling upgrade, §2 is the
  procedure, and the honest statement is that the cross-version step has not been rehearsed.
- **The node image is a TEST FIXTURE, not a deliverable, and there is no registry digest by
  design.** `just yb-node-image` prints a local image **ID**, which is not a registry digest.
  Nothing pulls it: consumers build the extension from source at a tag and own their own artifact
  identity and deployment. Publishing it would create a distribution channel nobody consumes and
  an implicit support claim for bytes nobody is meant to run.
- **Restore is rehearsed; disaster recovery is not.** The suite above proves the extension and its
  payloads survive a dump and restore. It says nothing about whether your backups exist, are
  recent, or can be found under pressure.
- **Performance numbers are a baseline, not a threshold.** `just bench-yb` records what `kmoney`
  costs against `text` and `numeric(36,18)` on this hardware. There is deliberately no pass/fail
  limit: a number invented before there is anything to regress against is either so loose it never
  fires or so tight it fires on somebody else's machine.
- **One YugabyteDB version.** Everything here is `2025.2.5.1-b1` at the pinned digest. That is what
  the pin is for — it is not a claim about `2025.2.x` in general.
