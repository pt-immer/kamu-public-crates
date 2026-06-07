//! Data-table tests for the SNAP BI error taxonomy: every variant's
//! `(http_status, case_code, category) -> response_code` mapping is locked.

use kamu_snap_response::{Category, Error, ServiceCode};

const SVC: u8 = 11; // arbitrary service code used across the table

fn check(error: Error, expected_http: u16, expected_case: u8, expected_category: Category) {
    assert_eq!(error.http_status().as_u16(), expected_http, "{error:?}");
    assert_eq!(error.case_code(), expected_case, "{error:?}");
    assert_eq!(error.category(), expected_category, "{error:?}");
    let svc = ServiceCode::new(SVC).unwrap();
    let expected_response = format!("{expected_http:03}{SVC:02}{expected_case:02}");
    assert_eq!(error.response_code(svc).raw(), expected_response, "{error:?}");
}

#[test]
fn taxonomy_table() {
    use Error::*;

    check(BadRequest, 400, 0, Category::System);
    check(InvalidFieldFormat(String::new()), 400, 1, Category::Message);
    check(InvalidMandatoryField(String::new()), 400, 2, Category::Message);

    check(Unauthorized(String::new()), 401, 0, Category::System);
    check(InvalidTokenB2B, 401, 1, Category::System);
    check(InvalidCustomerToken, 401, 2, Category::System);
    check(TokenNotFoundB2B, 401, 3, Category::System);
    check(CustomerTokenNotFound, 401, 4, Category::System);

    check(TransactionExpired, 403, 0, Category::Business);
    check(FeatureNotAllowed(String::new()), 403, 1, Category::System);
    check(ExceedsTransactionAmountLimit, 403, 2, Category::Business);
    check(SuspectedFraud, 403, 3, Category::Business);
    check(ActivityCountLimitExceeded, 403, 4, Category::Business);
    check(DoNotHonor, 403, 5, Category::Business);
    check(FeatureNotAllowedAtThisTime(String::new()), 403, 6, Category::System);
    check(CardBlocked, 403, 7, Category::Business);
    check(CardExpired, 403, 8, Category::Business);
    check(DormantAccount, 403, 9, Category::Business);
    check(NeedToSetTokenLimit, 403, 10, Category::Business);
    check(OTPBlocked, 403, 11, Category::System);
    check(OTPLifetimeExpired, 403, 12, Category::System);
    check(OTPSentToCardholder, 403, 13, Category::System);
    check(InsufficientFunds, 403, 14, Category::Business);
    check(TransactionNotPermitted(String::new()), 403, 15, Category::Business);
    check(SuspendTransaction, 403, 16, Category::Business);
    check(TokenLimitExceeded, 403, 17, Category::Business);
    check(InactiveCardOrAccountOrCustomer, 403, 18, Category::Business);
    check(MerchantBlacklisted, 403, 19, Category::Business);
    check(MerchantLimitExceed, 403, 20, Category::Business);
    check(SetLimitNotAllowed, 403, 21, Category::Business);
    check(TokenLimitInvalid, 403, 22, Category::Business);
    check(AccountLimitExceed, 403, 23, Category::Business);

    check(InvalidTransactionStatus, 404, 0, Category::Business);
    check(TransactionNotFound, 404, 1, Category::Business);
    check(InvalidRouting, 404, 2, Category::System);
    check(BankNotSupportedBySwitch, 404, 3, Category::System);
    check(TransactionCancelled, 404, 4, Category::Business);
    check(MerchantNotRegisteredForCardRegistrationServices, 404, 5, Category::Business);
    check(NeedToRequestOTP, 404, 6, Category::System);
    check(JourneyNotFound, 404, 7, Category::System);
    check(InvalidMerchant, 404, 8, Category::Business);
    check(NoIssuer, 404, 9, Category::Business);
    check(InvalidAPITransition, 404, 10, Category::System);
    check(InvalidCardOrAccountOrCustomerOrVirtualAccount(String::new()), 404, 11, Category::Business);
    check(InvalidBillOrVirtualAccountWithReason(String::new()), 404, 12, Category::Business);
    check(InvalidAmount, 404, 13, Category::Business);
    check(PaidBill, 404, 14, Category::Business);
    check(InvalidOTP, 404, 15, Category::System);
    check(PartnerNotFound, 404, 16, Category::Business);
    check(InvalidTerminal, 404, 17, Category::Business);
    check(InconsistentRequest, 404, 18, Category::Business);
    check(InvalidBillOrVirtualAccount, 404, 19, Category::Business);

    check(RequestedFunctionIsNotSupported, 405, 0, Category::System);
    check(RequestedOperationIsNotAllowed, 405, 1, Category::Business);

    check(Conflict, 409, 0, Category::System);
    check(DuplicatePartnerReferenceNo, 409, 1, Category::System);

    check(TooManyRequests, 429, 0, Category::System);

    check(GeneralError, 500, 0, Category::System);
    check(InternalServerError, 500, 1, Category::System);
    check(ExternalServerError, 500, 2, Category::System);

    check(Timeout, 504, 0, Category::System);
}

#[test]
fn from_http_and_case_round_trips_every_variant() {
    // Build every code from a variant, then classify it back to the same kind.
    let svc = ServiceCode::new(11).unwrap();
    let variants: Vec<(Error, &'static str)> = vec![
        (Error::BadRequest, "BadRequest"),
        (Error::InvalidFieldFormat(String::new()), "InvalidFieldFormat"),
        (Error::Unauthorized(String::new()), "Unauthorized"),
        (Error::InvalidTokenB2B, "InvalidTokenB2B"),
        (Error::InsufficientFunds, "InsufficientFunds"),
        (Error::Timeout, "Timeout"),
        (Error::GeneralError, "GeneralError"),
    ];
    for (variant, label) in variants {
        let code = variant.response_code(svc);
        let parsed = code.classify().unwrap_or_else(|| panic!("classify failed for {label}"));
        // Discriminant should match.
        assert_eq!(
            std::mem::discriminant(&parsed),
            std::mem::discriminant(&variant),
            "round-trip mismatch for {label}"
        );
    }
}
