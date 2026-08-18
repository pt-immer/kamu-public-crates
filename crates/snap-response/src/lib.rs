#![doc = include_str!("../README.md")]
#![forbid(unsafe_code)]

pub mod category;
pub mod envelope;
pub mod error;
pub mod response_code;

pub use category::Category;
pub use envelope::{
    FailureResponse, MalformedResponse, PayloadError, PayloadObject, SnapResponse, SuccessResponse,
    internal_error_body,
};
#[cfg(feature = "crypto")]
pub use error::{CryptoFailure, CryptoFailureClass};
pub use error::{Error, ErrorClass};
pub use response_code::{
    CaseCode, CodeOutOfRange, RawResponseCode, ResponseCode, ResponseCodeError, ServiceCode,
    ValidResponseCode,
};
