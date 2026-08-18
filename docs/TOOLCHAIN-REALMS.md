# Toolchain realms

The pins have one home, `.config/dev-tools.json`, and two readings. A developer
machine is long-lived and already carries tools; a CI runner is created for one
job and destroyed.

## What a pin means in each realm

| | developer machine | CI runner |
| --- | --- | --- |
| floor-class tool | any version at or above the pin | the pinned version |
| exact-class tool | the pinned version | the pinned version |
| where it comes from | the host, else `.tools/bin` | the pinned installer action |

A pin is a floor unless its entry states why it must be exact. Two reasons
qualify: the tool's output is the verdict, so a machine above the floor passes
locally and fails in CI; or the version is load-bearing elsewhere, as
`cargo-pgrx` must equal the `pgrx` the extension builds against. The reason
lives in the entry, not here.

## Resolution order

Recipes, `just doctor` and `just setup` resolve the host's `PATH` first, then
`.tools/bin` and `node_modules/.bin`.

> A host copy that does **not** answer its pin still comes first, and shadows
> anything `just setup` installs.

`just setup` therefore refuses to install a Cargo tool beneath one, naming the
path to upgrade or remove. A Node tool is reported rather than skipped, because
npm installs the whole tree in one command. Doctor's marker says which copy
answered: `•` host, `✓` repository-local; a failing row names the path.

## What installs what

`just setup` installs the toolchains and their components, the repository-local
Cargo tools, and the Node tools. It does not install operating-system packages.

CI installs each pinned tool through the installer action the manifest is
indexed into, at the exact pinned version.

PostgreSQL provisioning for the extension lane belongs to the lane's container
image, which installs the server and its development headers and initialises
`PGRX_HOME` against them.

## The builder image

The reusable half of that image is its own build stage, published so it can be
pulled rather than reproduced. The one repository-specific part is `/work/.pgrx`,
which an extension built elsewhere ignores.

Its tag is derived from the repository inputs that decide it: an unchanged input
names a tag that already exists, a changed one names a tag that never has.

### The tag is discovery; the digest is identity

A tag does not close over everything that decides the bytes — the apt indexes
resolved while the image builds, the signing key fetched at build time, or a
deliberate `rebuild`. Two builds sharing a tag can differ, so a consumer that
needs a fixed image pins the digest. Each publish prints it and exposes it as a
job output. The base image is pinned the same way.

Publishing happens from the default branch, so a change to the image and the
jobs that consume it cannot land in the same run.

## Adding a tool

1. Add an entry to `.config/dev-tools.json` under the section that owns how it
   is installed. State `exact` only with the reason.
2. Name it in the request that installs it, indexing the version out of the
   published manifest.

`tools/repo-policy` fails the build if a version literal appears in anything
Actions executes, if a request reads another tool's entry, or if a manifest path
no key answers is indexed.
