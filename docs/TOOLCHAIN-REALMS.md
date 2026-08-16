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

`just setup` therefore refuses to install beneath one. When the host carries a
tool that misses its pin, setup names the path and asks for that copy to be
upgraded or removed, rather than installing a binary no recipe would reach.
Doctor's marker says which copy answered: `•` for the host, `✓` for a
repository-local one.

## What installs what

`just setup` installs the toolchains and their components, the repository-local
Cargo tools, and the Node tools. It does not install operating-system packages;
doctor names the version to install when one is missing.

CI installs each pinned tool through the installer action the manifest is
indexed into, at the exact pinned version. A runner is fresh, so exactness there
costs nothing and buys a reproducible run.

PostgreSQL provisioning for the extension lane belongs to the lane's own
container image, which installs the server and its development headers and
initialises `PGRX_HOME` against them. A job that provisions PostgreSQL by hand
is re-implementing that image.

## The builder image

The reusable half of that image — a toolchain, `cargo-pgrx`, one PostgreSQL
major with its development headers, and a `PGRX_HOME` initialised against them —
is its own build stage and is published under a name that says what it is. It
carries nothing specific to this repository, so anyone building a pgrx extension
can pull it instead of reproducing the provisioning.

**When it is rebuilt is not a schedule.** Its tag is derived from the inputs that
decide it, so the question does not arise: an unchanged input names a tag that
already exists and nothing is built, and a changed input names a tag that has
never existed and it is built. There is no window in which a published image is
stale, because no tag ever changes meaning.

That holds only if every input is visible as a change. The base image is
therefore pinned by digest as well as by tag: an upstream rebuild for a
distribution security patch produces different bytes under the same tag, and
without the digest nothing in this repository would move when it did.

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
