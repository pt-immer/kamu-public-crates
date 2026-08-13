//! A literal outside the money domain must fail the BUILD, not construct and refuse later.
//!
//! The domain is applied by `text::parse_amount`, the same function every runtime ingress
//! reaches. A macro that checked the domain itself would be the second place that rule lives.

use kamu_money_core::iso::USD;
use kamu_money_core::Money;

fn main() {
    // One canonical unit past the top of the domain.
    const TOO_LARGE: Money<USD> = kamu_money_core::money!(USD, "1000000000000000000");
    let _ = TOO_LARGE;
}
