//! SNAP BI error taxonomy.

use crate::{CaseCode, Category, ServiceCode, ValidResponseCode};

macro_rules! variant_pattern {
    ($variant:ident) => {
        Self::$variant
    };
    ($variant:ident, $payload:ty) => {
        Self::$variant(..)
    };
}

macro_rules! define_taxonomy {
    (
        $(
            $variant:ident $(($payload:ty))? => (
                $display:literal,
                $message:literal,
                $http:ident,
                $case:literal,
                $category:ident
            );
        )+
    ) => {
        /// Server-side SNAP BI errors.
        ///
        /// Use [`Error::class`] when only stable wire classification is needed.
        #[derive(Debug, thiserror::Error)]
        #[non_exhaustive]
        pub enum Error {
            $(
                #[doc = $message]
                #[error($display)]
                $variant $(($payload))?,
            )+
            /// Classified bridge from `kamu-snap-crypto`.
            #[cfg(feature = "crypto")]
            #[error(transparent)]
            Crypto(#[from] CryptoFailure),
        }

        /// Context-free classification for received and locally generated errors.
        #[derive(
            Debug,
            Clone,
            Copy,
            PartialEq,
            Eq,
            Hash,
            serde::Serialize,
            serde::Deserialize,
        )]
        #[non_exhaustive]
        pub enum ErrorClass {
            $(
                #[doc = $message]
                $variant,
            )+
        }

        impl ErrorClass {
            /// Every taxonomy entry, in wire-table order.
            pub const ALL: &'static [Self] = &[
                $(Self::$variant,)+
            ];

            /// Coarse operational category.
            #[must_use]
            pub const fn category(self) -> Category {
                match self {
                    $(Self::$variant => Category::$category,)+
                }
            }

            /// HTTP status assigned by SNAP BI.
            #[must_use]
            pub const fn http_status(self) -> http::StatusCode {
                match self {
                    $(Self::$variant => http::StatusCode::$http,)+
                }
            }

            /// Two-digit case component.
            #[must_use]
            pub const fn case_code(self) -> CaseCode {
                match self {
                    $(Self::$variant => CaseCode::from_valid($case),)+
                }
            }

            /// Canonical context-free response message.
            #[must_use]
            pub const fn message(self) -> &'static str {
                match self {
                    $(Self::$variant => $message,)+
                }
            }

            /// Rust variant name, suitable for structured logs.
            #[must_use]
            pub const fn as_str(self) -> &'static str {
                match self {
                    $(Self::$variant => stringify!($variant),)+
                }
            }

            /// Compose a valid wire code under `service`.
            #[must_use]
            pub fn response_code(self, service: ServiceCode) -> ValidResponseCode {
                ValidResponseCode::from_parts(self.http_status(), service, self.case_code())
            }

            /// Look up a taxonomy entry without manufacturing missing context.
            #[must_use]
            pub fn from_http_and_case(
                http: http::StatusCode,
                case: CaseCode,
            ) -> Option<Self> {
                Some(match (http, case.get()) {
                    $((http::StatusCode::$http, $case) => Self::$variant,)+
                    _ => return None,
                })
            }
        }

        impl core::fmt::Display for ErrorClass {
            fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                formatter.write_str(self.message())
            }
        }

        impl Error {
            /// Stable context-free classification.
            #[must_use]
            pub fn class(&self) -> ErrorClass {
                match self {
                    $(variant_pattern!($variant $(, $payload)?) => ErrorClass::$variant,)+
                    #[cfg(feature = "crypto")]
                    Self::Crypto(error) => error.class().error_class(),
                }
            }

            /// Coarse operational category.
            #[must_use]
            pub fn category(&self) -> Category {
                self.class().category()
            }

            /// HTTP status assigned by SNAP BI.
            #[must_use]
            pub fn http_status(&self) -> http::StatusCode {
                self.class().http_status()
            }

            /// Two-digit case component.
            #[must_use]
            pub fn case_code(&self) -> CaseCode {
                self.class().case_code()
            }

            /// Compose a valid wire code under `service`.
            #[must_use]
            pub fn response_code(&self, service: ServiceCode) -> ValidResponseCode {
                self.class().response_code(service)
            }

            /// Public wire message. Crypto sources are never disclosed.
            #[must_use]
            pub fn response_message(&self) -> String {
                #[cfg(feature = "crypto")]
                if let Self::Crypto(error) = self {
                    return error.class().error_class().message().to_owned();
                }
                self.to_string()
            }

            /// Crypto-specific operational class, when this error wraps crypto.
            #[cfg(feature = "crypto")]
            #[must_use]
            pub const fn crypto_class(&self) -> Option<CryptoFailureClass> {
                match self {
                    Self::Crypto(error) => Some(error.class()),
                    _ => None,
                }
            }
        }
    };
}

