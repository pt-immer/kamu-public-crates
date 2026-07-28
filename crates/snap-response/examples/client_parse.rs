//! Parse success, failure, and malformed SNAP BI responses.

use kamu_snap_response::{Category, SnapResponse};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BalancePayload {
    account_no: String,
    current_balance: String,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let wires = [
        r#"{"responseCode":"2001100","responseMessage":"Successful","accountNo":"1234","currentBalance":"99000.00"}"#,
        r#"{"responseCode":"4031114","responseMessage":"Insufficient Funds"}"#,
        r#"{"responseCode":"500000","responseMessage":"General Error"}"#,
    ];

    for wire in wires {
        let response: SnapResponse<BalancePayload> = serde_json::from_str(wire)?;

        match response {
            SnapResponse::Success(success) => {
                let payload = success.payload();
                println!("account {} balance {}", payload.account_no, payload.current_balance);
            }
            SnapResponse::Failure(failure) => {
                let policy = match failure.error_class().map(|class| class.category()) {
                    Some(Category::Business) => "do not retry",
                    Some(Category::Message) => "fix the request",
                    Some(Category::System) => "retry with backoff",
                    _ => "review manually",
                };
                println!("{}: {policy}", failure.response_code(),);
            }
            SnapResponse::Malformed(malformed) => {
                println!(
                    "malformed responseCode {:?}: {}",
                    malformed.response_code().as_str(),
                    malformed.response_message()
                );
            }
            _ => println!("unsupported response state"),
        }
    }

    Ok(())
}
