# Toolchain realms

A tool reaches this repository two ways, and the two are allowed to differ.

A **developer machine** is long-lived, shared with every other project on it, and
already carries tools. A **CI runner** is created for one job and destroyed after
it. Asking both to provision identically means one of them does unnecessary work:
either the runner rebuilds what a machine already had, or setup overwrites a
machine's working tool with a byte-identical copy under a different path.

So the pins have one home — `.config/dev-tools.json` — and two readings.

## What a pin means in each realm

| | developer machine | CI runner |
| --- | --- | --- |
| floor-class tool | any version at or above the pin | the pinned version |
| exact-class tool | the pinned version | the pinned version |
| where it comes from | the host, else `.tools/bin` | the pinned installer action |

A pin is a **floor** by default: a newer `just` or `cargo-nextest` runs the same
recipes and the same tests, so requiring an exact patch would churn a machine for
nothing.

A pin is **exact** where the entry says why. Two reasons qualify:

- **The tool's output is the verdict.** A formatter or a linter decides whether
  the gate passes. A different version formats differently or carries a
  different rule set, so a machine above the floor passes locally and fails in
  CI — the worst failure a contributor can be handed, because the diff that
  broke it is not theirs.
- **The version is load-bearing elsewhere.** `cargo-pgrx` must equal the `pgrx`
  the extension builds against or the extension does not link.

The reason is stated in the entry, not here, so this page cannot go stale
against it. A security advisory that forces an exact version is the same
mechanism with a different reason.

## Resolution order

Recipes, `just doctor` and `just setup` all resolve a tool as: the host's `PATH`
first, then `.tools/bin` and `node_modules/.bin`. The `Justfile` exports that
order, and doctor searches the same one — otherwise it reports on a binary no
recipe would run.

The consequence is worth stating plainly, because it is not obvious:

> A host copy that does **not** answer its pin still comes first, and shadows
> anything `just setup` installs.

`just setup` therefore refuses to install a Cargo tool beneath one. When the host
carries a copy that misses its pin, setup names the path and asks for that copy
to be upgraded or removed, rather than installing a binary no recipe would
reach. A Node tool is reported rather than skipped, because npm installs the
whole tree in one command and cannot leave one package out; the install below
`node_modules/.bin` succeeds and no recipe reaches it either. Doctor's marker
says which copy answered: `•` for the host, `✓` for a repository-local one, and
a failing row names the path it judged.

## What installs what

`just setup` installs the toolchains and their components, the repository-local
Cargo tools, and the Node tools. It does not install operating-system packages;
doctor names the version to install when one is missing.

CI installs each pinned tool through the installer action the manifest is
indexed into, at the exact pinned version. A runner is fresh, so exactness there
costs nothing and buys a reproducible run.

PostgreSQL provisioning for the extension lane belongs to the lane's own
container image, which installs the server and its development headers and
initialises `PGRX_HOME` against them. Two lane jobs still provision it by hand —
an apt line, a `pg_config` path, a `PGRX_HOME` initialisation — and each states
the major a third time. Retiring that is what the builder image below is for.
They move once it is published, which cannot happen in the same change.

## The builder image

The reusable half of that image — a toolchain, `cargo-pgrx`, one PostgreSQL
major with its development headers, and a `PGRX_HOME` initialised against them —
is its own build stage and is published under a name that says what it is.
Almost nothing in it is specific to this repository: it creates `/work/.pgrx`,
because this repository's Cargo configuration forces `PGRX_HOME` there, and an
extension built elsewhere simply ignores that directory.

**When it is rebuilt is not a schedule.** Its tag is derived from the repository
inputs that decide it, so the question does not arise: an unchanged input names
a tag that already exists and nothing is built, and a changed input names a tag
that has never existed and it is built.

### The tag is discovery; the digest is identity

A tag closes over this repository's inputs. It does not close over everything
that decides the bytes:

- the apt indexes resolved while the image builds, and the PostgreSQL patch
  levels they carry;
- the signing key fetched over the network at build time;
- a deliberate `rebuild`, which republishes one tag on purpose.

So **two builds sharing a tag can differ**, and a consumer that needs a fixed
image pins the **digest**, which is immutable by construction. Each publish
prints the digest and exposes it as a job output.

The base image here is pinned that way for the same reason: an upstream rebuild
for a distribution security patch produces different bytes under an unchanged
tag, and without the digest nothing in this repository would move when it did.

Publishing happens from the default branch, so a change to the image and the
jobs that consume it cannot land in the same run — the image is published first,
and the consumers move once the tag exists.

## Adding a tool

1. Add an entry to `.config/dev-tools.json` under the section that owns how it
   is installed. State `exact` only with the reason it must be exact.
2. Name it in the request that installs it, indexing the version out of the
   published manifest rather than restating it.

Nothing else states a version. `tools/repo-policy` fails the build if a version
literal appears in anything Actions executes, if a request reads another tool's
entry, or if a manifest path no key answers is indexed.
