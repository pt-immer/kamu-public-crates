//! `actix-web::Responder` adapter for [`kamu_snap_response::SnapResponse`].
//!
//! Add this crate to enable `actix_web::Responder` on `SnapResponse<T>`. The
//! impl is defensive: if `responseCode` cannot be parsed back into an HTTP
//! status (malformed wire code), the response falls back to
//! `INTERNAL_SERVER_ERROR` instead of panicking. Closes review F-02.

#![forbid(unsafe_code)]

use actix_web::{HttpRequest, HttpResponse, HttpResponseBuilder, Responder, body::BoxBody};
use kamu_snap_response::SnapResponse;
use serde::Serialize;

/// Newtype wrapping `SnapResponse<T>` for the actix `Responder` impl.
///
/// This indirection is required by Rust's orphan rule — neither
/// `kamu-snap-response` nor `actix-web` is owned by this crate. Convert via
/// `SnapResponderExt::into_actix` (or wrap by hand: `ActixResponder(resp)`).
pub struct ActixResponder<T>(pub SnapResponse<T>);

impl<T: Serialize> Responder for ActixResponder<T> {
    type Body = BoxBody;

    fn respond_to(self, _: &HttpRequest) -> HttpResponse<Self::Body> {
        let status = self.0.envelope.response_code.http().unwrap_or(http::StatusCode::INTERNAL_SERVER_ERROR);
        // actix::StatusCode and http::StatusCode share the underlying type;
        // re-wrap so the builder is satisfied across actix-web versions.
        let actix_status = actix_web::http::StatusCode::from_u16(status.as_u16())
            .unwrap_or(actix_web::http::StatusCode::INTERNAL_SERVER_ERROR);
        HttpResponseBuilder::new(actix_status).json(self.0)
    }
}

/// Extension trait providing `.into_actix()` for ergonomic conversion.
pub trait SnapResponderExt<T> {
    /// Wrap into [`ActixResponder`] for use as an actix handler return value.
    fn into_actix(self) -> ActixResponder<T>;
}

impl<T> SnapResponderExt<T> for SnapResponse<T> {
    fn into_actix(self) -> ActixResponder<T> {
        ActixResponder(self)
    }
}
