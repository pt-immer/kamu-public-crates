//! How a Money knows what currency it is: from its type, and only from its type.
//!
//! `Money<USD>` carries a zero-sized currency marker, and a mismatch is a compile error. Runtime
//! currency values belong at parsing and database boundaries, not in arithmetic.

use crate::iso::Iso4217;

/// A currency, known at compile time. Implemented by the generated ISO register,
/// never by hand — and enforced, not requested: the crate-private sealing
/// supertrait is unnameable downstream.
///
/// `CODE` is the only stored fact. Exponent, alpha-3 code, and name derive from it at compile
/// time.
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
