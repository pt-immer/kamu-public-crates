//! `axum::response::IntoResponse` adapter for
//! [`kamu_snap_response::SnapResponse`].
//!
//! Wraps `SnapResponse<T>` in a newtype that implements `IntoResponse` for
//! axum 0.7+. Defensive fallback to `INTERNAL_SERVER_ERROR` if the
//! `responseCode` cannot be parsed back into an HTTP status.

#![forbid(unsafe_code)]

use axum::{Json, response::IntoResponse};
use kamu_snap_response::SnapResponse;
use serde::Serialize;

/// Newtype wrapping `SnapResponse<T>` for axum's `IntoResponse` impl
/// (orphan-rule shim).
pub struct AxumResponder<T>(pub SnapResponse<T>);

impl<T: Serialize> IntoResponse for AxumResponder<T> {
    fn into_response(self) -> axum::response::Response {
        let status = self.0.envelope.response_code.http().unwrap_or(http::StatusCode::INTERNAL_SERVER_ERROR);
        (status, Json(self.0)).into_response()
    }
}

/// Extension trait: `.into_axum()` on `SnapResponse<T>`.
pub trait SnapResponderExt<T> {
    /// Wrap into [`AxumResponder`].
    fn into_axum(self) -> AxumResponder<T>;
}

impl<T> SnapResponderExt<T> for SnapResponse<T> {
    fn into_axum(self) -> AxumResponder<T> {
        AxumResponder(self)
    }
}
