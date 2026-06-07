//! SNAP BI error taxonomy.
//!
//! Every variant carries a fixed `(http_status, case_code, category)` triple
//! exposed via [`Error::http_status`], [`Error::case_code`], and
//! [`Error::category`]. The full wire `responseCode` is composed via
//! [`Error::response_code`].
//!
//! Compared to v1.x:
//!
//! - `Unathorized` is renamed to [`Error::Unauthorized`] (hard rename).
//! - `#[non_exhaustive]` is present — new variants are non-breaking.
//! - The `#[default]` derive on `GeneralError` is dropped — callers must
//!   construct an `Error` explicitly. No silent 500 fallback.
//! - Method names lose their `get_` prefix.
//! - Returns [`http::StatusCode`], not `actix_web::http::StatusCode`. Actix
//!   re-exports the same upstream type, so existing consumers see no change.

use crate::category::Category;
use crate::response_code::{ResponseCode, ServiceCode};

/// All known SNAP BI error variants plus a feature-gated `Crypto` bridge.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// General request failed error, including message parsing failed.
    #[error("Bad Request")]
    BadRequest,
    /// Invalid format
    #[error("Invalid Field Format {0}")]
    InvalidFieldFormat(String),
    /// Missing or invalid format on mandatory field
    #[error("Invalid Mandatory Field {0}")]
    InvalidMandatoryField(String),
    /// General unauthorized error (No Interface Def, API is Invalid, Oauth
    /// Failed, Verify Client Secret Fail, Client Forbidden Access API, Unknown
    /// Client, Key not Found).
    #[error("Unauthorized. {0}")]
    Unauthorized(String),
    /// Token found in request is invalid (Access Token Not Exist, Access Token
    /// Expiry)
    #[error("Invalid Token (B2B)")]
    InvalidTokenB2B,
    /// Token found in request is invalid (Access Token Not Exist, Access Token
    /// Expiry)
    #[error("Invalid Customer Token")]
    InvalidCustomerToken,
    /// Token not found in the system. This occurs on any API that requires
    /// token as input parameter
    #[error("Token Not Found (B2B)")]
    TokenNotFoundB2B,
    /// Token not found in the system. This occurs on any API that requires
    /// token as input parameter
    #[error("Customer Token Not Found")]
    CustomerTokenNotFound,
    /// Transaction expired
    #[error("Transaction Expired")]
    TransactionExpired,
    /// This merchant is not allowed to call Direct Debit APIs
    #[error("Feature Not Allowed {0}")]
    FeatureNotAllowed(String),
    /// Exceeds Transaction Amount Limit
    #[error("Exceeds Transaction Amount Limit")]
    ExceedsTransactionAmountLimit,
    /// Suspected Fraud
    #[error("Suspected Fraud")]
    SuspectedFraud,
    /// Too many requests, exceeds transaction frequency limit
    #[error("Activity Count Limit Exceeded")]
    ActivityCountLimitExceeded,
    /// Account or User status is abnormal
    #[error("Do Not Honor")]
    DoNotHonor,
    /// Cut off in progress
    #[error("Feature Not Allowed At This Time. {0}")]
    FeatureNotAllowedAtThisTime(String),
    /// The payment card is blocked
    #[error("Card Blocked")]
    CardBlocked,
    /// The payment card is expired
    #[error("Card Expired")]
    CardExpired,
    /// The account is dormant
    #[error("Dormant Account")]
    DormantAccount,
    /// Need to set token limit
    #[error("Need To Set Token Limit")]
    NeedToSetTokenLimit,
    /// OTP has been blocked
    #[error("OTP Blocked")]
    OTPBlocked,
    /// OTP has been expired
    #[error("OTP Lifetime Expired")]
    OTPLifetimeExpired,
    /// Initiates request OTP to the issuer
    #[error("OTP Sent To Cardholder")]
    OTPSentToCardholder,
    /// Insufficient funds
    #[error("Insufficient Funds")]
    InsufficientFunds,
    /// Transaction not permitted
    #[error("Transaction Not Permitted. {0}")]
    TransactionNotPermitted(String),
    /// Suspend transaction
    #[error("Suspend Transaction")]
    SuspendTransaction,
    /// Purchase amount exceeds the token limit set prior
    #[error("Token Limit Exceeded")]
    TokenLimitExceeded,
    /// Inactive account
    #[error("Inactive Card/Account/Customer")]
    InactiveCardOrAccountOrCustomer,
    /// Merchant is suspended from calling any APIs
    #[error("Merchant Blacklisted")]
    MerchantBlacklisted,
    /// Merchant aggregated purchase amount on that day exceeds the agreed limit
    #[error("Merchant Limit Exceed")]
    MerchantLimitExceed,
    /// Set limit not allowed on particular token
    #[error("Set Limit Not Allowed")]
    SetLimitNotAllowed,
    /// The token limit desired by the merchant is not within the agreed range
    /// between the merchant and the Issuer
    #[error("Token Limit Invalid")]
    TokenLimitInvalid,
    /// Account aggregated purchase amount on that day exceeds the agreed limit
    #[error("Account Limit Exceed")]
    AccountLimitExceed,
    /// Invalid transaction status
    #[error("Invalid Transaction Status")]
    InvalidTransactionStatus,
    /// Transaction not found
    #[error("Transaction Not Found")]
    TransactionNotFound,
    /// Invalid routing
    #[error("Invalid Routing")]
    InvalidRouting,
    /// Bank not supported by switch
    #[error("Bank Not Supported By Switch")]
    BankNotSupportedBySwitch,
    /// Transaction is cancelled by customer
    #[error("Transaction Cancelled")]
    TransactionCancelled,
    /// Merchant is not registered for Card Registration services
    #[error("Merchant Is Not Registered For Card Registration Services")]
    MerchantNotRegisteredForCardRegistrationServices,
    /// Need to request OTP
    #[error("Need To Request OTP")]
    NeedToRequestOTP,
    /// The journeyId cannot be found in the system
    #[error("Journey Not Found")]
    JourneyNotFound,
    /// Merchant does not exist or status abnormal
    #[error("Invalid Merchant")]
    InvalidMerchant,
    /// No issuer
    #[error("No Issuer")]
    NoIssuer,
    /// Invalid API transition within a journey
    #[error("Invalid API Transition")]
    InvalidAPITransition,
    /// Card information may be invalid, or the card account may be blacklisted,
    /// or Virtual Account number maybe invalid.
    #[error("Invalid Card/Account/Customer {0}/Virtual Account")]
    InvalidCardOrAccountOrCustomerOrVirtualAccount(String),
    /// The bill or Virtual account is blocked/ suspended/not found.
    #[error("Invalid Bill/Virtual Account {0}")]
    InvalidBillOrVirtualAccountWithReason(String),
    /// The amount doesn't match with what supposed to
    #[error("Invalid Amount")]
    InvalidAmount,
    /// The bill has been paid
    #[error("Paid Bill")]
    PaidBill,
    /// OTP is incorrect
    #[error("Invalid OTP")]
    InvalidOTP,
    /// Partner number can't be found
    #[error("Partner Not Found")]
    PartnerNotFound,
    /// Terminal does not exist in the system
    #[error("Invalid Terminal")]
    InvalidTerminal,
    /// Inconsistent request parameter found for the same partner reference
    /// number/transaction id.
    #[error("Inconsistent Request")]
    InconsistentRequest,
    /// The bill is expired.
    #[error("Invalid Bill/Virtual Account")]
    InvalidBillOrVirtualAccount,
    /// Requested function is not supported
    #[error("Requested Function Is Not Supported")]
    RequestedFunctionIsNotSupported,
    /// Requested operation to cancel/refund transaction is not allowed at this
    /// time.
    #[error("Requested Operation Is Not Allowed")]
    RequestedOperationIsNotAllowed,
    /// Cannot use same X-EXTERNAL-ID in same day
    #[error("Conflict")]
    Conflict,
    /// Transaction has previously been processed indicates the same
    /// partnerReferenceNo already success
    #[error("Duplicate partnerReferenceNo")]
    DuplicatePartnerReferenceNo,
    /// Maximum transaction limit exceeded
    #[error("Too Many Requests")]
    TooManyRequests,
    /// General Error
    #[error("General Error")]
    GeneralError,
    /// Unknown internal server failure, please retry the process again
    #[error("Internal Server Error")]
    InternalServerError,
    /// Backend system failure, etc.
    #[error("External Server Error")]
    ExternalServerError,
    /// Timeout from the issuer
    #[error("Timeout")]
    Timeout,

    /// Bridge variant carrying a [`kamu_snap_crypto::Error`]. Only present
    /// when the `crypto` feature is enabled.
    #[cfg(feature = "crypto")]
    #[error("crypto error: {0}")]
    Crypto(#[from] kamu_snap_crypto::Error),
}

