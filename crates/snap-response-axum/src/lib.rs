#![doc = include_str!("../README.md")]
#![forbid(unsafe_code)]

use axum::{
    http::{StatusCode, header},
    response::IntoResponse,
};
use kamu_snap_response::{ServiceCode, SnapResponse, internal_error_body};
use serde::Serialize;

/// Orphan-rule newtype implementing [`IntoResponse`].
pub struct AxumResponder<T>(pub SnapResponse<T>);

impl<T: Serialize> IntoResponse for AxumResponder<T> {
    fn into_response(self) -> axum::response::Response {
        let status = self.0.http_status();
        let service = self
            .0
            .valid_response_code()
            .map(kamu_snap_response::ValidResponseCode::service_code)
            .unwrap_or(ServiceCode::ZERO);
        let Ok(body) = serde_json::to_vec(&self.0) else {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                [(header::CONTENT_TYPE, "application/json")],
                internal_error_body(service),
            )
                .into_response();
        };

        (status, [(header::CONTENT_TYPE, "application/json")], body).into_response()
    }
}

/// Converts a SNAP BI response into its Axum responder.
pub trait SnapResponderExt<T> {
    /// Wrap this response.
    fn into_axum(self) -> AxumResponder<T>;
}

impl<T> SnapResponderExt<T> for SnapResponse<T> {
    fn into_axum(self) -> AxumResponder<T> {
        AxumResponder(self)
    }
}
