//! Parse a SNAP BI response from the wire, classify error responses into
//! typed [`Error`] variants, and dispatch on [`Category`].
//!
//! Demonstrates the recommended client-side path post v2.0:
//!
//! 1. Deserialise with the consumer's typed payload `T`.
//! 2. Inspect `envelope.response_code` defensively — malformed codes do not
//!    blow up the parse; `.raw()` is always available.
//! 3. Map back to a typed [`Error`] variant via [`ResponseCode::classify`] and
//!    decide retry policy from [`Error::category`].

use kamu_snap_response::{Category, SnapResponse};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct BalancePayload {
    #[serde(rename = "accountNo")]
    account_no: String,
    #[serde(rename = "currentBalance")]
    current_balance: String,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let wires = [
        r#"{"responseCode":"2001100","responseMessage":"Successful","accountNo":"1234","currentBalance":"99000.00"}"#,
        r#"{"responseCode":"4011100","responseMessage":"Unauthorized. Invalid token"}"#,
        r#"{"responseCode":"4031114","responseMessage":"Insufficient Funds"}"#,
        // BRI's 6-digit source-doc defect — still parseable, raw preserved.
        r#"{"responseCode":"500000","responseMessage":"General Error"}"#,
    ];

    for wire in wires {
        println!("---");
        println!("wire: {wire}");

        let resp: SnapResponse<BalancePayload> = serde_json::from_str(wire)?;
        println!("raw responseCode: {}", resp.envelope.response_code.raw());

        match resp.envelope.response_code.classify() {
            Some(err) => {
                println!("classified error: {err}");
                println!("category: {}", err.category());
                let action = match err.category() {
                    Category::Business => "do NOT retry (business rule violation)",
                    Category::System => "retry with backoff",
                    Category::Message => "fix client request before retry",
                    Category::Success => "no action",
                    _ => "review manually",
                };
                println!("policy: {action}");
            }
            None => {
                if let Some(payload) = resp.payload() {
                    println!("success! account {} balance {}", payload.account_no, payload.current_balance);
                } else {
                    println!("unrecognized code with no payload — log and escalate");
                }
            }
        }
    }
    Ok(())
}
