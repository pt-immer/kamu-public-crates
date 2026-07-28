//! How a Money knows what currency it is: from its type, and only from its type.
//!
//! `Money<USD>` carries no currency data at all — the marker is a ZST, the value is 16 bytes,
//! and a mismatch is a compile error. There is deliberately no runtime-currency variant.
//!
//! **An earlier revision had one** (`Money<Dyn>`, with `CurrencyRepr` selecting a `Tag` of
//! either `()` or `Iso4217`). It was deleted, and the reason is worth keeping: it offered
//! `try_add`/`try_sub`, so it *looked* like money and invited callers to compute in the
//! unchecked mode. C4 removed `Add` from it to discourage exactly that, which is the design
//! admitting the type was a hazard while keeping it. A boundary is a place you pass through,
//! not a place you work, and the schema is what decides where that boundary falls: a column is
//! declared single-currency or mixed in its DDL, so the decode target follows from the type of
//! the column rather than from a Rust-side guess. See C3, and C8's two-type split.

use crate::iso::Iso4217;

/// A currency, known at compile time. Implemented by the generated ISO register,
/// never by hand — and enforced, not requested: the crate-private sealing
/// supertrait is unnameable downstream.
///
/// **`CODE` is the only fact.** Everything else about a currency is derived from it, at compile
/// time: `C::CODE.exponent()`, `C::CODE.alpha3()`, `C::CODE.name()` are all `const fn`. An
/// earlier revision also carried `const EXP: Option<u8>`, duplicating what `Iso4217` already
/// knows — and then needed a generated test across every currency to prove the two copies
/// hadn't diverged. Deleting the duplication makes drift unrepresentable instead of merely
/// tested. Do not reintroduce a derived fact as an associated const.
///
/// Without the seal, a downstream crate could claim `CODE = Iso4217::USD` and
/// impersonate genuine USD anywhere a `Money<C>` is accepted. That counterfeit
/// implementation compiled before the seal was added; the compile-fail suite
/// pins the refusal.
pub trait StaticCurrency: crate::sealed::Sealed {
    /// The ISO 4217 code. The single source of truth for this currency.
    const CODE: Iso4217;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::iso::{IDR, JPY, USD, XAU};

    #[test]
    fn a_currency_reports_its_own_code() {
        assert_eq!(USD::CODE, Iso4217::USD);
        assert_eq!(IDR::CODE, Iso4217::IDR);
    }

    /// `CODE` is the only fact; everything else derives from it at compile time. This is why
    /// there is no `EXP` and no drift test: there is nothing to drift from.
    #[test]
    fn every_currency_fact_derives_from_code_at_compile_time() {
        const USD_EXP: Option<u8> = USD::CODE.exponent();
        const JPY_EXP: Option<u8> = JPY::CODE.exponent();
        const XAU_EXP: Option<u8> = XAU::CODE.exponent();
        assert_eq!(USD_EXP, Some(2));
        assert_eq!(JPY_EXP, Some(0));
        assert_eq!(XAU_EXP, None, "gold has no minor unit");
        assert_eq!(USD::CODE.alpha3(), "USD");
    }

    /// The marker is a ZST, so the currency costs nothing at runtime. This is what makes
    /// deleting the runtime variant free rather than a trade: there was never a size argument
    /// for `Money<Dyn>`, only a convenience one.
    #[test]
    fn a_currency_marker_is_zero_sized() {
        assert_eq!(size_of::<USD>(), 0);
        assert_eq!(size_of::<IDR>(), 0);
    }
}
