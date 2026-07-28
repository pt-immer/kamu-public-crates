// A downstream crate must NOT be able to hand-implement StaticCurrency.
//
// Before the private sealing supertrait existed, this compiled and ran: a counterfeit
// currency declaring `CODE = Iso4217::USD` impersonated genuine USD through erase()/try_cast().
// The trait's doc comment said "never by hand", which is documentation, not access control.
// Ten agents and four formal reviews missed it because the invariant was on nobody's checklist.
// It is on one now.
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