impl Error {
    /// Coarse [`Category`] for retry-policy / logging consumers.
    pub fn category(&self) -> Category {
        match self {
            Self::BadRequest => Category::System,
            Self::InvalidFieldFormat(_) => Category::Message,
            Self::InvalidMandatoryField(_) => Category::Message,
            Self::Unauthorized(_) => Category::System,
            Self::InvalidTokenB2B => Category::System,
            Self::InvalidCustomerToken => Category::System,
            Self::TokenNotFoundB2B => Category::System,
            Self::CustomerTokenNotFound => Category::System,
            Self::TransactionExpired => Category::Business,
            Self::FeatureNotAllowed(_) => Category::System,
            Self::ExceedsTransactionAmountLimit => Category::Business,
            Self::SuspectedFraud => Category::Business,
            Self::ActivityCountLimitExceeded => Category::Business,
            Self::DoNotHonor => Category::Business,
            Self::FeatureNotAllowedAtThisTime(_) => Category::System,
            Self::CardBlocked => Category::Business,
            Self::CardExpired => Category::Business,
            Self::DormantAccount => Category::Business,
            Self::NeedToSetTokenLimit => Category::Business,
            Self::OTPBlocked => Category::System,
            Self::OTPLifetimeExpired => Category::System,
            Self::OTPSentToCardholder => Category::System,
            Self::InsufficientFunds => Category::Business,
            Self::TransactionNotPermitted(_) => Category::Business,
            Self::SuspendTransaction => Category::Business,
            Self::TokenLimitExceeded => Category::Business,
            Self::InactiveCardOrAccountOrCustomer => Category::Business,
            Self::MerchantBlacklisted => Category::Business,
            Self::MerchantLimitExceed => Category::Business,
            Self::SetLimitNotAllowed => Category::Business,
            Self::TokenLimitInvalid => Category::Business,
            Self::AccountLimitExceed => Category::Business,
            Self::InvalidTransactionStatus => Category::Business,
            Self::TransactionNotFound => Category::Business,
            Self::InvalidRouting => Category::System,
            Self::BankNotSupportedBySwitch => Category::System,
            Self::TransactionCancelled => Category::Business,
            Self::MerchantNotRegisteredForCardRegistrationServices => Category::Business,
            Self::NeedToRequestOTP => Category::System,
            Self::JourneyNotFound => Category::System,
            Self::InvalidMerchant => Category::Business,
            Self::NoIssuer => Category::Business,
            Self::InvalidAPITransition => Category::System,
            Self::InvalidCardOrAccountOrCustomerOrVirtualAccount(_) => Category::Business,
            Self::InvalidBillOrVirtualAccountWithReason(_) => Category::Business,
            Self::InvalidAmount => Category::Business,
            Self::PaidBill => Category::Business,
            Self::InvalidOTP => Category::System,
            Self::PartnerNotFound => Category::Business,
            Self::InvalidTerminal => Category::Business,
            Self::InconsistentRequest => Category::Business,
            Self::InvalidBillOrVirtualAccount => Category::Business,
            Self::RequestedFunctionIsNotSupported => Category::System,
            Self::RequestedOperationIsNotAllowed => Category::Business,
            Self::Conflict => Category::System,
            Self::DuplicatePartnerReferenceNo => Category::System,
            Self::TooManyRequests => Category::System,
            Self::GeneralError => Category::System,
            Self::InternalServerError => Category::System,
            Self::ExternalServerError => Category::System,
            Self::Timeout => Category::System,
            #[cfg(feature = "crypto")]
            Self::Crypto(_) => Category::System,
        }
    }

