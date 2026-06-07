//! Sign an outbound SNAP BI service request end-to-end.
//!
//! Demonstrates the recommended client-side path:
//!
//! 1. Build canonical `stringToSign` via [`ServiceStringToSign`].
//! 2. Compute HMAC-SHA512 via [`sign_service`].
//! 3. Convert to wire encoding (base64 by default; BRI doc-style hex shown for
//!    comparison).
//! 4. Build the canonical SNAP BI header set via [`ServiceHeaders::builder`].

use http::Method;
use kamu_snap_crypto::Encoding;
use kamu_snap_crypto::snap_bi::headers::ServiceHeaders;
use kamu_snap_crypto::snap_bi::{ServiceStringToSign, now_jakarta_seconds, sign_service};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client_id = "client-key-001";
    let client_secret = b"client-secret-001";
    let bearer_token = "eyJhbGciOiJSUzI1NiIs..."; // from prior OAuth call

    let method = Method::POST;
    let path = "/snap/v1.0/transfer-intrabank/payment";
    let body = br#"{"partnerReferenceNo":"abc-123","amount":{"value":"10000.00","currency":"IDR"}}"#;
    let timestamp = now_jakarta_seconds();

    let parts = ServiceStringToSign {
        method: &method,
        path,
        access_token: bearer_token,
        body,
        timestamp: &timestamp,
    };

    let sig = sign_service(client_secret, &parts)?;
    println!("base64 signature: {}", sig.encode(Encoding::Base64));
    println!("hex signature:    {}", sig.encode(Encoding::HexLower));

    let headers = ServiceHeaders::builder()
        .partner_id(client_id)
        .channel_id("12345")
        .external_id("000000001")
        .timestamp(&timestamp)
        .signature(sig.to_base64())
        .bearer_token(bearer_token)
        .build()?;

    println!("\ncanonical headers:");
    for (name, value) in headers.into_pairs() {
        println!("  {name}: {value}");
    }

    Ok(())
}
