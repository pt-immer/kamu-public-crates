use kamu_snap_response::{
    CaseCode, CodeOutOfRange, ErrorClass, ResponseCode, ResponseCodeError, ServiceCode, ValidResponseCode,
};

#[test]
fn full_parse_shares_one_validated_result() {
    let code = ResponseCode::parse("4011100");

    assert_eq!(code.as_str(), "4011100");
    assert_eq!(code.http_status(), Some(http::StatusCode::UNAUTHORIZED));
    assert_eq!(code.service_code(), ServiceCode::new(11));
    assert_eq!(code.case_code(), CaseCode::new(0));
    assert_eq!(code.classify(), Some(ErrorClass::Unauthorized));
    assert!(code.valid().is_some());
    assert!(code.raw().is_none());
}

#[test]
fn every_malformed_component_invalidates_the_whole_code() {
    let cases = [
        "500000",  // wrong length
        "200ab00", // invalid service
        "20011ab", // invalid case
        "20é000",  // seven bytes, non-ASCII
        "ABCDEFG", // valid length, no digits
        "0001100", // invalid HTTP status
        "",
    ];

    for raw in cases {
        let code = ResponseCode::parse(raw);
        assert_eq!(code.as_str(), raw);
        assert!(code.valid().is_none(), "{raw}");
        assert!(code.raw().is_some(), "{raw}");
        assert_eq!(code.http_status(), None, "{raw}");
        assert_eq!(code.service_code(), None, "{raw}");
        assert_eq!(code.case_code(), None, "{raw}");
        assert_eq!(code.classify(), None, "{raw}");
    }
}

#[test]
fn unknown_but_syntactically_valid_code_keeps_all_components() {
    let code = ResponseCode::parse("4189942");

    assert_eq!(code.http_status(), Some(http::StatusCode::IM_A_TEAPOT));
    assert_eq!(code.service_code(), ServiceCode::new(99));
    assert_eq!(code.case_code(), CaseCode::new(42));
    assert_eq!(code.classify(), None);
}

#[test]
fn strict_parser_reports_failure_site() {
    assert!(matches!(ValidResponseCode::parse("200110"), Err(ResponseCodeError::Length { actual: 6 })));
    assert!(matches!(ValidResponseCode::parse("200a100"), Err(ResponseCodeError::NonDigit { index: 3 })));
    assert!(matches!(ValidResponseCode::parse("0001100"), Err(ResponseCodeError::HttpStatus { value: 0 })));
}

#[test]
fn typed_construction_is_infallible_and_canonical() {
    let service = ServiceCode::try_from(5).unwrap();
    let case = CaseCode::try_from(11).unwrap();
    let code = ValidResponseCode::from_parts(http::StatusCode::NOT_FOUND, service, case);

    assert_eq!(code.as_str(), "4040511");
    assert_eq!(code.http_status(), http::StatusCode::NOT_FOUND);
    assert_eq!(code.service_code(), service);
    assert_eq!(code.case_code(), case);
}

#[test]
fn numeric_construction_is_fallible_without_panics() {
    assert!(matches!(
        ValidResponseCode::try_from_parts(http::StatusCode::OK, 100, 0),
        Err(CodeOutOfRange::Service(100))
    ));
    assert!(matches!(
        ValidResponseCode::try_from_parts(http::StatusCode::OK, 0, 100),
        Err(CodeOutOfRange::Case(100))
    ));
}

#[test]
fn code_components_validate_and_pad() {
    let service = ServiceCode::try_from(5).unwrap();
    let case = CaseCode::try_from(7).unwrap();

    assert_eq!(service.to_string(), "05");
    assert_eq!(case.to_string(), "07");
    assert!(ServiceCode::try_from(100).is_err());
    assert!(CaseCode::try_from(255).is_err());
}

#[test]
fn serde_is_total_for_response_code_and_strict_for_valid_code() {
    let raw: ResponseCode = serde_json::from_str(r#""200ab00""#).unwrap();
    assert_eq!(raw.as_str(), "200ab00");
    assert!(raw.valid().is_none());
    assert_eq!(serde_json::to_string(&raw).unwrap(), r#""200ab00""#);

    let valid: ValidResponseCode = serde_json::from_str(r#""2001100""#).unwrap();
    assert_eq!(valid.as_str(), "2001100");
    assert_eq!(valid.to_string(), "2001100");
    let total = ResponseCode::from(valid.clone());
    assert_eq!(total.valid(), Some(&valid));
    assert!(serde_json::from_str::<ValidResponseCode>(r#""200ab00""#).is_err());

    let malformed = ResponseCode::parse("bad");
    let raw = malformed.raw().unwrap();
    assert_eq!(raw.to_string(), "bad");
    assert_eq!(serde_json::to_string(raw).unwrap(), r#""bad""#);
}

#[test]
fn component_serde_enforces_two_digit_range() {
    let service: ServiceCode = serde_json::from_str("99").unwrap();
    let case: CaseCode = serde_json::from_str("42").unwrap();

    assert_eq!(serde_json::to_string(&service).unwrap(), "99");
    assert_eq!(serde_json::to_string(&case).unwrap(), "42");
    assert!(serde_json::from_str::<ServiceCode>("100").is_err());
    assert!(serde_json::from_str::<CaseCode>("100").is_err());
    assert_eq!(CodeOutOfRange::Service(100).to_string(), "service code must be 0..=99, got 100");
    assert_eq!(CodeOutOfRange::Case(100).to_string(), "case code must be 0..=99, got 100");
}
