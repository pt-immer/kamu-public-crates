#![doc = include_str!("../README.md")]
#![forbid(unsafe_code)]

use actix_web::{HttpRequest, HttpResponse, HttpResponseBuilder, Responder, body::BoxBody};
use kamu_snap_response::{ServiceCode, SnapResponse, internal_error_body};
use serde::Serialize;

/// Orphan-rule newtype implementing [`Responder`].
pub struct ActixResponder<T>(pub SnapResponse<T>);

impl<T: Serialize> Responder for ActixResponder<T> {
    type Body = BoxBody;

    fn respond_to(self, _: &HttpRequest) -> HttpResponse<Self::Body> {
        let status = self.0.http_status();
        let service = self
            .0
            .valid_response_code()
            .map(kamu_snap_response::ValidResponseCode::service_code)
            .unwrap_or(ServiceCode::ZERO);
        let Ok(body) = serde_json::to_vec(&self.0) else {
            return HttpResponse::InternalServerError()
                .content_type("application/json")
                .body(internal_error_body(service));
        };
        let status = actix_web::http::StatusCode::from_u16(status.as_u16())
            .unwrap_or(actix_web::http::StatusCode::INTERNAL_SERVER_ERROR);

        HttpResponseBuilder::new(status).content_type("application/json").body(body)
    }
}

/// Converts a SNAP BI response into its Actix Web responder.
pub trait SnapResponderExt<T> {
    /// Wrap this response.
    fn into_actix(self) -> ActixResponder<T>;
}

impl<T> SnapResponderExt<T> for SnapResponse<T> {
    fn into_actix(self) -> ActixResponder<T> {
        ActixResponder(self)
    }
}