define_taxonomy! {
    BadRequest => ("Bad Request", "Bad Request", BAD_REQUEST, 0, System);
    InvalidFieldFormat(String) => ("Invalid Field Format {0}", "Invalid Field Format", BAD_REQUEST, 1, Message);
    InvalidMandatoryField(String) => ("Invalid Mandatory Field {0}", "Invalid Mandatory Field", BAD_REQUEST, 2, Message);
    Unauthorized(String) => ("Unauthorized. {0}", "Unauthorized", UNAUTHORIZED, 0, System);
    InvalidTokenB2B => ("Invalid Token (B2B)", "Invalid Token (B2B)", UNAUTHORIZED, 1, System);
    InvalidCustomerToken => ("Invalid Customer Token", "Invalid Customer Token", UNAUTHORIZED, 2, System);
    TokenNotFoundB2B => ("Token Not Found (B2B)", "Token Not Found (B2B)", UNAUTHORIZED, 3, System);
    CustomerTokenNotFound => ("Customer Token Not Found", "Customer Token Not Found", UNAUTHORIZED, 4, System);
    TransactionExpired => ("Transaction Expired", "Transaction Expired", FORBIDDEN, 0, Business);
    FeatureNotAllowed(String) => ("Feature Not Allowed {0}", "Feature Not Allowed", FORBIDDEN, 1, System);
    ExceedsTransactionAmountLimit => ("Exceeds Transaction Amount Limit", "Exceeds Transaction Amount Limit", FORBIDDEN, 2, Business);
    SuspectedFraud => ("Suspected Fraud", "Suspected Fraud", FORBIDDEN, 3, Business);
    ActivityCountLimitExceeded => ("Activity Count Limit Exceeded", "Activity Count Limit Exceeded", FORBIDDEN, 4, Business);
    DoNotHonor => ("Do Not Honor", "Do Not Honor", FORBIDDEN, 5, Business);
    FeatureNotAllowedAtThisTime(String) => ("Feature Not Allowed At This Time. {0}", "Feature Not Allowed At This Time", FORBIDDEN, 6, System);
    CardBlocked => ("Card Blocked", "Card Blocked", FORBIDDEN, 7, Business);
    CardExpired => ("Card Expired", "Card Expired", FORBIDDEN, 8, Business);
    DormantAccount => ("Dormant Account", "Dormant Account", FORBIDDEN, 9, Business);
    NeedToSetTokenLimit => ("Need To Set Token Limit", "Need To Set Token Limit", FORBIDDEN, 10, Business);
    OTPBlocked => ("OTP Blocked", "OTP Blocked", FORBIDDEN, 11, System);
    OTPLifetimeExpired => ("OTP Lifetime Expired", "OTP Lifetime Expired", FORBIDDEN, 12, System);
    OTPSentToCardholder => ("OTP Sent To Cardholder", "OTP Sent To Cardholder", FORBIDDEN, 13, System);
    InsufficientFunds => ("Insufficient Funds", "Insufficient Funds", FORBIDDEN, 14, Business);
    TransactionNotPermitted(String) => ("Transaction Not Permitted. {0}", "Transaction Not Permitted", FORBIDDEN, 15, Business);
    SuspendTransaction => ("Suspend Transaction", "Suspend Transaction", FORBIDDEN, 16, Business);
    TokenLimitExceeded => ("Token Limit Exceeded", "Token Limit Exceeded", FORBIDDEN, 17, Business);
    InactiveCardOrAccountOrCustomer => ("Inactive Card/Account/Customer", "Inactive Card/Account/Customer", FORBIDDEN, 18, Business);
    MerchantBlacklisted => ("Merchant Blacklisted", "Merchant Blacklisted", FORBIDDEN, 19, Business);
    MerchantLimitExceed => ("Merchant Limit Exceed", "Merchant Limit Exceed", FORBIDDEN, 20, Business);
    SetLimitNotAllowed => ("Set Limit Not Allowed", "Set Limit Not Allowed", FORBIDDEN, 21, Business);
    TokenLimitInvalid => ("Token Limit Invalid", "Token Limit Invalid", FORBIDDEN, 22, Business);
    AccountLimitExceed => ("Account Limit Exceed", "Account Limit Exceed", FORBIDDEN, 23, Business);
    InvalidTransactionStatus => ("Invalid Transaction Status", "Invalid Transaction Status", NOT_FOUND, 0, Business);
    TransactionNotFound => ("Transaction Not Found", "Transaction Not Found", NOT_FOUND, 1, Business);
    InvalidRouting => ("Invalid Routing", "Invalid Routing", NOT_FOUND, 2, System);
    BankNotSupportedBySwitch => ("Bank Not Supported By Switch", "Bank Not Supported By Switch", NOT_FOUND, 3, System);
    TransactionCancelled => ("Transaction Cancelled", "Transaction Cancelled", NOT_FOUND, 4, Business);
    MerchantNotRegisteredForCardRegistrationServices => ("Merchant Is Not Registered For Card Registration Services", "Merchant Is Not Registered For Card Registration Services", NOT_FOUND, 5, Business);
    NeedToRequestOTP => ("Need To Request OTP", "Need To Request OTP", NOT_FOUND, 6, System);
    JourneyNotFound => ("Journey Not Found", "Journey Not Found", NOT_FOUND, 7, System);
    InvalidMerchant => ("Invalid Merchant", "Invalid Merchant", NOT_FOUND, 8, Business);
    NoIssuer => ("No Issuer", "No Issuer", NOT_FOUND, 9, Business);
    InvalidAPITransition => ("Invalid API Transition", "Invalid API Transition", NOT_FOUND, 10, System);
    InvalidCardOrAccountOrCustomerOrVirtualAccount(String) => ("Invalid Card/Account/Customer {0}/Virtual Account", "Invalid Card/Account/Customer/Virtual Account", NOT_FOUND, 11, Business);
    InvalidBillOrVirtualAccountWithReason(String) => ("Invalid Bill/Virtual Account {0}", "Invalid Bill/Virtual Account", NOT_FOUND, 12, Business);
    InvalidAmount => ("Invalid Amount", "Invalid Amount", NOT_FOUND, 13, Business);
    PaidBill => ("Paid Bill", "Paid Bill", NOT_FOUND, 14, Business);
    InvalidOTP => ("Invalid OTP", "Invalid OTP", NOT_FOUND, 15, System);
    PartnerNotFound => ("Partner Not Found", "Partner Not Found", NOT_FOUND, 16, Business);
    InvalidTerminal => ("Invalid Terminal", "Invalid Terminal", NOT_FOUND, 17, Business);
    InconsistentRequest => ("Inconsistent Request", "Inconsistent Request", NOT_FOUND, 18, Business);
    InvalidBillOrVirtualAccount => ("Invalid Bill/Virtual Account", "Invalid Bill/Virtual Account", NOT_FOUND, 19, Business);
    RequestedFunctionIsNotSupported => ("Requested Function Is Not Supported", "Requested Function Is Not Supported", METHOD_NOT_ALLOWED, 0, System);
    RequestedOperationIsNotAllowed => ("Requested Operation Is Not Allowed", "Requested Operation Is Not Allowed", METHOD_NOT_ALLOWED, 1, Business);
    Conflict => ("Conflict", "Conflict", CONFLICT, 0, System);
    DuplicatePartnerReferenceNo => ("Duplicate partnerReferenceNo", "Duplicate partnerReferenceNo", CONFLICT, 1, System);
    TooManyRequests => ("Too Many Requests", "Too Many Requests", TOO_MANY_REQUESTS, 0, System);
    GeneralError => ("General Error", "General Error", INTERNAL_SERVER_ERROR, 0, System);
    InternalServerError => ("Internal Server Error", "Internal Server Error", INTERNAL_SERVER_ERROR, 1, System);
    ExternalServerError => ("External Server Error", "External Server Error", INTERNAL_SERVER_ERROR, 2, System);
    Timeout => ("Timeout", "Timeout", GATEWAY_TIMEOUT, 0, System);
}

