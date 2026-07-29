//! Build and sign an outbound SNAP BI service request.

use http::Method;
use kamu_snap_crypto::HmacSigner;
use kamu_snap_crypto::snap_bi::{
    AccessToken, ServiceRequest, ServiceStringToSign, Unsigned, now_jakarta_seconds,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client_secret = b"client-secret-001";
    let access_token = AccessToken::from_credential("oauth-access-token")?;
    let timestamp = now_jakarta_seconds();
    let method = Method::POST;
    let body = br#"{"partnerReferenceNo":"abc-123","amount":{"value":"10000.00","currency":"IDR"}}"#;

    let canonical = ServiceStringToSign::new(
        &method,
        "/snap/v1.0/transfer-intrabank/payment",
        access_token,
        body,
        &timestamp,
    )?;
    let unsigned: ServiceRequest<'_, Unsigned> =
        ServiceRequest::new(canonical, "client-key-001", "12345", "000000001")?;
    let signed = unsigned.sign(&HmacSigner::new(client_secret));
    let headers = signed.headers().into_pairs();

    // Values include credentials and signatures. Log names, never values.
    println!(
        "prepared {} signed headers: {}",
        headers.len(),
        headers.iter().map(|(name, _)| *name).collect::<Vec<_>>().join(", "),
    );

    Ok(())
}
