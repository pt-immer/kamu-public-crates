# kamu-iso3166

[![Crates.io][badge-crates]][link-crates]
[![docs.rs][badge-docs]][link-docs]
[![CI][badge-ci]][link-ci]

[![License][badge-license]][link-license]
[![MSRV][badge-msrv]][link-msrv]

Zero-allocation, `no_std`-compatible ISO 3166-1 and ISO 3166-2 primitives for
the Rust ecosystem.

Part of the [`kamu-public-crates`](https://github.com/pt-immer/kamu-public-crates) workspace.

## Scope

- **ISO 3166-1** (module [`country`]): `Alpha2` enum, `Alpha3` enum,
  `Numeric(u16)` newtype, checked parsing, and lossless conversion among
  assigned codes.
- **ISO 3166-2** (module [`subdivision`]): `Subdivision` entries keyed by parent
  country, plus a generated `Category` enum.
- **ISO 3166-3**: out of scope.

## Features

| Feature | Default | Description                                      |
| ------- | ------- | ------------------------------------------------ |
| `std`   | yes     | Enables standard-library support in dependencies. |
| `serde` | no      | Implements serde traits for all public types.     |

Disable default features for strict `no_std`:

```toml
kamu-iso3166 = { version = "0.5", default-features = false }
```

## Example

```rust
use kamu_iso3166::{Alpha2, Numeric, Subdivision};

let id = Alpha2::ID;
assert_eq!(id.as_str(), "ID");
assert_eq!(id.to_alpha3().as_str(), "IDN");
assert_eq!(id.to_numeric(), Numeric::try_from(360u16).unwrap());

// Case-insensitive, zero-allocation parsing.
let jakarta = Subdivision::try_from_str("id-jk").unwrap();
assert_eq!(jakarta.parent, Alpha2::ID);

// Iterate every assigned country.
assert!(Alpha2::iter().count() >= 240);
```

## Serde representation

Enable `serde` without requiring `std` or an allocator:

```toml
kamu-iso3166 = { version = "0.5", default-features = false, features = ["serde"] }
```

The canonical serialized forms are:

| Type          | Form             | Example      |
| ------------- | ---------------- | ------------ |
| `Alpha2`      | string           | `"ID"`       |
| `Alpha3`      | string           | `"IDN"`      |
| `Numeric`     | unsigned integer | `360`        |
| `Category`    | string           | `"PROVINCE"` |
| `Subdivision` | code string      | `"ID-JK"`    |

Deserialization validates each value against the pinned ISO dataset.

## Dependencies

Runtime dependencies are kept minimal and none require the allocator:

- [`phf`](https://crates.io/crates/phf) — perfect-hash lookup for parsing.
- [`thiserror`](https://crates.io/crates/thiserror) — ergonomic error types
  (`no_std` mode).

## MSRV

Rust **1.94** (the workspace MSRV; the crate itself only needs
`core::error::Error` in `no_std`, stable since 1.81).

## Licensing

- Crate source code: **`MIT OR Apache-2.0`** (see [`LICENSE-MIT`](LICENSE-MIT)
  and [`LICENSE-APACHE`](LICENSE-APACHE)).
- Embedded ISO 3166 data: **CC BY-SA 4.0**, vendored from
  [`ipregistry/iso3166`](https://github.com/ipregistry/iso3166) at a pinned
  commit. See [`NOTICE`](NOTICE) and [`VENDORED.md`](VENDORED.md).

Required attribution when redistributing the compiled data:

> This site or product includes Ipregistry ISO 3166 data available from
> <https://ipregistry.co>.

## Building from source

The ISO 3166 data is attached as a git submodule. After cloning the workspace:

```sh
git clone --recurse-submodules https://github.com/pt-immer/kamu-public-crates.git
# or, after a plain clone:
git submodule update --init --recursive

cargo build -p kamu-iso3166
```

Published crate tarballs include the CSV files directly, so downstream consumers
building from `crates.io` do **not** need submodule access.

[`country`]: https://docs.rs/kamu-iso3166/latest/kamu_iso3166/country/
[`subdivision`]: https://docs.rs/kamu-iso3166/latest/kamu_iso3166/subdivision/

[badge-crates]: https://img.shields.io/crates/v/kamu-iso3166?style=flat-square&logo=rust
[badge-docs]: https://img.shields.io/docsrs/kamu-iso3166?style=flat-square&logo=docs.rs&label=docs.rs
[badge-ci]: https://img.shields.io/github/actions/workflow/status/pt-immer/kamu-public-crates/on-pr-synced.yml?branch=main&style=flat-square&label=CI
[badge-license]: https://img.shields.io/crates/l/kamu-iso3166?style=flat-square
[badge-msrv]: https://img.shields.io/badge/MSRV-1.94-blue?style=flat-square&logo=rust

[link-crates]: https://crates.io/crates/kamu-iso3166
[link-docs]: https://docs.rs/kamu-iso3166
[link-ci]: https://github.com/pt-immer/kamu-public-crates/actions/workflows/on-pr-synced.yml
[link-license]: https://github.com/pt-immer/kamu-public-crates/blob/main/crates/iso3166
[link-msrv]: https://github.com/pt-immer/kamu-public-crates/blob/main/Cargo.toml
