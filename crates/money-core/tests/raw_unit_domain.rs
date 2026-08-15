//! Every public raw-unit entry point enforces the money domain.
//!
//! These take a bare `i128`, so none of them inherits `Money<C>`'s constructor proof. The claim
//! spans `advanced::arithmetic` and `text`, which is why it is a crate test rather than a module
//! one: no single module owns it.

use core::num::NonZeroU32;
use kamu_money_core::Rounding;
use kamu_money_core::advanced::arithmetic::{allocate_units, div_int_units};
use kamu_money_core::advanced::domain::DOMAIN_MAX;
use kamu_money_core::iso::Iso4217;

/// The raw-unit entry points documented their domain precondition and did not enforce it.
/// `i128::MAX` went in and out-of-domain values came back — parts no `Money` constructor
/// would admit, returned as though they were money.
#[test]
fn the_raw_unit_entry_points_refuse_values_no_money_could_hold() {
    let three = NonZeroU32::new(3).unwrap();
    for out_of_domain in [i128::MAX, i128::MIN, DOMAIN_MAX + 1, -DOMAIN_MAX - 1] {
        assert!(
            kamu_money_core::text::render(out_of_domain, Iso4217::USD).is_err(),
            "render accepted {out_of_domain}"
        );
        assert!(allocate_units(out_of_domain, &[1, 1]).is_err(), "allocate_units accepted {out_of_domain}");
        assert!(
            div_int_units(out_of_domain, three, Rounding::TowardZero).is_err(),
            "div_int_units accepted {out_of_domain}"
        );
    }

    // ...and the domain edges themselves still work. A check that rejected everything would
    // pass the assertions above while breaking every real caller.
    for edge in [DOMAIN_MAX, -DOMAIN_MAX, 0, 1, -1] {
        assert!(kamu_money_core::text::render(edge, Iso4217::USD).is_ok(), "edge {edge}");
        assert!(allocate_units(edge, &[1, 1]).is_ok(), "edge {edge}");
        assert!(div_int_units(edge, three, Rounding::TowardZero).is_ok(), "edge {edge}");
    }
}
