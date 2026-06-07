# `kamu-snap-crypto`

[![Crates.io][badge-crates]][link-crates]
[![docs.rs][badge-docs]][link-docs]
[![CI][badge-ci]][link-ci]

[![License][badge-license]][link-license]
[![MSRV][badge-msrv]][link-msrv]

Framework-agnostic cryptography for Bank Indonesia SNAP BI integrations.

Part of the [`kamu-public-crates`](https://github.com/pt-immer/kamu-public-crates) workspace.

## What this crate is

The cryptographic spine shared by every PT IMMER SNAP BI consumer (client and server). Exposes:

| Item | Purpose |
|---|---|
| `HmacSigner` | HMAC-SHA512 sign/verify, constant-time verify, `&self` (no Mutex) |
| `RsaSigner<S>` / `RsaVerifier<S>` | PKCS#8-only RSA, generic over a sealed `SignatureScheme` |
| `Pkcs1v15Sha256` / `Pkcs1v15Sha512` / `PssSha256` / `PssSha512` | Built-in schemes |
| `Signature` + `Encoding` | Encoding-agnostic raw signature bytes |
| `snap_bi` (feature `snap-bi`, default on) | Canonical SNAP BI recipes |
| `webhook` (feature `webhook`, default on) | Provider-extensible webhook verifier |

## 30-second quickstart

```rust
use kamu_snap_crypto::HmacSigner;

let signer = HmacSigner::new(b"shared-secret")?;
let sig = signer.sign(b"payload");
println!("{}", sig.to_base64());

signer.verify(&sig, b"payload")?; // constant-time
# Ok::<(), kamu_snap_crypto::Error>(())
```

For a full SNAP BI service-call signing flow including stringToSign,
timestamp, and headers, see `examples/sign_snap_bi_request.rs`.

## SNAP BI recipes (feature `snap-bi`)

```rust,no_run
use http::Method;
use kamu_snap_crypto::snap_bi::{ServiceStringToSign, now_jakarta_seconds, sign_service};

let method = Method::POST;
let timestamp = now_jakarta_seconds();
let parts = ServiceStringToSign {
    method: &method,
    path: "/snap/v1.0/transfer-intrabank/payment",
    access_token: "bearer-token",
    body: br#"{"partnerReferenceNo":"abc"}"#,
    timestamp: &timestamp,
};
let sig = sign_service(b"client-secret", &parts)?;
// sig is encoding-agnostic — pick base64 or hex at call site
println!("{}", sig.to_base64());
# Ok::<(), kamu_snap_crypto::Error>(())
```

## Webhook verification (feature `webhook`)

```rust,no_run
use http::HeaderMap;
use kamu_snap_crypto::webhook::{InacashCashoutVerifier, WebhookVerifier};

let verifier = InacashCashoutVerifier { secret: "shared-secret".into() };
let headers: HeaderMap = todo!();
let body: &[u8] = todo!();
verifier.verify(&headers, body)?;
# Ok::<(), kamu_snap_crypto::Error>(())
```

## Security guarantees

- `#![forbid(unsafe_code)]`. No `unsafe` block anywhere in this crate.
- **Constant-time verification**: HMAC uses `hmac::Mac::verify_slice`; RSA uses `rsa::signature::Verifier::verify`. Both are constant-time per upstream guarantees.
- **PKCS#8 enforcement**: PKCS#1 PEMs are rejected by upstream parsers. Convert with `openssl pkcs8 -topk8 -nocrypt -in pkcs1.pem -out pkcs8.pem`.
- **No HTTP framework coupling**: leaf crate. `wasm32-unknown-unknown` compiles require the consumer to enable `getrandom/js` (pulled in via `rsa`).

## Encodings

`Signature` is encoding-agnostic. Pick at the call site:

```rust
# use kamu_snap_crypto::{HmacSigner, Encoding};
# let signer = HmacSigner::new(b"x").unwrap();
let sig = signer.sign(b"hi");
sig.to_base64();
sig.to_hex_lower();
sig.to_base64_url_nopad();
sig.encode(Encoding::Base64);
```

Decoders mirror via `Signature::from_base64`, `from_hex`, `from_base64_url_nopad`, and the `Signature::decode(s, enc)` dispatcher.

## Feature flags

| Feature | Default | Purpose |
|---|---|---|
| `snap-bi` | on | stringToSign builders, SHA helpers, Jakarta timestamp formatters, header builders |
| `webhook` | on | `WebhookVerifier` trait + Inacash + BRI VA built-ins |

Disable with `default-features = false` for pure-HMAC / pure-RSA usage (e.g. WASM).

## Framework adapters

Inbound-verify glue per framework lives in adjacent crates:

- `kamu-snap-crypto-actix` — actix-web `verify_request` helper
- `kamu-snap-crypto-axum` — axum/`http` `verify_request` helper

## Migration from v1.x

| v1.x | v2.0 |
|---|---|
| `SymmetricCrypto::create(s)` | `HmacSigner::new(s)` |
| `signer.sign(p)` (`&mut self`, returns `String`) | `signer.sign(p)` (`&self`, returns `Signature`) |
| `AsymmetricCryptoSigner` | `RsaSigner` (default = `RsaSigner<Pkcs1v15Sha256>`) |
| `signer.sign_as_base64(p)` | `signer.sign(p).to_base64()` |
| `AsymmetricCryptoVerifier::verify_base64(s, p)` | `verifier.verify(&Signature::from_base64(s)?, p)` |
| (no helpers) | `snap_bi::sign_service`, `sha256_lower_hex`, etc. |

`From<CryptoError> for ResponseError` moved to `kamu-snap-response` behind its `crypto` feature.

## License

Dual-licensed under either [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE) at
your option (`MIT OR Apache-2.0`). Previously MIT-only in `pt-immer/lib-snap`.

[badge-crates]: https://img.shields.io/crates/v/kamu-snap-crypto?style=flat-square&logo=rust
[badge-docs]: https://img.shields.io/docsrs/kamu-snap-crypto?style=flat-square&logo=docs.rs&label=docs.rs
[badge-ci]: https://img.shields.io/github/actions/workflow/status/pt-immer/kamu-public-crates/on-pr-synced.yml?branch=main&style=flat-square&label=CI
[badge-license]: https://img.shields.io/crates/l/kamu-snap-crypto?style=flat-square
[badge-msrv]: https://img.shields.io/badge/MSRV-1.85-blue?style=flat-square&logo=rust

[link-crates]: https://crates.io/crates/kamu-snap-crypto
[link-docs]: https://docs.rs/kamu-snap-crypto
[link-ci]: https://github.com/pt-immer/kamu-public-crates/actions/workflows/on-pr-synced.yml
[link-license]: https://github.com/pt-immer/kamu-public-crates/blob/main/crates/snap-crypto
[link-msrv]: https://github.com/pt-immer/kamu-public-crates/blob/main/Cargo.toml
