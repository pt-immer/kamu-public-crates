# `kamu-snap-crypto`

[![Crates.io][badge-crates]][link-crates]
[![docs.rs][badge-docs]][link-docs]
[![CI][badge-ci]][link-ci]

[![License][badge-license]][link-license]
[![MSRV][badge-msrv]][link-msrv]

Framework-neutral authentication for Bank Indonesia SNAP BI integrations.

## Use it from the boundary inward

The top-level request API owns Bearer parsing, canonicalization, signing, and
verification. Lower-level HMAC, RSA-SHA256, hashes, timestamps, and signature
encodings remain available for integrations that need them.

```rust
use http::Method;
use kamu_snap_crypto::{AccessToken, HmacSigner, ServiceRequest};
use kamu_snap_crypto::snap_bi::{ServiceStringToSign, Unsigned};

fn build_headers() -> Result<Vec<(&'static str, String)>, Box<dyn std::error::Error>> {
    let method = Method::POST;
    let token = AccessToken::from_credential("oauth-access-token")?;
    let canonical = ServiceStringToSign::new(
        &method,
        "/snap/v1.0/balance-inquiry",
        token,
        br#"{"accountNo":"1231271284141"}"#,
        "2026-07-28T12:34:56+07:00",
    )?;
    let request: ServiceRequest<'_, Unsigned> =
        ServiceRequest::new(canonical, "partner-id", "12345", "000000001")?;

    Ok(request
        .sign(&HmacSigner::new("client-secret"))
        .headers()
        .into_pairs())
}
```

`ServiceHeaders` is available only after `sign`. Its `Debug` output redacts
tokens, signatures, identifiers, and payloads.

## Verify inbound requests

Framework adapters extract their request types, then delegate all policy here:

```rust
use http::Method;
use kamu_snap_crypto::{
    ServiceRequestParts, ServiceVerificationError, verify_service_request,
};

fn verify(signature: &str, body: &[u8]) -> Result<(), ServiceVerificationError> {
    let request = ServiceRequestParts::new(
        &Method::POST,
        "/snap/v1.0/dummy",
        "Bearer token",
        signature,
        "2026-07-28T12:34:56+07:00",
        body,
    )?;
    verify_service_request("client-secret", request)
}
```

The `Bearer` scheme is ASCII case-insensitive. Exactly one ASCII space and one
RFC 6750 `b64token` credential are required.

BRI signs the origin-form path without query parameters. `CanonicalPath`
therefore preserves percent-encoded octets and rejects `?` or `#`.

## Webhooks

Body-only providers and request-context providers have separate interfaces:

```rust
use http::HeaderMap;
use kamu_snap_crypto::Error;
use kamu_snap_crypto::webhook::{BodyWebhookVerifier, InacashCashoutVerifier};

fn verify_inacash(headers: &HeaderMap, body: &[u8]) -> Result<(), Error> {
    InacashCashoutVerifier::new("shared-secret").verify_body(headers, body)
}
```

`BriVaPaidVerifier` implements `RequestWebhookVerifier` because BRI VA signs
method, path, access token, body hash, and timestamp. It cannot be called
through the body-only interface.

## Algorithms and keys

- `HmacSigner`: HMAC-SHA512; construction is infallible and verification uses
  `hmac::Mac::verify_slice`.
- `RsaSigner`: PKCS#1 v1.5 + SHA-256 with a PKCS#8 private-key PEM.
- `RsaVerifier`: PKCS#1 v1.5 + SHA-256 with an SPKI public-key PEM.
- `Signature`: standard base64, unpadded base64url, or lowercase hexadecimal.

The RSA dependency is subject to
[RUSTSEC-2023-0071](https://rustsec.org/advisories/RUSTSEC-2023-0071.html).
This crate uses RSA for SNAP BI signing and signature verification, not
attacker-controlled ciphertext decryption. See the repository `deny.toml` for
the accepted-risk scope.

## Features

| Feature | Default | Adds |
| --- | --- | --- |
| `snap-bi` | yes | Validated requests, canonical recipes, timestamps, headers |
| `webhook` | yes | Body-only and request-aware provider verifiers |

With default features disabled, only HMAC, RSA-SHA256, and signature encodings
remain.

## Migrating from 2.x

- `HmacSigner::new` now returns `HmacSigner`, not `Result`.
- `RsaSigner` and `RsaVerifier` now expose only SNAP BI's RSA-SHA256 scheme.
- `RsaVerifier::from_pkcs8_public_pem` is now `from_spki_pem`.
- `ServiceStringToSign` fields are private; use `new`.
- `ServiceHeaders::builder` is replaced by
  `ServiceRequest<Unsigned>::sign(...).headers()`.
- `WebhookVerifier` is split into `BodyWebhookVerifier` and
  `RequestWebhookVerifier`.

## Examples

[`sign_snap_bi_request`](examples/sign_snap_bi_request.rs) builds and signs an
outbound SNAP BI service request. Run it with
`cargo run --example sign_snap_bi_request`.

## License

Dual-licensed under either [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE) at
your option (`MIT OR Apache-2.0`).

[badge-crates]: https://img.shields.io/crates/v/kamu-snap-crypto?style=flat-square&logo=rust
[badge-docs]: https://img.shields.io/docsrs/kamu-snap-crypto?style=flat-square&logo=docs.rs&label=docs.rs
[badge-ci]: https://img.shields.io/github/actions/workflow/status/pt-immer/kamu-public-crates/on-pr-synced.yml?branch=main&style=flat-square&label=CI
[badge-license]: https://img.shields.io/crates/l/kamu-snap-crypto?style=flat-square
[badge-msrv]: https://img.shields.io/badge/MSRV-1.94-blue?style=flat-square&logo=rust

[link-crates]: https://crates.io/crates/kamu-snap-crypto
[link-docs]: https://docs.rs/kamu-snap-crypto
[link-ci]: https://github.com/pt-immer/kamu-public-crates/actions/workflows/on-pr-synced.yml
[link-license]: https://github.com/pt-immer/kamu-public-crates/blob/main/crates/snap-crypto
[link-msrv]: https://github.com/pt-immer/kamu-public-crates/blob/main/Cargo.toml
