//! Writing money as text: the trim rule, and the two forms built on it.

use super::SCALE_USIZE;
use crate::Money;
#[cfg(feature = "serde")]
use crate::Rate;
use crate::StaticCurrency;
use crate::domain::{POW10_SCALE, in_domain};
use crate::errors::AmountError;
use crate::iso::Iso4217;

/// `10^SCALE` as `u128`, for the unsigned split. Same constant as [`POW10_SCALE`], widened.
const SCALE_U128: u128 = POW10_SCALE.unsigned_abs();
/// The digits of `units` at [`SCALE`](crate::advanced::domain::SCALE) places, trimmed to `min_dp`, as `(negative, whole,
/// fraction)`.
///
/// The trim rule itself, with nothing assembled yet. [`render_fixed_point`] joins the parts
/// with a point; [`crate::locale`] groups the whole part and joins with a locale's own
/// separator, which it cannot do by splitting a finished string — a locale whose *group*
/// separator is `.` (German, Indonesian) would have no way to tell the two roles apart.
///
/// One function so that the canonical form and every display form cannot disagree about what
/// the digits ARE. They are allowed to differ in `min_dp`, in separators, and in decoration;
/// they are not allowed to differ in the number.
pub(crate) fn fixed_point_parts(units: i128, min_dp: usize) -> (bool, String, String) {
    // `i128::MIN` has no positive counterpart. Formatting remains total even for a corrupted
    // internal value.
    let magnitude = units.unsigned_abs();
    let whole = magnitude.checked_div(SCALE_U128).expect("SCALE_U128 is 10^18, never zero");
    let frac = magnitude.checked_rem(SCALE_U128).expect("SCALE_U128 is 10^18, never zero");

    let mut digits = format!("{frac:0SCALE_USIZE$}");
    // Trim to `min_dp` but stop at the first non-zero digit from the right.
    while digits.len() > min_dp && digits.ends_with('0') {
        digits.pop();
    }

    (units < 0, whole.to_string(), digits)
}

/// Render `units` at [`SCALE`](crate::advanced::domain::SCALE) places, trimmed to `min_dp`, with its sign. Shared by every
/// text form in the crate so `Money` and `Rate` cannot drift apart on the digits.
pub(crate) fn render_fixed_point(units: i128, min_dp: usize) -> String {
    let (negative, whole, digits) = fixed_point_parts(units, min_dp);
    let sign = if negative { "-" } else { "" };

    if digits.is_empty() { format!("{sign}{whole}") } else { format!("{sign}{whole}.{digits}") }
}
/// Render `units` as `"<ISO> <amount>"` for a currency known only at **run time**.
///
/// The non-generic twin of [`Money`](crate::Money)'s [`Display`](core::fmt::Display), used by runtime-currency
/// boundaries such as PostgreSQL. Sharing this implementation prevents adapter-specific trim
/// rules.
/// # Errors
/// [`AmountError`] if `units` is outside the domain.
///
/// The check is not defensive padding. Without it this function emitted **canonical-looking
/// text that its own [`parse`](crate::text::parse) refuses** — `render(i128::MAX, USD)` produced
/// `"USD 170141183460469231731.687303715884105727"`, which the parser rejects as
/// [`ParseMoneyError::Amount`](crate::errors::ParseMoneyError::Amount). A renderer whose output its parser rejects is a silent corruption
/// waiting for a caller that trusts the pair, which is exactly what an adapter does.
/// [`Money`](crate::Money)'s `Display` cannot reach this arm: `Money<C>` is in-domain by construction.
pub fn render(units: i128, currency: Iso4217) -> Result<String, AmountError> {
    if !in_domain(units) {
        return Err(AmountError::out_of_domain(units));
    }
    // `None` means the currency genuinely has no minor unit (gold), which is 0 places, not
    // "unknown" — the same reading Display uses.
    let min_dp = usize::from(currency.exponent().unwrap_or(0));
    Ok(format!("{} {}", currency.alpha3(), render_fixed_point(units, min_dp)))
}
/// The amount half of a money literal, with no currency prefix: `"10.50"`.
///
/// For a boundary that carries the currency **out of band** — a structured wire form whose
/// sibling field names it, or a database column whose *type* fixes it. Repeating the code
/// inside the number would be nonsense there. Same digits as
/// [`Display`](core::fmt::Display), same rule, one implementation.
///
/// # Why this takes `Money<C>` and returns no `Result`
///
/// The typed input is the whole point. `Money<C>` is in-domain by construction and carries its
/// own code, so there is no incoherent state left to report — contrast a loose
/// `(units, currency)` pair, which can be out of domain and whose two halves nothing ties
/// together. A renderer over that pair would need an error variant that this one does not,
/// which is a defect in the pair rather than a feature of the renderer.
#[must_use]
pub fn render_amount<C: StaticCurrency>(m: Money<C>) -> String {
    render_fixed_point(m.units(), usize::from(C::CODE.exponent().unwrap_or(0)))
}

