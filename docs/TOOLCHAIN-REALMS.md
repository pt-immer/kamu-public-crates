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
locally and fails in CI; or another tool requires that exact version. The reason
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
