# kamu-iso3166

[![CI](https://github.com/pt-immer/kamu-public-crates/actions/workflows/on-release-published.yml/badge.svg)](https://github.com/pt-immer/kamu-public-crates/actions/workflows/on-release-published.yml)
[![crates.io](https://img.shields.io/crates/v/kamu-iso3166.svg)](https://crates.io/crates/kamu-iso3166)
[![docs.rs](https://img.shields.io/docsrs/kamu-iso3166)](https://docs.rs/kamu-iso3166)

Zero-allocation, `no_std`-compatible ISO 3166-1 and ISO 3166-2 primitives for
the Rust ecosystem.

Part of the [`kamu-libs`](https://github.com/pt-immer/kamu-public-crates) workspace.

## Scope

- **ISO 3166-1** (module [`country`]): `Alpha2` enum, `Alpha3` enum,
  `Numeric(u16)` newtype, with total infallible conversions between them.
- **ISO 3166-2** (module [`subdivision`]): `Subdivision` entries keyed by parent
  country, plus a generated `Category` enum.
- **ISO 3166-3**: *out of scope*; planned for a later release.

> The pre-0.2.0 module names `one` / `two` remain available as deprecated
> aliases of `country` / `subdivision` and will be removed in a future release.

## Features

| Feature | Default | Description                                               |
| ------- | ------- | --------------------------------------------------------- |
| `std`   | yes     | Enables `std::error::Error` integrations.                 |
| `alloc` | no      | Reserved for future API surfaces accepting owned strings. |
| `serde` | no      | `Serialize`/`Deserialize` for all public types.           |

Disable default features for strict `no_std`:

```toml
kamu-iso3166 = { version = "0.2", default-features = false }
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

## Dependencies

Runtime dependencies are kept minimal and none require the allocator:

- [`phf`](https://crates.io/crates/phf) — perfect-hash lookup for parsing.
- [`thiserror`](https://crates.io/crates/thiserror) — ergonomic error types
  (`no_std` mode).

## MSRV

Rust **1.85** (the workspace MSRV; required for `core::error::Error` in
`no_std`).

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
