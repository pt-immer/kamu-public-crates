use std::collections::HashSet;

use kamu_snap_response::{Category, Error, ErrorClass, ServiceCode};

#[test]
fn declarative_taxonomy_contains_61_unique_wire_pairs() {
    assert_eq!(ErrorClass::ALL.len(), 61);

    let mut pairs = HashSet::new();
    for class in ErrorClass::ALL {
        let pair = (class.http_status().as_u16(), class.case_code().get());
        assert!(pairs.insert(pair), "duplicate wire pair for {class:?}");
        assert_eq!(ErrorClass::from_http_and_case(class.http_status(), class.case_code()), Some(*class));

        let code = class.response_code(ServiceCode::try_from(11).unwrap());
        assert_eq!(code.http_status(), class.http_status());
        assert_eq!(code.case_code(), class.case_code());
        assert_eq!(code.classify(), Some(*class));
        assert!(!class.as_str().is_empty());
        assert!(!class.message().is_empty());
        assert_eq!(class.to_string(), class.message());
        let wire = serde_json::to_string(class).unwrap();
        assert_eq!(serde_json::from_str::<ErrorClass>(&wire).unwrap(), *class);
    }
}

#[test]
fn contextual_server_error_keeps_context_and_stable_class_separate() {
    let error = Error::InvalidFieldFormat("amount".into());

    assert_eq!(error.to_string(), "Invalid Field Format amount");
    assert_eq!(error.response_message(), "Invalid Field Format amount");
    assert_eq!(error.class(), ErrorClass::InvalidFieldFormat);
    assert_eq!(error.http_status(), http::StatusCode::BAD_REQUEST);
    assert_eq!(error.case_code().get(), 1);
    assert_eq!(error.category(), Category::Message);
    assert_eq!(error.response_code(ServiceCode::try_from(11).unwrap()).as_str(), "4001101");
}

#[test]
fn representative_categories_remain_stable() {
    assert_eq!(Error::BadRequest.category(), Category::System);
    assert_eq!(Error::InsufficientFunds.category(), Category::Business);
    assert_eq!(Error::InvalidRouting.category(), Category::System);
    assert_eq!(Error::Timeout.category(), Category::System);
}

#[cfg(feature = "crypto")]
mod crypto {
    use kamu_snap_crypto::{Error as CryptoError, ServiceVerificationError, snap_bi::InputError};
    use kamu_snap_response::{CryptoFailureClass, Error, ErrorClass, ServiceCode, SnapResponse};

    #[test]
    fn authentication_failure_maps_to_401_without_source_disclosure() {
        let error = Error::from(CryptoError::SymmetricVerifyFailed);

        assert_eq!(error.crypto_class(), Some(CryptoFailureClass::Authentication));
        assert_eq!(error.class(), ErrorClass::Unauthorized);
        assert_eq!(error.http_status(), http::StatusCode::UNAUTHORIZED);
        assert_eq!(error.response_message(), "Unauthorized");
    }

    #[test]
    fn request_validation_failure_maps_to_400() {
        let error = Error::from(CryptoError::SnapBiInput(InputError::InvalidPath));

        assert_eq!(error.crypto_class(), Some(CryptoFailureClass::InvalidRequest));
        assert_eq!(error.class(), ErrorClass::BadRequest);
        assert_eq!(error.http_status(), http::StatusCode::BAD_REQUEST);
    }

    #[test]
    fn service_signature_failure_remains_authentication() {
        let error =
            Error::from(CryptoError::ServiceVerification(ServiceVerificationError::SignatureMismatch));

        assert_eq!(error.crypto_class(), Some(CryptoFailureClass::Authentication));
        assert_eq!(error.class(), ErrorClass::Unauthorized);
    }

    #[test]
    fn authentication_headers_are_distinct_from_other_missing_input() {
        let authentication = Error::from(CryptoError::MissingHeader { name: "authorization" });
        let request = Error::from(CryptoError::MissingHeader { name: "X-TIMESTAMP" });

        assert_eq!(authentication.crypto_class(), Some(CryptoFailureClass::Authentication));
        assert_eq!(request.crypto_class(), Some(CryptoFailureClass::InvalidRequest));
    }

    #[test]
    fn local_key_failure_maps_to_500_and_redacts_details() {
        let marker = "do-not-send-this-key-parser-detail";
        let error = Error::from(CryptoError::InvalidPublicKey(marker.into()));

        assert_eq!(error.crypto_class(), Some(CryptoFailureClass::Configuration));
        assert_eq!(error.class(), ErrorClass::InternalServerError);
        assert_eq!(error.http_status(), http::StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(error.response_message(), "Internal Server Error");
        assert!(!error.to_string().contains(marker));
        assert!(!format!("{error:?}").contains(marker));

        let Error::Crypto(source) = &error else {
            panic!("expected crypto bridge");
        };
        assert_eq!(source.class(), CryptoFailureClass::Configuration);
        assert!(std::error::Error::source(source).is_some());
        assert!(matches!(source.source_error(), CryptoError::InvalidPublicKey(_)));

        let response = SnapResponse::<()>::failure(error, ServiceCode::try_from(11).unwrap());
        let SnapResponse::Failure(details) = response else {
            panic!("expected failure");
        };
        assert_eq!(details.crypto_class(), Some(CryptoFailureClass::Configuration));
        assert_eq!(details.response_message(), "Internal Server Error");
    }
}
