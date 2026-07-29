# kamu-money-pg — extension lane contract

`extensions/money-pg` is an excluded Cargo workspace containing the `kmoney`
pgrx extension and its database validation harness. It implements the
PostgreSQL side of
[`kamu-money-core` C8](../../crates/money-core/DESIGN.md#c8--postgresql-boundary).

Current support:

- PostgreSQL 15, 16, 17, and 18;
- YugabyteDB at the digest recorded in
  [`YB-PINNED.txt`](kamu-money-pg/yb/YB-PINNED.txt);
- Edition 2024 and Rust 1.96;
- pgrx 0.19.1 through the `pt-immer/pgrx-yugabytedb` fork.

## Workspace boundary

```text
extensions/money-pg/
├── Cargo.toml             nested workspace, patch, profiles, MSRV
├── Cargo.lock             lane-only dependency graph
├── deny.toml              lane-only advisory and license policy
├── hygiene/               pgrx-free structural guards and payload tests
└── kamu-money-pg/         pgrx cdylib, SQL cases, Docker and YB harness
```

The lane is excluded from the repository root because Cargo honors
`[patch.crates-io]` and profiles only at a workspace root. Adding the pgrx fork
to the public workspace would also put git dependencies and PostgreSQL backend
features into the lockfile and audit surface of nine unrelated crates.

`kamu-money-core` remains a version dependency, never a manifest path.
Developer and ordinary container tests inject the local normalized package.
`gate-pg-release` disables that patch and proves registry resolution.

The lane is `publish = false`. `kamu-money-pg` is a package and release
identity, but not a crates.io artifact: Cargo packages do not carry the root
pgrx patch needed by its `yb-pg15` feature.

## Names

| Surface | Name | Reason |
| --- | --- | --- |
| Cargo package and directory | `kamu-money-pg` | Repository and release identity |
| Control file and SQL extension | `kmoney` | Unquoted PostgreSQL identifier |
| Library, shared object, `module_pathname` | `kmoney` | One SQL-side artifact name |
| Strict SQL type | `kmoney('USD')` | Currency pinned through typmod |
| Heterogeneous SQL type | `kmoney_mixed` | Currency carried per value; no arithmetic |

The control-file stem determines the extension name. Treat SQL type names and
the 18-byte storage layout as migration-sensitive public interfaces.

## Layering

```text
PostgreSQL / pgrx ABI
        |
        v
src/ffi/       raw datum access, memory contexts, C-visible symbols
        |
        v
src/safe/      payload, validation, typmod, rendering, operations
        |
        v
kamu-money-core exact arithmetic and canonical text
```

All unsafe syntax is confined to `src/ffi/` and enforced by a Syn-based hygiene
test. `src/safe/` owns semantics and accepts ordinary values or fixed byte
arrays, not unproved raw pointers.

## Payload and unsafe warranty

Both SQL types use the same fixed payload:

```text
bytes  0..16   signed i128 canonical units, little-endian
bytes 16..18   ISO 4217 numeric code, little-endian
```

The Rust payload is byte-backed and has alignment 1. It does not create an
`&i128` from PostgreSQL memory, whose alignment is weaker than Rust requires for
`i128`.

Before semantic use, one validator checks:

- exact payload width;
- assigned ISO numeric code;
- expected typmod currency where one exists;
- canonical money domain.

The ABI warranty is intentionally narrow:

- catalog entries for both SQL types report `typlen = 18`, pass-by-reference,
  byte alignment, and plain storage;
- pgrx calls the conversion traits only for their registered OIDs;
- non-null scalar and array datums expose the registered fixed width;
- FFI allocation uses the active PostgreSQL memory context;
- cleanup that must survive a PostgreSQL error does not depend on Rust
  destructors running after pgrx translates the error.

Miri proves only the safe payload module. Live catalog, scalar, array, binary,
corruption, PostgreSQL-major, and YugabyteDB tests cover the foreign ABI that
Miri cannot model.

## SQL semantics

- `kmoney('USD')` checks its currency at input/coercion and again in each
  operation, because PostgreSQL does not pass typmod to operators.
- `kmoney_mixed` supports equality and checked conversion to a named currency.
  It has no ordering, arithmetic, or `sum`, so unsupported computation fails at
  planning rather than after reading rows.
- `sum(kmoney)` uses a 256-bit transition state and checks the money domain only
  when finalizing. Partial aggregation therefore remains order-independent.
- Division returns quotient and residue together.
- Allocation borrows the pgrx array, checks its length before iterating, and
  caps the number of parts before materializing weights.
- Text input/output, arithmetic, allocation, and stable hashing delegate to
  `kamu-money-core`.
- Binary send/receive use exactly the validated 18-byte payload.

There is deliberately no cast to PostgreSQL `numeric`.

## Verification ladder

| Layer | Command or suite | Claim |
| --- | --- | --- |
| Formatting, Clippy, docs, deny | `just pg gate-offline` | Rust and repository policy without a database |
| Safe payload | hygiene tests and Miri | Width, byte order, code, currency, domain |
| PostgreSQL majors | `just pg test-pg-all` | Catalog, SQL semantics, binary I/O on PG15–18 |
| Portable driver path | `just pg test-pg-driver` | postgres-types and sqlx against native columns |
| YB image controls | `just pg yb-image-selftest` | Unknown and moved tags fail closed |
| Stock/YB equivalence | `just pg yb-ab` | Same SQL cases and golden output |
| Cluster behavior | release gate suites | Every node, tablet split, concurrency, replica, restore |
| Developer lane gate | `just gate-pg` | Offline checks, PG15–18, and portable database adapters |
| Repository pre-push gate | `just gate-all` | Public workspace plus developer lane gate |
| Native YB release proof | `just pg gate-pg-release` | From-source native build and all cluster suites |

[`kamu-money-pg/tests/pg_regress/COVERAGE.md`](kamu-money-pg/tests/pg_regress/COVERAGE.md)
maps every `#[pg_test]` to a portable SQL case or a reasoned
`NOT-PORTABLE` entry. Hygiene tests check both directions, required files,
golden labels, and orphan cases.

The release gate resolves one immutable YugabyteDB base image, builds one node
image, extracts the artifact from that image, and passes the same identities to
every suite. It also checks shipped bytes for benchmark-only probe symbols.

pgrx's raw generated SQL is not byte-reproducible because object order and
source-position comments can change. `schema-hash` strips provenance comments,
sorts objects, and compares the normalized schema. Release proof extracts the
SQL from the built image instead of regenerating a lookalike.

## Supported build and release model

The pgrx fork is pinned by tag and lockfile. Its `yb-pg15` changes are
feature-gated; stock PostgreSQL builds use the same dependency without that
feature. A YugabyteDB image change must pass the header probe and full release
gate before its digest is recorded.

The node-image target is a validation fixture and a source pattern for
consumers. This repository does not publish a production container image.
Deploying organizations build and own their artifact identity.

A GitHub release may use `kamu-money-pg-vX.Y.Z`. The release workflow verifies
the manifest and stops before crates.io.

## Known limits

- The `pt-immer/pgrx-yugabytedb` fork is maintained by this project.
- Certified against `yugabytedb/yugabyte:2025.2.5.1-b1`, the pinned image every
  suite boots. Other versions are not certified rather than known-broken: any
  YugabyteDB whose PostgreSQL fork the pgrx fork supports should work, and
  re-pinning plus a green `just pg gate-pg-release` is what turns that into a
  claim.
- Same-version dump/restore is gated. A two-version rolling YugabyteDB upgrade
  has not been rehearsed.
- Managed platforms that reject third-party native extensions require the
  canonical-text adapter instead.
- RPO, RTO, backup cadence, rollout authority, and incident response belong to
  the consuming platform.
- Benchmarks are comparative signals, not machine-independent pass/fail
  thresholds.

See the [YugabyteDB runbook](kamu-money-pg/yb/RUNBOOK.md) for adoption,
diagnosis, deployment order, and rollback cutoffs.