    /// HTTP status code mapped per the SNAP BI taxonomy.
    pub fn http_status(&self) -> http::StatusCode {
        match self {
            Self::BadRequest | Self::InvalidFieldFormat(_) | Self::InvalidMandatoryField(_) => {
                http::StatusCode::BAD_REQUEST
            }

            Self::Unauthorized(_)
            | Self::InvalidTokenB2B
            | Self::InvalidCustomerToken
            | Self::TokenNotFoundB2B
            | Self::CustomerTokenNotFound => http::StatusCode::UNAUTHORIZED,

            Self::TransactionExpired
            | Self::FeatureNotAllowed(_)
            | Self::ExceedsTransactionAmountLimit
            | Self::SuspectedFraud
            | Self::ActivityCountLimitExceeded
            | Self::DoNotHonor
            | Self::FeatureNotAllowedAtThisTime(_)
            | Self::CardBlocked
            | Self::CardExpired
            | Self::DormantAccount
            | Self::NeedToSetTokenLimit
            | Self::OTPBlocked
            | Self::OTPLifetimeExpired
            | Self::OTPSentToCardholder
            | Self::InsufficientFunds
            | Self::TransactionNotPermitted(_)
            | Self::SuspendTransaction
            | Self::TokenLimitExceeded
            | Self::InactiveCardOrAccountOrCustomer
            | Self::MerchantBlacklisted
            | Self::MerchantLimitExceed
            | Self::SetLimitNotAllowed
            | Self::TokenLimitInvalid
            | Self::AccountLimitExceed => http::StatusCode::FORBIDDEN,

            Self::InvalidOTP
            | Self::InvalidTransactionStatus
            | Self::TransactionCancelled
            | Self::MerchantNotRegisteredForCardRegistrationServices
            | Self::PaidBill
            | Self::PartnerNotFound
            | Self::JourneyNotFound
            | Self::InvalidMerchant
            | Self::NoIssuer
            | Self::TransactionNotFound
            | Self::InconsistentRequest
            | Self::InvalidAmount
            | Self::InvalidAPITransition
            | Self::InvalidRouting
            | Self::BankNotSupportedBySwitch
            | Self::InvalidTerminal
            | Self::InvalidCardOrAccountOrCustomerOrVirtualAccount(_)
            | Self::InvalidBillOrVirtualAccountWithReason(_)
            | Self::InvalidBillOrVirtualAccount
            | Self::NeedToRequestOTP => http::StatusCode::NOT_FOUND,

            Self::RequestedFunctionIsNotSupported | Self::RequestedOperationIsNotAllowed => {
                http::StatusCode::METHOD_NOT_ALLOWED
            }

            Self::Conflict | Self::DuplicatePartnerReferenceNo => http::StatusCode::CONFLICT,

            Self::TooManyRequests => http::StatusCode::TOO_MANY_REQUESTS,

            Self::GeneralError | Self::InternalServerError | Self::ExternalServerError => {
                http::StatusCode::INTERNAL_SERVER_ERROR
            }

            Self::Timeout => http::StatusCode::GATEWAY_TIMEOUT,

            #[cfg(feature = "crypto")]
            Self::Crypto(_) => http::StatusCode::UNAUTHORIZED,
        }
    }

