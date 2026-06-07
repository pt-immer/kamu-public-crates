# Changelog — `kamu-snap-response`

## 2.0.0 — 2026-05-28

### Breaking

- **Crate is now a leaf** — no `actix-web` dep, no `phf`. Compiles to `wasm32-unknown-unknown`. Adapter crates carry the framework couplings.
- **Renamed types**:
  - `SNAPResponse<T>` → `SnapResponse<T>`
  - `SNAPResponseCommon` → `SnapEnvelope`
  - `ResponseError` alias → use `Error` directly
  - `ResponseCategory` alias → use `Category` directly
- **Fixed the `Unathorized` typo** at the definition site → `Error::Unauthorized`.
- **Drop `#[default]` on `Error::GeneralError`** — no more silent 500 fallback.
- **Drop `Clone` on `Error`** (so future `#[source]` chains can carry non-`Clone` upstream errors).
- **Methods renamed**:
  - `get_category` → `category`
  - `get_http_status_code` → `http_status`
  - `get_case_code` → `case_code`
  - `get_code(svc: u8)` → `response_code(svc: ServiceCode)`
- **Custom `Deserialize` propagates payload errors loudly** — no silent `payload = None` on schema mismatch.
- **`From<Result<T>> for SnapResponse<T>` removed** — handlers must call `ok(svc)` / `err(svc)` explicitly.
- **`traced_guard!` macro + `macros.rs` removed**. Consumers use `?` against the new `Error::Crypto(#[from])` variant.
- **`http::StatusCode`** instead of `actix_web::http::StatusCode` (actix re-exports the same upstream type — no observable change for actix consumers).

### Added

- **`ResponseCode` newtype**: defensive `parse(s)` never errors; preserves `raw()` for malformed wire codes; `http() / service() / case()` return `None` on malformed.
- **`ResponseCode::classify()`**: inverse parser that maps received wire codes back to typed `Error` variants. String-bearing variants reconstruct with empty string; wire `responseMessage` remains in `envelope.response_message`.
- **`ServiceCode` newtype**: `const fn new(u8) -> Option<Self>` rejects `>99` at construction. Closes the old `service_code % 100` modulo-truncation hazard.
- **`#[non_exhaustive]`** on `Error` and `Category`. Future variants are non-breaking.
- **`Category` derives**: now `PartialEq`, `Eq`, `Hash`, `Serialize`, `Deserialize`, `Display`, `as_str() -> &'static str`.
- **`crypto` feature** (off by default): enables `Error::Crypto(#[from] kamu_snap_crypto::Error)` — absorbs the bridge that used to live in `kamu-snap-crypto` (making that crate a true leaf).
- 21 integration tests: ResponseCode parser, classify, full 61-variant taxonomy table, round-trip serialise/deserialise, F-08 regression (payload error propagation), F-03 regression (malformed code preservation).
- README with quickstart, formula explainer, migration table.

### Adapter crates (separate packages)

- `kamu-snap-response-actix` — `Responder` impl, defensive `.http().unwrap_or(500)` (no `.unwrap()`).
- `kamu-snap-response-axum` — `IntoResponse` impl with the same defensive fallback.
