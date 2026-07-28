# Changelog — `kamu-snap-crypto`

## 3.0.0 — 2026-07-28

This release moves SNAP BI request policy into validated core types and removes
algorithms outside the protocol contract.

### Breaking

- `AccessToken` now requires exactly one Bearer credential. Scheme matching is
  ASCII case-insensitive; raw, Basic, empty, whitespace-ambiguous, and invalid
  RFC 6750 credentials are rejected.
- `ServiceStringToSign` and `OAuthStringToSign` fields are private. Their
  constructors validate path, timestamp, token, and header-compatible values.
- `ServiceHeaders::builder` is replaced by the
  `ServiceRequest<Unsigned> -> ServiceRequest<Signed>` transition. Complete
  headers exist only after signing.
- `HmacSigner::new` and `sign_service` are infallible.
- RSA is limited to SNAP BI's PKCS#1 v1.5 + SHA-256 scheme. The generic
  `SignatureScheme`, SHA-512, and PSS types were removed.
- `RsaVerifier::from_pkcs8_public_pem` is now `from_spki_pem`, matching the
  public-key container it parses.
- `WebhookVerifier` is split into `BodyWebhookVerifier` and
  `RequestWebhookVerifier`. `BriVaPaidVerifier` now verifies full request
  context instead of implementing an always-failing body-only contract.

### Added

- Framework-neutral `ServiceRequestParts` and `verify_service_request`.
- Structured `AuthorizationError`, `InputError`, and
  `ServiceVerificationError` taxonomies.
- `BodyHmacVerifier`; the two Inacash names are discoverability aliases over
  the same implementation.
- A vector built from BRI's published field examples that pins body hashing and
  confirms query parameters are excluded from the canonical path.

### Fixed

- `format_jakarta` now converts its input to `+07:00` before formatting.
- Invalid service-signature encoding and length are rejected during request
  construction. Invalid raw RSA signature length reports `InvalidRawSignature`,
  not a base64 decoding failure.
- Debug output for canonical requests, access tokens, signed headers, OAuth
  headers, and secret-bearing provider verifiers is redacted.

## 2.3.0 — 2026-07-27

Toolchain maintenance only. No code or public API changes.

### Changed

- Minimum supported Rust version raised to 1.94.

## 2.2.0 — 2026-06-14

Webhook-verifier hardening + doc fix surfaced by a knowledge-graph audit.

### Fixed

- `BriVaPaidVerifier::canonical_payload` now returns an error instead of
  silently HMAC-ing the raw request body. BRI VA signs the full SNAP BI service
  `stringToSign` (`method:path:accessToken:lowercaseHex(SHA256(body)):timestamp`),
  not the body, so the previous body-only stub would reject every legitimate
  callback — or pass only against a same-shaped test signature and then fail in
  production. Use `kamu-snap-crypto-actix` / `kamu-snap-crypto-axum`
  `verify_request`, or implement `WebhookVerifier` with the request context and
  override `canonical_payload`. Added webhook-provider tests (Inacash
  round-trip + tamper rejection, missing-header, and the BriVa fail-loud path).

### Changed

- `ServiceHeaders::into_pairs` doc corrected: the emitted header names are the
  canonical SNAP BI **uppercase** forms — build the `HeaderName` with
  `HeaderName::from_bytes` / `TryFrom<&str>`, not `from_static` (which panics on
  non-lowercase input).

## 2.1.0 — 2026-06-11

Toolchain/metadata only. No code or public API changes.

### Changed

- MSRV raised from 1.85 to 1.88 (workspace-wide, driven by the workspace
  `time` >= 0.3.47 floor for RUSTSEC-2026-0009). This crate has no `time`
  dependency itself.

## 2.0.1 — 2026-06-08

Docs only. No code or public API changes.

### Changed

- Standardized the README badge block (Crates.io / docs.rs / CI / License / MSRV)
  and added a workspace link.

## 2.0.0 — 2026-05-28

### Breaking

- **Crate is now a leaf** — no `kamu-snap-response` dep, no transitive `actix-web`. `wasm32-unknown-unknown` compiles require the consumer to enable `getrandom/js` (transitive via `rsa`).
- **Renamed types**: `SymmetricCrypto` → `HmacSigner`. Two structs both called `Crypto` are now `RsaSigner` / `RsaVerifier`.
- **`&self`, not `&mut self`** on `sign` and `verify` (HMAC, RSA). One signer can serve many threads with no `Mutex`.
- **Encoding-agnostic** `Signature` newtype + `Encoding` enum (Base64 / Base64UrlNoPad / HexLower). `sign` returns `Signature`, not `String`.
- **Sealed `SignatureScheme` trait** with 4 built-in schemes: `Pkcs1v15Sha256` (default), `Pkcs1v15Sha512`, `PssSha256`, `PssSha512`.
- **Error enum**: `#[non_exhaustive]`, renamed variants, `#[source]` chains. `From<Error> for kamu_snap_response::ResponseError` impl removed — it lives in `kamu-snap-response` behind the `crypto` feature now.

### Added

- New `snap_bi` module (feature `snap-bi`, default on):
  - `sha256_lower_hex` / `sha512_lower_hex`
  - `now_jakarta_ms`, `now_jakarta_seconds`, `format_jakarta`
  - `ServiceStringToSign`, `OAuthStringToSign` builders
  - One-shot `sign_service` / `verify_service` / `sign_oauth` / `verify_oauth`
  - `ServiceHeaders` / `OAuthHeaders` framework-agnostic header builders
- New `webhook` module (feature `webhook`, default on):
  - `WebhookVerifier` trait
  - Built-in `InacashCashoutVerifier`, `InacashQrisVerifier`, `BriVaPaidVerifier`
- 34 integration tests:
  - RFC 4231 HMAC-SHA512 known-answer vectors (cases 1–4, 6, 7)
  - RSA round-trip for all 4 schemes + wrong-key/wrong-payload negative tests
  - Garbage PEM rejection
  - Signature encoding dispatcher
  - SNAP BI recipe tests (NIST SHA-256 vectors, stringToSign format, headers builder)
- README with quickstart, security guarantees, migration table.

### Adapter crates (separate packages)

- `kamu-snap-crypto-actix` ships an inbound-verify helper for actix-web requests.
- `kamu-snap-crypto-axum` ships the equivalent for axum / `http::request::Parts`.