/// Operational class retained when converting a crypto error.
#[cfg(feature = "crypto")]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum CryptoFailureClass {
    /// Remote credentials or signatures failed authentication.
    Authentication,
    /// Inbound request components were malformed.
    InvalidRequest,
    /// Local keys or configuration were invalid.
    Configuration,
    /// A future or unclassified internal failure.
    Internal,
}

#[cfg(feature = "crypto")]
impl CryptoFailureClass {
    /// Classify without exposing the source error on the wire.
    #[must_use]
    pub fn from_error(error: &kamu_snap_crypto::Error) -> Self {
        use kamu_snap_crypto::Error as CryptoError;

        match error {
            CryptoError::InvalidPublicKey(_) | CryptoError::InvalidSecretKey(_) => Self::Configuration,
            CryptoError::SignatureDecode { .. }
            | CryptoError::InvalidRawSignature(_)
            | CryptoError::SymmetricVerifyFailed
            | CryptoError::AsymmetricVerifyFailed => Self::Authentication,
            CryptoError::MissingHeader { name } | CryptoError::InvalidHeader { name }
                if is_auth_header(name) =>
            {
                Self::Authentication
            }
            CryptoError::SnapBiInput(_)
            | CryptoError::MissingHeader { .. }
            | CryptoError::InvalidHeader { .. } => Self::InvalidRequest,
            CryptoError::ServiceVerification(error) => Self::from_service_verification(error),
            _ => Self::Internal,
        }
    }