    /// 2-digit case code embedded in the `responseCode`.
    pub fn case_code(&self) -> u8 {
        match self {
            Self::BadRequest => 0,
            Self::InvalidFieldFormat(_) => 1,
            Self::InvalidMandatoryField(_) => 2,
            Self::Unauthorized(_) => 0,
            Self::InvalidTokenB2B => 1,
            Self::InvalidCustomerToken => 2,
            Self::TokenNotFoundB2B => 3,
            Self::CustomerTokenNotFound => 4,
            Self::TransactionExpired => 0,
            Self::FeatureNotAllowed(_) => 1,
            Self::ExceedsTransactionAmountLimit => 2,
            Self::SuspectedFraud => 3,
            Self::ActivityCountLimitExceeded => 4,
            Self::DoNotHonor => 5,
            Self::FeatureNotAllowedAtThisTime(_) => 6,
            Self::CardBlocked => 7,
            Self::CardExpired => 8,
            Self::DormantAccount => 9,
            Self::NeedToSetTokenLimit => 10,
            Self::OTPBlocked => 11,
            Self::OTPLifetimeExpired => 12,
            Self::OTPSentToCardholder => 13,
            Self::InsufficientFunds => 14,
            Self::TransactionNotPermitted(_) => 15,
            Self::SuspendTransaction => 16,
            Self::TokenLimitExceeded => 17,
            Self::InactiveCardOrAccountOrCustomer => 18,
            Self::MerchantBlacklisted => 19,
            Self::MerchantLimitExceed => 20,
            Self::SetLimitNotAllowed => 21,
            Self::TokenLimitInvalid => 22,
            Self::AccountLimitExceed => 23,
            Self::InvalidTransactionStatus => 0,
            Self::TransactionNotFound => 1,
            Self::InvalidRouting => 2,
            Self::BankNotSupportedBySwitch => 3,
            Self::TransactionCancelled => 4,
            Self::MerchantNotRegisteredForCardRegistrationServices => 5,
            Self::NeedToRequestOTP => 6,
            Self::JourneyNotFound => 7,
            Self::InvalidMerchant => 8,
            Self::NoIssuer => 9,
            Self::InvalidAPITransition => 10,
            Self::InvalidCardOrAccountOrCustomerOrVirtualAccount(_) => 11,
            Self::InvalidBillOrVirtualAccountWithReason(_) => 12,
            Self::InvalidAmount => 13,
            Self::PaidBill => 14,
            Self::InvalidOTP => 15,
            Self::PartnerNotFound => 16,
            Self::InvalidTerminal => 17,
            Self::InconsistentRequest => 18,
            Self::InvalidBillOrVirtualAccount => 19,
            Self::RequestedFunctionIsNotSupported => 0,
            Self::RequestedOperationIsNotAllowed => 1,
            Self::Conflict => 0,
            Self::DuplicatePartnerReferenceNo => 1,
            Self::TooManyRequests => 0,
            Self::GeneralError => 0,
            Self::InternalServerError => 1,
            Self::ExternalServerError => 2,
            Self::Timeout => 0,
            #[cfg(feature = "crypto")]
            Self::Crypto(_) => 0,
        }
    }

