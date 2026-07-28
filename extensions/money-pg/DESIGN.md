# kamu-money-pg — Extension Lane Contract

**Status:** `kmoney` as a native PostgreSQL type (C8), green on PostgreSQL 15–18, native and
byte-exact on YugabyteDB (E16). Open items in §6.
**Toolchain:** edition 2024, MSRV 1.97 — pgrx 0.19.1 requires 1.96.

> **Scope.** The workspace layout (§3), the test contract (§4) and the open items (§6). The
> classification, evidence base, contracts and rejected alternatives describe the money scalar and
> live with it, in [`crates/money-core/DESIGN.md`](../../crates/money-core/DESIGN.md).
>
> Section numbers are preserved on both sides so every `C` and `E` citation keeps resolving. `E21`
> is defined here — it is a property of the extension's build. Every other `E` and `C` this lane
> cites (`E9`, `E12`–`E18`, `E20`, `C8`–`C10`) is defined in that document. Checked per identifier.
>
> §5 travelled with the scalar. §7 was a phasing plan whose every phase is done, and a completed
> plan is not a contract; it is not carried.

---

## 3. Workspace

pgrx requires a `cdylib`, which cannot be a library crate — so the extension is its own package,
and this lane is its own Cargo workspace:

```text
extensions/money-pg/
  kamu-money-pg/    pgrx extension -> kmoney   (cdylib)
  hygiene/          repository guards; no dependencies, so they run without pgrx
```

`kamu-money-core` is **not** a member. It is a published crate in the main workspace at
`crates/money-core`, and this lane depends on it by version, with the path injected on the command
line. The lane is separate because `[patch.crates-io]` is root-only, `panic = "unwind"` is honoured
only at a workspace root, and pgrx needs a higher MSRV than the published crates.

**The Cargo name and the SQL name are chosen independently, and both are deliberate.**

| name | spelling | why |
|---|---|---|
| Cargo package, directory | `kamu-money-pg` | the published identity, under the `kamu-` prefix with its siblings |
| `kmoney.control`, extension | `kmoney` | `cargo-pgrx` derives `extname` from the control file's **stem**, never from the package name (`command/get.rs:88-106`, read from the 0.19.1 source). The SQL name is therefore free, and is chosen to need no double-quoting |
| `[lib] name`, `kmoney.so`, `module_pathname` | `kmoney` | set explicitly rather than inherited, so all three SQL-side artefacts carry one name |

**A type name is the one part of an extension that cannot be revised after deployment** without a
dump and restore. `rmoney_t` became `rmoney` became `kmoney` while nothing was deployed; that
freedom is spent.

---

## 4. Test Contract

Each test pins a claim. A claim without a test is a rumour. The rows below are this lane's; the
scalar's own contract and the tests pinning it are documented with `kamu-money-core`.

| Test | Pins |
|---|---|
| `proptest` vs real PostgreSQL (testcontainers): `decode(encode(m)) == m` | C8, C9 |
| `proptest` vs real PostgreSQL: `pg_sum(rows) == rust_sum(rows)`, bit-exact | E9's exactness claim |
| All **63 portable** `#[pg_test]` assertions run against a live YugabyteDB as SQL cases, byte-identical to hand-authored goldens; the 64th asserts a stock-PostgreSQL *plan* and is declared `NOT-PORTABLE` with its reason | E17 — the YB evidence surface is the whole contract, not one script |
| The planner really **splits** `sum(kmoney)` into `Partial`/`Finalize`, and the two plans agree over domain-edge rows | R2-F4b — a `CREATE AGGREGATE` that silently forbids partial aggregation passes every hand-driven test |
| Every `#[pg_test]` has a row in `tests/pg_regress/COVERAGE.md`, or an explicit `NOT-PORTABLE: <reason>` | E17 — a skipped test counted as a pass is worse than an absent one; checked offline, in `gate-offline` |
| The case-suite oracle rejects 14 realistic corruptions, each **for its own reason** — including a client that died with perfect bytes, and a case with no golden | E17 — an oracle nothing checks certifies whatever the code currently does |
| The stock-PG15 reference runs the **same cases against the same goldens** | E17 — makes a YB failure a divergence rather than a question about the port's fidelity |
| `CREATE EXTENSION` on **one** node of a 3-node cluster; the type usable, and the pinned hashes identical, from **every** node | E18 — the DDL propagates; the shared library does not |
| A node with `kmoney.so` removed fails **loudly** rather than diverging | E18 — the negative control without which every cross-node probe is vacuous |
| A value written on one node reads back byte-identically on the others, and survives a forced **tablet split** | E18 — asserted as ordered-text md5 + hash fold + row count, because a count or a sum survives one corrupted payload |
| Concurrent balanced double-entry transfers across 3 nodes **conserve the total exactly**, and the ledger's legs cancel to zero | E18 — the invariant this type exists for, on a transaction layer that is DocDB rather than PostgreSQL's |
| Deliberate `SERIALIZABLE` contention **must** produce a retryable error | E18 — the positive control; otherwise "conservation under concurrency" never exercised the conflict path |
| The three shimmed ABI symbols still have the expected shape in this image's headers, checked **before** patching | E19 — converts a YugabyteDB upgrade from a production incident into a build failure |
| The YugabyteDB tag still resolves to the digest the fork was validated against | E19 — a new image is adopted deliberately, never by a `docker pull` |
| `kmoney` data survives a node restart, a node failure with writes continuing, and a rejoin | E19 — G5, minus the rolling version upgrade, which is named as not covered |
| `gate-pg-release` depends on all four YugabyteDB gates | E19 — dropping one for wall-clock is the obvious temptation and would make the gate's claim untrue |

