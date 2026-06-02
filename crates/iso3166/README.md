# kamu-iso3166

Zero-allocation, `no_std`-compatible ISO 3166-1 and ISO 3166-2 primitives for
the Rust ecosystem.

## Status

Pre-release (`0.1.0` in development). API subject to change.

## Scope (v0.1)

- **ISO 3166-1**: `Alpha2` enum, `Alpha3` enum, `Numeric(u16)` newtype.
- **ISO 3166-2**: subdivisions keyed by parent country.
- **ISO 3166-3**: *out of scope for v0.1*; planned for a later release.

## Features

| Feature | Default | Description                                              |
| ------- | ------- | -------------------------------------------------------- |
| `std`   | yes     | Enables `std::error::Error` integrations.                |
| `alloc` | no      | Reserved for future API surfaces accepting owned strings.|
| `serde` | no      | `Serialize`/`Deserialize` for all public types.          |

Disable default features for strict `no_std`:

```toml
kamu-iso3166 = { version = "0.1", default-features = false }
```

## Dependencies

Runtime dependencies are kept minimal and none require the allocator:

- [`phf`](https://crates.io/crates/phf) — perfect-hash lookup for parsing.
- [`thiserror`](https://crates.io/crates/thiserror) — ergonomic error types
  (`no_std` mode).

## MSRV

Rust **1.85** (required for `core::error::Error` in `no_std`).

## Licensing

- Crate source code: **Apache-2.0** (see [`LICENSE`](LICENSE)).
- Embedded ISO 3166 data: **CC BY-SA 4.0**, vendored from
  [`ipregistry/iso3166`](https://github.com/ipregistry/iso3166) at a pinned
  commit. See [`NOTICE`](NOTICE) and [`VENDORED.md`](VENDORED.md).

Required attribution when redistributing the compiled data:

> This site or product includes Ipregistry ISO 3166 data available from
> <https://ipregistry.co>.

## Building from source

The ISO 3166 data is attached as a git submodule. After cloning:

```sh
git submodule update --init --recursive
cargo build
```

Published crate tarballs include the CSV files directly; downstream consumers
building from `crates.io` do **not** need submodule access.
