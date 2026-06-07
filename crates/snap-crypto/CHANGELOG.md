# Changelog — `kamu-snap-crypto`

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