### E21 — pgrx's generated SQL is not reproducible (measured 2026-07-27)

Measured with `cargo-pgrx 0.19.1` against PostgreSQL 18 via `cargo pgrx schema pg18 --out …`. Two
runs over **byte-identical source** were compared as a control, alongside a run with one
`#[pg_extern]` moved into a plain child module:

| generation | raw `sha256` | normalized `sha256` |
|---|---|---|
| baseline | `c52b666a33b2dece` | `ac5817b996d8aa6b` |
| rerun, unchanged source | `ed87c428951f7d97` | `ac5817b996d8aa6b` |
| `kmoney_div` in `mod division` | `2e73d4860e53e807` | `ac5817b996d8aa6b` |

**All three raw hashes differ. All three normalized hashes are identical**, over the same 32
objects. Three consequences:

- **pgrx's raw SQL is nondeterministic.** The order entities are emitted in varies between runs of
  the same source — a 437-line `diff` in which two functions swap places, neither having moved.
  Rebuilding proves nothing, which is why the release gate extracts artefacts from the built image
  rather than regenerating them.
- **A byte diff of generated SQL is a useless oracle.** `just pg schema-hash` strips pgrx's
  embedded `-- …/lib.rs:<line>` provenance comments — they encode source position, so any refactor
  changes them by construction — then sorts the objects and hashes that.
- **A plain child module is free.** No module path reaches a SQL name and the emitted object set is
  unchanged. The trap is `#[pg_schema]`, a different attribute that *does* create a SQL schema.

This establishes the mechanism for one representative item, and that a normalized hash is a
trustworthy oracle for checking the rest.

---

## 6. Open Items

| Item | Status |
|---|---|
| Self-maintained pgrx / YugabyteDB fork | **OPEN by decision.** `pt-immer/pgrx-yugabytedb` is ours to maintain. Every YugabyteDB digest or PostgreSQL-major change repeats `just pg gate-pg-release`. |
| Two-version rolling upgrade | **UNREHEARSED.** Restore is proven same-version. `kamu-money-pg/yb/RUNBOOK.md` §2 is the procedure; the cross-version step needs a second image digest *and* a second from-source build against that image's headers. |
| Container-backed suites | **BLOCKED on publication.** Every Docker build here uses the lane root as its context, so `crates/money-core` is not in the image and the injected path patch cannot reach it. `gate-pg` and `gate-pg-release` wait for `kamu-money-core` on crates.io. |
| RPO/RTO, backup scheduling, rollback authority, incident response | **NOT THIS REPOSITORY'S.** They belong to the platform consuming the artefact. |
| Registry publication of the node image | **STRUCK, not deferred.** The node image is a **test fixture**, not a deliverable: consumers build from source at a version and own their own artefact identity. Publishing it would create a distribution channel nobody consumes and an implicit support claim for bytes nobody is meant to run. Recorded rather than deleted, because three reviews raised it in good faith and a fourth would too. |