/// The rate half of a rate literal, with no pair prefix: `"16000"`.
#[cfg(feature = "serde")]
pub(crate) fn render_rate<Base: StaticCurrency, Quote: StaticCurrency>(r: Rate<Base, Quote>) -> String {
    render_fixed_point(r.units(), 0)
}

#[cfg(test)]
mod tests {
    use crate::domain::DOMAIN_MAX;
    use crate::iso::{JPY, KWD, USD, XAU};
    use crate::{Iso4217, Money, text};

    fn usd(units: i128) -> Money<USD> {
        Money::<USD>::try_from_units(units).unwrap()
    }

    /// The whole trim rule, as a table. One stored value, four settlement exponents.
    #[test]
    fn the_minimum_width_is_the_iso_settlement_exponent() {
        let half = 10_500_000_000_000_000_000; // 10.5
        assert_eq!(Money::<USD>::try_from_units(half).unwrap().to_string(), "USD 10.50"); // exp 2
        assert_eq!(Money::<JPY>::try_from_units(half).unwrap().to_string(), "JPY 10.5"); // exp 0
        assert_eq!(Money::<KWD>::try_from_units(half).unwrap().to_string(), "KWD 10.500"); // exp 3
        assert_eq!(Money::<XAU>::try_from_units(half).unwrap().to_string(), "XAU 10.5"); // None -> 0

        let whole = 10_000_000_000_000_000_000; // 10
        assert_eq!(Money::<USD>::try_from_units(whole).unwrap().to_string(), "USD 10.00");
        assert_eq!(Money::<JPY>::try_from_units(whole).unwrap().to_string(), "JPY 10");
        assert_eq!(Money::<KWD>::try_from_units(whole).unwrap().to_string(), "KWD 10.000");
        assert_eq!(Money::<XAU>::try_from_units(whole).unwrap().to_string(), "XAU 10");
    }
    /// Every significant digit survives, all the way down to one canonical unit.
    ///
    /// Padding up to the settlement exponent is allowed; dropping a significant fractional digit is
    /// not.
    #[test]
    fn trimming_never_rounds() {
        assert_eq!(usd(10_123_456_789_000_000_000).to_string(), "USD 10.123456789");
        assert_eq!(usd(1).to_string(), "USD 0.000000000000000001");
        assert_eq!(usd(-1).to_string(), "USD -0.000000000000000001");
        assert_eq!(usd(DOMAIN_MAX).to_string(), "USD 999999999999999999.999999999999999999");
    }
    #[test]
    fn zero_and_sign_render_correctly() {
        assert_eq!(Money::<USD>::ZERO.to_string(), "USD 0.00");
        assert_eq!(Money::<JPY>::ZERO.to_string(), "JPY 0");
        assert_eq!(usd(-10_500_000_000_000_000_000).to_string(), "USD -10.50");
        // -0 does not exist: there is one zero, and it has no sign.
        assert_eq!(usd(0).to_string(), "USD 0.00");
    }
    /// Every currency, both paths, same string. `Display` delegates to `render`, so this pins the
    /// delegation rather than a coincidence.
    #[test]
    fn the_runtime_codec_renders_exactly_what_display_renders() {
        let units = 10_500_000_000_000_000_000; // 10.5
        assert_eq!(text::render(units, Iso4217::USD).unwrap(), usd(units).to_string());
        assert_eq!(
            text::render(units, Iso4217::JPY).unwrap(),
            Money::<JPY>::try_from_units(units).unwrap().to_string()
        );
        assert_eq!(
            text::render(units, Iso4217::KWD).unwrap(),
            Money::<KWD>::try_from_units(units).unwrap().to_string()
        );
        assert_eq!(
            text::render(units, Iso4217::XAU).unwrap(),
            Money::<XAU>::try_from_units(units).unwrap().to_string()
        );
    }
}
