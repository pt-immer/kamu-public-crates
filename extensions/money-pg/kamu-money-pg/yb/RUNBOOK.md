# `kmoney` on YugabyteDB — operator runbook

This runbook covers YugabyteDB image adoption, pgrx-fork updates, diagnosis,
deployment order, and rollback. Commands run from the repository root.

YugabyteDB can host pgrx extensions, but `kmoney` is not a vendor-supported
extension. The `yb-pg15` adaptations are maintained by this project. Managed
platforms that reject third-party native extensions must use the canonical-text
adapter described in [Fallback](#fallback-to-canonical-text).

## Standing support condition

The supported native combination is defined by four pins:

| Pin | Source of truth |
| --- | --- |
| YugabyteDB image | [`YB-PINNED.txt`](YB-PINNED.txt), tag plus registry digest |
| pgrx fork | lane-root `Cargo.toml` `[patch.crates-io]` |
| pgrx version | `kamu-money-pg/Cargo.toml` and Docker build arguments |
| Rust version | lane-root `rust-toolchain.toml` and `workspace.package.rust-version` |

The fork is
[`pt-immer/pgrx-yugabytedb`](https://github.com/pt-immer/pgrx-yugabytedb),
tagged `v0.19.1-yb.1` for the current pgrx 0.19.1 base. Its `yb-pg15` feature
adapts three YugabyteDB differences:

| YugabyteDB difference | Fork adaptation |
| --- | --- |
| YSQL memory context is thread-local | Alias to `YbCurrentMemoryContext` |
| `index_build_range_scan` has three additional parameters | Supply the YB arguments at generated call sites |
| `BackgroundWorker` has `bgw_oom_score_adj` | Initialize the additional field |

`probe-yb-abi.sh` checks those assumptions against the selected image's headers
before compilation. The Rust compiler then verifies that the fork still builds.

`yb-image.sh` resolves tags through the registry and fails closed:

- `YB_ALLOW_UNPINNED=1` permits evaluation of a tag absent from the pin file;
- `YB_ALLOW_DRIFT=1` permits evaluation when a recorded tag resolves to a new
  digest.

These variables are investigation overrides, not deployment settings. Neither
changes `YB-PINNED.txt`.

## Adopt a YugabyteDB image

Do not edit the pin first. The unrecorded or changed digest must fail before an
override proves the evaluation path is active.

```bash
NEW='yugabytedb/yugabyte:<new-tag>'
RESOLVER='extensions/money-pg/kamu-money-pg/yb/yb-image.sh'

# Expected to refuse. Record the immutable digest printed in the diagnostic.
YB_PULL=1 "$RESOLVER" "$NEW"

# For a new tag:
YB_ALLOW_UNPINNED=1 just pg yb-build "$NEW"
YB_ALLOW_UNPINNED=1 just pg gate-pg-release 4 "$NEW"

# For a recorded tag whose digest moved, use YB_ALLOW_DRIFT=1 instead.
```

The release gate:

1. resolves one immutable base image;
2. disables the local `kamu-money-core` Cargo patch;
3. runs the offline and PostgreSQL 15–18 gates;
4. builds one node image and extracts the artifact from it;
5. compares YugabyteDB with stock PostgreSQL 15;
6. runs portable cases, three-node behavior, concurrent transfers, a read
   replica, and same-version dump/restore;
7. refuses benchmark-only symbols in the shipped artifact.

If the gate passes:

1. replace or add the exact `tag<TAB>digest` row in `YB-PINNED.txt`;
2. rerun without an override:

   ```bash
   YB_PULL=1 just pg gate-pg-release 4 "$NEW"
   ```

3. update this runbook only if the support condition or procedure changed;
4. commit the pin and any required code changes together.

If the build or gate fails, keep the old pin. Re-derive the fork adaptation or
use the text fallback. Do not weaken the header probe to make an unknown image
pass.

## Update pgrx or the fork

A fork change is released in the fork repository first:

```bash
git checkout -b yugabytedb-<version> v<version>
# Apply the yb-pg15 changes behind cfg(feature = "yb-pg15").
git tag -s v<version>-yb.1
git push origin yugabytedb-<version> v<version>-yb.1
```

Then update this repository:

1. change both `[patch.crates-io]` entries in
   `extensions/money-pg/Cargo.toml`;
2. change pgrx and pgrx-test requirements in
   `extensions/money-pg/kamu-money-pg/Cargo.toml`;
3. update every Docker `cargo-pgrx` pin;
4. review every remaining version-specific comment or check:

   ```bash
   rg '0\.19\.1|v0\.19\.1-yb\.1' extensions/money-pg
   ```

5. refresh the lane lockfile;
6. run:

   ```bash
   just pg setup
   just pg doctor
   just gate-all
   ```

`cargo-pgrx` must exactly match the pgrx dependency version.

For an unpushed fork revision, a local checkout may temporarily live at
`extensions/money-pg/vendor/pgrx-yugabytedb`; the lane excludes `vendor` from
workspace discovery. Never commit a path patch as the release configuration.

## Diagnose a failure

| Symptom | Meaning and action |
| --- | --- |
| `tag has moved off the validated digest` | Registry content changed. Follow image adoption with `YB_ALLOW_DRIFT=1`; do not change the pin before the gate passes. |
| `tag is not recorded in the pin file` | No supported digest exists. Follow image adoption with `YB_ALLOW_UNPINNED=1`. |
| `probe-yb-abi: FAILED` | A header assumption changed. Re-derive the relevant fork adaptation. |
| Compile error in `pgrx` or `pgrx-pg-sys` | The fork no longer applies to the chosen pgrx/YB combination. Update the fork; do not patch generated source in a container. |
| `INCOHERENT TRIPLET` or `MANIFEST MISMATCH` | Shared object, control file, and SQL came from different builds. Delete that run's private output and rebuild as one artifact. |
| `could not access file "$libdir/kmoney"` on one node | That node lacks the extension library. Roll the same image to every tserver, including read replicas. |
| Stock PG15 and YugabyteDB fail the same case | Extension or portable-case defect. Start with `just pg test-pg 15`. |
| Stock PG15 passes and YugabyteDB fails | YugabyteDB divergence. Use the named case and golden diff to isolate the contract. |

Useful diagnostics:

```bash
just pg yb-pin-check
just pg yb-image-selftest
just pg yb-native
just pg test-yb-regress
docker ps -a --filter 'label=kamu-money-pg.revision'
```

The final command is read-only. Cleanup scripts target only containers and
networks labeled by this lane.

## Fallback to canonical text

If native loading is unavailable, enable `kamu-money-core`'s `postgres` or
`sqlx` feature and store canonical strings such as `USD 10.50` in a PostgreSQL
`text` column.

The fallback retains:

- the same parser, renderer, domain, and currency register;
- exact scalar and array round trips;
- compatibility with PostgreSQL and YugabyteDB without a native extension.

It gives up:

- database arithmetic, allocation, and `sum(kmoney)`;
- `kmoney('USD')` typmod enforcement;
- the fixed 18-byte native representation.

This is a mechanism migration, not a value-format migration. The native
input/output functions and portable adapters share the canonical codec.

## Deploy

This repository does not publish a production node image. A consuming platform
builds the node-image target from a signed source revision, pushes it to its own
registry, and deploys by registry digest.

The database order is mandatory:

1. Build and pass `just pg gate-pg-release` against the exact base digest.
2. Publish the consumer-owned node image and record its immutable digest.
3. Roll that image to every YSQL tserver and read replica.
4. Confirm every node is on the intended digest.
5. Run `CREATE EXTENSION kmoney` for first install, or
   `ALTER EXTENSION kmoney UPDATE` for an upgrade, once per cluster.
6. Recycle long-lived database connections so every backend loads the new
   library.

Binary first, catalog second. During a rolling image update, new binaries must
remain compatible with the current catalog until step 5 completes.

### Roll back

Before the catalog update, roll back by redeploying the previous image digest.

After `ALTER EXTENSION UPDATE`, do not deploy the old binary unless an explicit
downgrade script and compatibility proof exist. The old library may not match
the new catalog. Treat the catalog update as the rollback cutoff.

Keep the previous image digest deployable until the new rollout, catalog update,
connection recycle, and smoke tests are complete.

### Restore

`just pg test-yb-restore` proves same-version dump/restore into a clean cluster.
The target must already run an image containing `kmoney`; PostgreSQL dumps
`CREATE EXTENSION`, not the extension's member objects or shared library.

This test does not define backup cadence, RPO, RTO, retention, or operator
authority. The consuming platform owns those controls.

### Extension version changes

Any version bump beyond `0.1.0` must add the corresponding
`kmoney--old--new.sql` upgrade script. PostgreSQL refuses
`ALTER EXTENSION UPDATE` without a valid upgrade path.

A change to the 18-byte payload is a storage migration, not an ordinary release.
It requires explicit old/new decoding, upgrade and rollback plans, and
cross-version tests.

## Evidence limits

- One YugabyteDB digest is supported at a time.
- Same-version restart, node failure/rejoin, and dump/restore are tested.
- A two-version rolling YugabyteDB upgrade is not yet rehearsed.
- The node image produced here is a test fixture, not a published deliverable.
- Benchmarks compare mechanisms on one host; they are not portable thresholds.
- Native extension support does not cover managed services that prohibit custom
  shared libraries.