    /// SNAP BI taxonomy entry exposed to clients.
    #[must_use]
    pub const fn error_class(self) -> ErrorClass {
        match self {
            Self::Authentication => ErrorClass::Unauthorized,
            Self::InvalidRequest => ErrorClass::BadRequest,
            Self::Configuration | Self::Internal => ErrorClass::InternalServerError,
        }
    }

    fn from_service_verification(error: &kamu_snap_crypto::ServiceVerificationError) -> Self {
        use kamu_snap_crypto::ServiceVerificationError as VerificationError;

        match error {
            VerificationError::Authorization(_)
            | VerificationError::InvalidSignatureEncoding
            | VerificationError::InvalidSignatureLength { .. }
            | VerificationError::SignatureMismatch => Self::Authentication,
            VerificationError::MissingHeader { name } | VerificationError::InvalidHeader { name }
                if is_auth_header(name) =>
            {
                Self::Authentication
            }
            VerificationError::MissingHeader { .. }
            | VerificationError::InvalidHeader { .. }
            | VerificationError::InvalidMethod
            | VerificationError::Input(_) => Self::InvalidRequest,
            _ => Self::Internal,
        }
    }
}

#[cfg(feature = "crypto")]
fn is_auth_header(name: &str) -> bool {
    name.eq_ignore_ascii_case("Authorization") || name.eq_ignore_ascii_case("X-SIGNATURE")
}

/// A redacted crypto source plus its stable operational class.
#[cfg(feature = "crypto")]
pub struct CryptoFailure {
    class: CryptoFailureClass,
    source: kamu_snap_crypto::Error,
}

#[cfg(feature = "crypto")]
impl CryptoFailure {
    /// Stable operational class.
    #[must_use]
    pub const fn class(&self) -> CryptoFailureClass {
        self.class
    }

    /// Original source for server-side diagnostics.
    #[must_use]
    pub const fn source_error(&self) -> &kamu_snap_crypto::Error {
        &self.source
    }
}

#[cfg(feature = "crypto")]
impl From<kamu_snap_crypto::Error> for CryptoFailure {
    fn from(source: kamu_snap_crypto::Error) -> Self {
        let class = CryptoFailureClass::from_error(&source);
        Self { class, source }
    }
}

#[cfg(feature = "crypto")]
impl From<kamu_snap_crypto::Error> for Error {
    fn from(source: kamu_snap_crypto::Error) -> Self {
        Self::Crypto(source.into())
    }
}

#[cfg(feature = "crypto")]
impl core::fmt::Debug for CryptoFailure {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.debug_struct("CryptoFailure").field("class", &self.class).finish_non_exhaustive()
    }
}

#[cfg(feature = "crypto")]
impl core::fmt::Display for CryptoFailure {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let message = match self.class {
            CryptoFailureClass::Authentication => "crypto authentication failed",
            CryptoFailureClass::InvalidRequest => "crypto request input is invalid",
            CryptoFailureClass::Configuration => "crypto configuration is invalid",
            CryptoFailureClass::Internal => "crypto operation failed",
        };
        formatter.write_str(message)
    }
}

#[cfg(feature = "crypto")]
impl std::error::Error for CryptoFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.source)
    }
}