    /// Compose the full wire `responseCode` for this variant under `service`.
    pub fn response_code(&self, service: ServiceCode) -> ResponseCode {
        ResponseCode::from_parts(self.http_status().as_u16(), service, self.case_code())
    }

    /// Inverse classifier — given a received `(http, case)`, return the
    /// matching variant with empty payload for string-bearing variants.
    /// `None` for unknown pairs.
    pub fn from_http_and_case(http: u16, case: u8) -> Option<Self> {
        use Error::*;
        Some(match (http, case) {
            (400, 0) => BadRequest,
            (400, 1) => InvalidFieldFormat(String::new()),
            (400, 2) => InvalidMandatoryField(String::new()),

            (401, 0) => Unauthorized(String::new()),
            (401, 1) => InvalidTokenB2B,
            (401, 2) => InvalidCustomerToken,
            (401, 3) => TokenNotFoundB2B,
            (401, 4) => CustomerTokenNotFound,

            (403, 0) => TransactionExpired,
            (403, 1) => FeatureNotAllowed(String::new()),
            (403, 2) => ExceedsTransactionAmountLimit,
            (403, 3) => SuspectedFraud,
            (403, 4) => ActivityCountLimitExceeded,
            (403, 5) => DoNotHonor,
            (403, 6) => FeatureNotAllowedAtThisTime(String::new()),
            (403, 7) => CardBlocked,
            (403, 8) => CardExpired,
            (403, 9) => DormantAccount,
            (403, 10) => NeedToSetTokenLimit,
            (403, 11) => OTPBlocked,
            (403, 12) => OTPLifetimeExpired,
            (403, 13) => OTPSentToCardholder,
            (403, 14) => InsufficientFunds,
            (403, 15) => TransactionNotPermitted(String::new()),
            (403, 16) => SuspendTransaction,
            (403, 17) => TokenLimitExceeded,
            (403, 18) => InactiveCardOrAccountOrCustomer,
            (403, 19) => MerchantBlacklisted,
            (403, 20) => MerchantLimitExceed,
            (403, 21) => SetLimitNotAllowed,
            (403, 22) => TokenLimitInvalid,
            (403, 23) => AccountLimitExceed,

            (404, 0) => InvalidTransactionStatus,
            (404, 1) => TransactionNotFound,
            (404, 2) => InvalidRouting,
            (404, 3) => BankNotSupportedBySwitch,
            (404, 4) => TransactionCancelled,
            (404, 5) => MerchantNotRegisteredForCardRegistrationServices,
            (404, 6) => NeedToRequestOTP,
            (404, 7) => JourneyNotFound,
            (404, 8) => InvalidMerchant,
            (404, 9) => NoIssuer,
            (404, 10) => InvalidAPITransition,
            (404, 11) => InvalidCardOrAccountOrCustomerOrVirtualAccount(String::new()),
            (404, 12) => InvalidBillOrVirtualAccountWithReason(String::new()),
            (404, 13) => InvalidAmount,
            (404, 14) => PaidBill,
            (404, 15) => InvalidOTP,
            (404, 16) => PartnerNotFound,
            (404, 17) => InvalidTerminal,
            (404, 18) => InconsistentRequest,
            (404, 19) => InvalidBillOrVirtualAccount,

            (405, 0) => RequestedFunctionIsNotSupported,
            (405, 1) => RequestedOperationIsNotAllowed,

            (409, 0) => Conflict,
            (409, 1) => DuplicatePartnerReferenceNo,

            (429, 0) => TooManyRequests,

            (500, 0) => GeneralError,
            (500, 1) => InternalServerError,
            (500, 2) => ExternalServerError,

            (504, 0) => Timeout,

            _ => return None,
        })
    }
}
