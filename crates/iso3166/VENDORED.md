# Vendored data

This crate embeds ISO 3166 data from an external source. This document records
the provenance and licensing of that data.

## Upstream

- **Repository**: <https://github.com/ipregistry/iso3166>
- **Pinned commit**: `1224d32fecbec52b21dc5b18e327fa9c09cb1c92`
- **Fetch date**: 2026-04-21
- **License**: Creative Commons Attribution-ShareAlike 4.0 International
  (CC BY-SA 4.0) — <https://creativecommons.org/licenses/by-sa/4.0/>

## Vendoring mechanism

The upstream repository is attached as a **git submodule** at
`vendor/iso3166-csv/`. After cloning this repository, initialize the submodule:

```sh
git submodule update --init --recursive
```

Published crate tarballs include a tracked snapshot of the consumed CSV files
so that `cargo publish` consumers do not require submodule access. The list
of packaged files is declared via the `include = [...]` field in
`Cargo.toml`.

## Files consumed by `build.rs`

| File                                    | Role                                    |
| --------------------------------------- | --------------------------------------- |
| `vendor/iso3166-csv/countries.csv`      | ISO 3166-1 (alpha2, alpha3, numeric)    |
| `vendor/iso3166-csv/subdivisions.csv`   | ISO 3166-2 (subdivision codes & names)  |

## Files present upstream but **not** consumed in v0.1

- `administrative-languages.csv`
- `countries-sovereignty.csv`

These may be consumed in a future version.

## Schema notes (for codegen)

### `countries.csv` (250 rows + header)

Columns: `country_code_alpha2`, `country_code_alpha3`, `numeric_code`,
`name_short`, `name_long`. Country names are in English.

### `subdivisions.csv` (6261 rows + header)

Columns: `country_code_alpha2`, `subdivision_code_iso3166-2`,
`subdivision_name`, `language_code`, `parent_subdivision`, `category`,
`localVariant`. Each subdivision appears on exactly one row; the
`language_code` identifies the language the `subdivision_name` is written in
(most subdivisions do **not** have English names available — only ~23% use
`en`). The crate therefore exposes subdivision names in their upstream-provided
language (documented on the public accessor).

Distinct `category` values: ~100. The `Category` enum is generated with
`#[non_exhaustive]` + named variants for every value present at the pinned
commit + an `Other(&'static str)` fallback for forward compatibility.

## Checksums

SHA-256 of files consumed at the pinned commit:

| File              | SHA-256                                                            |
| ----------------- | ------------------------------------------------------------------ |
| `countries.csv`   | `037ff5b81cd1fb9652ea92e51b2db7988cd730dc466c3c3139aa31b75b051e7b` |
| `subdivisions.csv`| `31b707040dbfef0652701f8c5d7b275981a978bf9328ad684d9e646aeb946f4f` |

## Attribution requirement

Per the upstream README, any redistribution (including this crate) must
include the following notice:

> This site or product includes Ipregistry ISO 3166 data available from
> <https://ipregistry.co>.

This notice is present in the top-level `NOTICE` file and the crate README.

## Updating the pinned commit

1. `cd vendor/iso3166-csv && git fetch && git checkout <new-sha> && cd ../..`
2. `git add vendor/iso3166-csv`
3. Update the **Pinned commit**, **Fetch date**, and checksum rows above.
4. Re-run `cargo build` and full test suite.
5. Commit with a message referencing the upstream SHA change.
