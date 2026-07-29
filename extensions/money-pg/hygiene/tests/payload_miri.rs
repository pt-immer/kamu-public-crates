//! Pgrx-free tests for the exact payload codec used by the extension.

#[path = "../../kamu-money-pg/src/safe/payload.rs"]
mod payload;

use kamu_money_core::{DOMAIN_MAX, Iso4217};
use payload::{PAYLOAD_BYTES, Payload, ValidationError, validate_payload};

#[test]
fn payload_layout_and_little_endian_round_trip() {
    assert_eq!(size_of::<Payload>(), PAYLOAD_BYTES);
    assert_eq!(align_of::<Payload>(), 1);

    for (units, code) in [(0, 840), (i128::MIN, u16::MIN), (i128::MAX, u16::MAX)] {
        let payload = Payload::from_parts(units, code);
        assert_eq!(payload.units(), units);
        assert_eq!(payload.code(), code);
        assert_eq!(&payload.to_bytes()[..16], &units.to_le_bytes());
        assert_eq!(&payload.to_bytes()[16..], &code.to_le_bytes());
        assert_eq!(Payload::try_from(payload.to_bytes().as_slice()), Ok(payload));
    }
}

#[test]
fn payload_slice_conversion_requires_exact_width() {
    assert_eq!(Payload::try_from(&[0_u8; PAYLOAD_BYTES - 1][..]).unwrap_err().actual, 17);
    assert_eq!(Payload::try_from(&[0_u8; PAYLOAD_BYTES + 1][..]).unwrap_err().actual, 19);
}

#[test]
fn validation_checks_expected_code_assignment_and_domain() {
    let usd = Iso4217::from_alpha3("USD").expect("USD is assigned");
    let idr = Iso4217::from_alpha3("IDR").expect("IDR is assigned");

    let valid = validate_payload(Payload::from_parts(1, usd.numeric()), Some(usd)).expect("valid USD");
    assert_eq!(valid.payload(), Payload::from_parts(1, usd.numeric()));
    assert_eq!(valid.units(), 1);
    assert_eq!(valid.currency(), usd);

    assert_eq!(
        validate_payload(Payload::from_parts(1, idr.numeric()), Some(usd)),
        Err(ValidationError::UnexpectedCurrency { expected: usd, found_code: idr.numeric() })
    );
    assert_eq!(
        validate_payload(Payload::from_parts(1, 0), None),
        Err(ValidationError::UnknownCurrency { code: 0 })
    );
    assert!(matches!(
        validate_payload(Payload::from_parts(DOMAIN_MAX + 1, usd.numeric()), None),
        Err(ValidationError::OutOfDomain { units, currency }) if units == DOMAIN_MAX + 1 && currency == usd
    ));
    assert!(matches!(
        validate_payload(Payload::from_parts(-DOMAIN_MAX - 1, usd.numeric()), None),
        Err(ValidationError::OutOfDomain { units, currency }) if units == -DOMAIN_MAX - 1 && currency == usd
    ));
}
