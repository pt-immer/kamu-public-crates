// A downstream crate must not be able to implement StaticCurrency.
//
// A counterfeit currency declaring `CODE = Iso4217::USD` could otherwise impersonate genuine
// USD through erase()/try_cast(). Documentation cannot enforce that boundary; the private
// sealing supertrait does.
//
// This compile-fail case pins that boundary.
//
// trybuild compiles this file as its own crate depending on money-core, so the rejection
// proved here is the real downstream rejection, not crate-internal privacy.

use kamu_money_core::StaticCurrency;
use kamu_money_core::iso::Iso4217;

struct FakeUsd;

impl StaticCurrency for FakeUsd {
    const CODE: Iso4217 = Iso4217::USD;
}

fn main() {}
