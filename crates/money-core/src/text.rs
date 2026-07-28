//! The canonical text form: `"<ISO> <amount>"`, one trim rule, one parser.
//!
//! This module is **not** feature-gated and does not depend on serde. It is the single place
//! the crate turns money into characters, so that the `Display` a developer reaches for and
//! the wire a service emits cannot disagree — the serde codec (C7) is a thin wrapper over
//! exactly this.
//!
//! # The rule
//!
//! Render at [`SCALE`] digits, strip trailing zeros, **stop at the currency's ISO settlement
//! exponent** (`None` for XAU/XDR/XXX, treated as 0). Never round.
//!
//! ```text
//! stored 10.500000000000000000   ->   USD 10.50   JPY 10.5   KWD 10.500   XAU 10.5
//! stored 10.000000000000000000   ->   USD 10.00   JPY 10     KWD 10.000   XAU 10
//! stored  0.000000000000000001   ->   USD 0.000000000000000001            (nothing is dropped)
//! ```
//!
//! The minimum is the **settlement** exponent, not a display one. C2 keeps display dp in
//! `LocalePolicy` and off the wire — IDR settles at 2 and renders at 0 — so using the
//! settlement number keeps this form canonical and independent of any locale.
//!
//! Padding up to the minimum is the only thing added, which is what makes trimming compatible
//! with §0.1's *"display pads, never rounds"*: every significant digit survives.
//!
//! # Round-tripping, stated honestly
//!
//! Render is canonical; **parse is liberal**, accepting any exact decimal. So
//! `parse(render(v)) == v` holds for all `v`, but the converse does not — `"USD 10.5"` parses
//! and re-renders as `"USD 10.50"`. That makes the pair a **retraction**, not the bijection
//! C7 originally claimed.

use crate::currency::StaticCurrency;
use crate::domain::{POW10_SCALE, SCALE, in_domain};
use crate::error::{AmountError, ParseMoneyError, RateError};
use crate::iso::Iso4217;
use crate::money::Money;
use crate::rate::Rate;
use core::fmt;
use core::str::FromStr;

/// `10^SCALE` as `u128`, for the unsigned split. Same constant as [`POW10_SCALE`], widened.
const SCALE_U128: u128 = POW10_SCALE.unsigned_abs();

/// [`SCALE`] as `usize`, for string widths.
///
/// `SCALE as usize` would be lossless on every platform this crate can build for, but
/// `clippy::as_conversions` is denied crate-wide precisely so that "provably fine here" is
/// never the reason a cast ships. `try_from` states the proof instead of assuming it.
fn scale_usize() -> usize {
    usize::try_from(SCALE).expect("SCALE is 18; usize is at least 16 bits on every target")
}

/// The digits of `units` at [`SCALE`] places, trimmed to `min_dp`, as `(negative, whole,
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
    // `unsigned_abs` rather than `abs`: i128::MIN has no positive counterpart, and while it is
    // outside the domain, a Display impl must not be the thing that panics on a corrupted
    // value. (DESIGN.md E7)
    let magnitude = units.unsigned_abs();
    let whole = magnitude.checked_div(SCALE_U128).expect("SCALE_U128 is 10^18, never zero");
    let frac = magnitude.checked_rem(SCALE_U128).expect("SCALE_U128 is 10^18, never zero");

    let mut digits = format!("{frac:0width$}", width = scale_usize());
    // Trims DOWN to `min_dp` and pads UP to it, and does neither past a significant digit:
    // the loop stops at the first non-zero from the right. That is §0.1's "display pads,
    // never rounds" expressed as the only line of code that could violate it.
    while digits.len() > min_dp && digits.ends_with('0') {
        digits.pop();
    }

    (units < 0, whole.to_string(), digits)
}

/// Render `units` at [`SCALE`] places, trimmed to `min_dp`, with its sign. Shared by every
/// text form in the crate so `Money` and `Rate` cannot drift apart on the digits.
fn render_fixed_point(units: i128, min_dp: usize) -> String {
    let (negative, whole, digits) = fixed_point_parts(units, min_dp);
    let sign = if negative { "-" } else { "" };

    if digits.is_empty() { format!("{sign}{whole}") } else { format!("{sign}{whole}.{digits}") }
}

/// Parse a fixed-point decimal into canonical units. Liberal: any exact decimal.
///
/// Refuses excess precision rather than rounding — the failure that disqualified
/// `rust_decimal` (E2).
fn parse_fixed_point(text: &str) -> Result<i128, ParseMoneyError> {
    let (negative, digits) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text),
    };

    let (whole_text, frac_text) = match digits.split_once('.') {
        // A second '.' lands in `frac_text` and is rejected by the digit check below.
        Some((w, fr)) => (w, fr),
        None => (digits, ""),
    };

    if whole_text.is_empty() || !whole_text.bytes().all(|b| b.is_ascii_digit()) {
        return Err(ParseMoneyError::InvalidSyntax);
    }
    if !frac_text.bytes().all(|b| b.is_ascii_digit()) {
        return Err(ParseMoneyError::InvalidSyntax);
    }

    let supplied = u32::try_from(frac_text.len()).map_err(|_| ParseMoneyError::InvalidSyntax)?;
    if supplied > SCALE {
        return Err(ParseMoneyError::ExcessPrecision { digits: supplied });
    }

    // Right-pad the fraction to the canonical scale, then read the whole thing as one
    // integer. Every step is checked: the text is untrusted, and an overflow here would
    // otherwise be the silent corruption this crate exists to prevent.
    let mut magnitude: u128 = 0;
    for byte in
        whole_text.bytes().chain(frac_text.bytes().chain(core::iter::repeat(b'0')).take(scale_usize()))
    {
        // Every byte here is either an ASCII digit (checked above) or a padding b'0', so the
        // subtraction cannot underflow. `checked_sub` states that rather than relying on it.
        let digit = u128::from(
            byte.checked_sub(b'0').expect("bytes were verified as ASCII digits, or are padding zeros"),
        );
        magnitude =
            magnitude.checked_mul(10).and_then(|shifted| shifted.checked_add(digit)).ok_or(if negative {
                ParseMoneyError::NegativeMagnitudeOverflow
            } else {
                ParseMoneyError::PositiveMagnitudeOverflow
            })?;
    }

    if negative && magnitude == i128::MIN.unsigned_abs() {
        return Ok(i128::MIN);
    }

    let units = i128::try_from(magnitude).map_err(|_| {
        if negative {
            ParseMoneyError::NegativeMagnitudeOverflow
        } else {
            ParseMoneyError::PositiveMagnitudeOverflow
        }
    })?;
    if negative {
        Ok(units.checked_neg().expect("positive i128 magnitude can always be negated"))
    } else {
        Ok(units)
    }
}

/// Split `"<ISO> <amount>"` into its currency and its still-unparsed amount.
///
/// Separate from [`parse`] so that [`Money`]'s `FromStr` can compare the code against `C`
/// *before* reading the digits. Getting `WrongCurrency` back from `"IDR not-a-number"` is more
/// use at an API boundary than `InvalidSyntax`, and a single combined parser could not offer
/// that ordering to one caller and the plain answer to the other.
fn split_tagged(text: &str) -> Result<(Iso4217, &str), ParseMoneyError> {
    let (code_text, amount) = text.split_once(' ').ok_or(ParseMoneyError::InvalidSyntax)?;
    let code = Iso4217::from_alpha3(code_text).ok_or(ParseMoneyError::InvalidSyntax)?;
    Ok((code, amount))
}

/// Render `units` as `"<ISO> <amount>"` for a currency known only at **run time**.
///
/// The non-generic twin of [`Money`]'s [`Display`](core::fmt::Display), sharing its
/// implementation rather than
/// resembling it. A PostgreSQL type cannot be generic — C8 measured why the currency has to
/// travel in the value there — so without this, every adapter reimplements the trim rule and
/// C9's "thin over one codec" becomes a wish. Drift here is a *silent* wrong number in one
/// system and a right one in another, which is the failure mode this crate exists to remove.
/// # Errors
/// [`AmountError`] if `units` is outside the domain.
///
/// The check is not defensive padding. Without it this function emitted **canonical-looking
/// text that its own [`parse`] refuses** — `render(i128::MAX, USD)` produced
/// `"USD 170141183460469231731.687303715884105727"`, which the parser rejects as
/// [`ParseMoneyError::Amount`]. A renderer whose output its parser rejects is a silent corruption
/// waiting for a caller that trusts the pair, which is exactly what an adapter does.
/// [`Money`]'s `Display` cannot reach this arm: `Money<C>` is in-domain by construction.
pub fn render(units: i128, currency: Iso4217) -> Result<String, AmountError> {
    if !in_domain(units) {
        return Err(AmountError::out_of_domain(units));
    }
    // `None` means the currency genuinely has no minor unit (gold), which is 0 places, not
    // "unknown" — the same reading Display uses.
    let min_dp = usize::from(currency.exponent().unwrap_or(0));
    Ok(format!("{} {}", currency.alpha3(), render_fixed_point(units, min_dp)))
}

/// Parse a bare amount — no currency prefix — into domain-checked canonical units.
///
/// # Errors
/// [`ParseMoneyError::InvalidSyntax`] for malformed input,
/// [`ParseMoneyError::ExcessPrecision`] above [`SCALE`] fractional digits,
/// [`ParseMoneyError::PositiveMagnitudeOverflow`] or
/// [`ParseMoneyError::NegativeMagnitudeOverflow`] when the signed canonical-unit
/// value cannot fit `i128`,
/// and [`ParseMoneyError::Amount`] outside the money domain.
pub fn parse_amount(text: &str) -> Result<i128, ParseMoneyError> {
    let units = parse_fixed_point(text)?;
    // parse_fixed_point stops at what an i128 can hold; the DOMAIN is narrower. The generic
    // path got this check from `Money::try_from_units`, so a caller taking raw units needs it
    // applied here or the two paths would not agree on what is in range. Same `in_domain`
    // that `try_from_units` calls, rather than a restatement of the bound.
    if !in_domain(units) {
        return Err(AmountError::out_of_domain(units).into());
    }
    Ok(units)
}

/// Parse `"<ISO> <amount>"` without knowing the currency at compile time.
///
/// For adapters that must accept **whatever currency arrives** — a database type, a wire
/// boundary — where [`Money`]'s `FromStr` cannot be used because there is no `C` to name.
/// Returns the currency alongside the units rather than resolving it, because deciding what a
/// mismatch means is the caller's job: `Money<C>`'s `FromStr` treats it as an error, while a
/// storage type stores it.
///
/// # Errors
/// As [`parse_amount`], plus [`ParseMoneyError::InvalidSyntax`] if the string is not
/// `<code> <amount>` or the code is not a known ISO 4217 alpha-3.
pub fn parse(text: &str) -> Result<(Iso4217, i128), ParseMoneyError> {
    let (code, amount) = split_tagged(text)?;
    Ok((code, parse_amount(amount)?))
}

/// The amount half of a money literal, with no currency prefix: `"10.50"`.
///
/// The structured wire form carries the currency in its own field, so repeating it inside the
/// number would be nonsense. Same digits as [`Display`], same rule, one implementation.
#[cfg(feature = "serde")]
pub(crate) fn render_amount<C: StaticCurrency>(m: Money<C>) -> String {
    render_fixed_point(m.units(), usize::from(C::CODE.exponent().unwrap_or(0)))
}

/// The rate half of a rate literal, with no pair prefix: `"16000"`.
#[cfg(feature = "serde")]
pub(crate) fn render_rate<Base: StaticCurrency, Quote: StaticCurrency>(r: Rate<Base, Quote>) -> String {
    render_fixed_point(r.units(), 0)
}

impl<C: StaticCurrency> fmt::Display for Money<C> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Delegates to the same [`render`] a runtime-currency caller gets. Two implementations
        // agreeing today is not the same property as one implementation.
        //
        // The `expect` is unreachable: `Money<C>` cannot hold out-of-domain units — every
        // constructor goes through `try_from_units`, and the raw `i128` is not publicly reachable.
        // That is precisely why `render` takes the check and `Display` does not need one.
        let rendered = render(self.units(), C::CODE).expect("Money<C> is in-domain by construction");
        pad_without_truncating(f, &rendered)
    }
}

/// Honour the formatter's width and alignment, and REFUSE its precision.
///
/// `f.write_str` ignores both, so `format!("{:>16}", money)` silently produced `"USD 10.00"`
/// with no padding and no diagnostic — and right-aligning amounts in a column is the most
/// common thing anyone asks of this type.
///
/// `f.pad` fixes that but brings a hazard with it: `pad` treats `precision` as a **truncation**,
/// so `{:.2}` would render `Money` as `"US"`. A mangled amount in front of a human is the
/// failure this crate is organised around, so precision is ignored rather than honoured. There
/// is no sensible meaning for it here anyway — the number of fractional digits is the
/// currency's to decide, not the format string's, and a caller who wants a different width
/// wants [`crate::locale::LocalePolicy`].
///
/// This does NOT touch the frozen output. `to_string()` and `{}` build a `Formatter` with
/// width, precision and alignment all `None`, and `pad` in that case is exactly `write_str` —
/// so all five consumers of the canonical form see byte-identical bytes.
fn pad_without_truncating(f: &mut fmt::Formatter<'_>, rendered: &str) -> fmt::Result {
    if f.precision().is_some() { f.write_str(rendered) } else { f.pad(rendered) }
}

/// A rate is a RATIO, not an amount, so no currency's minor unit governs it: it trims all the
/// way down to the last significant digit. `USD/IDR/16000`, never `USD/IDR/16000.00`.
///
/// Shape is ISO 15022 field 92B's: `:4!c//3!a/3!a/15d`, e.g. `:92B::EXCH//GBP/USD/1,619`
/// meaning 1,00 GBP = 1,619 USD — first code the BASE, second the QUOTE. One deviation,
/// deliberate: the decimal separator is a POINT, not SWIFT's comma, because C7's reason for
/// using a string at all is exact decimal transport into JavaScript, and `parseFloat("1,619")`
/// is `1`.
impl<Base: StaticCurrency, Quote: StaticCurrency> fmt::Display for Rate<Base, Quote> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let rendered = format!(
            "{}/{}/{}",
            Base::CODE.alpha3(),
            Quote::CODE.alpha3(),
            render_fixed_point(self.units(), 0)
        );
        pad_without_truncating(f, &rendered)
    }
}

impl<Base: StaticCurrency, Quote: StaticCurrency> FromStr for Rate<Base, Quote> {
    type Err = RateError;

    /// Parse `"<BASE>/<QUOTE>/<rate>"`, checking **both** ends of the pair.
    ///
    /// # Errors
    /// [`ParseMoneyError::WrongCurrency`] if either end disagrees; the base is reported first,
    /// because accepting a reversed pair would invert the price — the one error a quote feed
    /// can make that still looks like a number.
    /// Other parse failures are wrapped by [`RateError::Parse`].
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut parts = s.split('/');
        // The trailing `None` is load-bearing: it rejects "USD/IDR/1/2". Without it a fourth
        // field would be silently ignored, which on a wire is how a value gets misread.
        let (Some(base), Some(quote), Some(amount), None) =
            (parts.next(), parts.next(), parts.next(), parts.next())
        else {
            return Err(ParseMoneyError::InvalidSyntax.into());
        };

        let base = Iso4217::from_alpha3(base).ok_or(ParseMoneyError::InvalidSyntax)?;
        if base != Base::CODE {
            return Err(ParseMoneyError::WrongCurrency { expected: Base::CODE, found: base }.into());
        }
        let quote = Iso4217::from_alpha3(quote).ok_or(ParseMoneyError::InvalidSyntax)?;
        if quote != Quote::CODE {
            return Err(ParseMoneyError::WrongCurrency { expected: Quote::CODE, found: quote }.into());
        }

        let units = parse_fixed_point(amount)?;
        // Preserve the constructor's distinction between magnitude and sign failures.
        Self::try_from_units(units)
    }
}

impl<C: StaticCurrency> FromStr for Money<C> {
    type Err = ParseMoneyError;

    /// Parse `"<ISO> <amount>"`.
    ///
    /// Liberal in what it accepts — any exact decimal, canonical or not — and strict in what
    /// it refuses. In particular it **never rounds**: more fractional digits than [`SCALE`]
    /// is [`ParseMoneyError::ExcessPrecision`], not a quietly truncated value.
    ///
    /// # Errors
    /// [`ParseMoneyError::WrongCurrency`] if the code does not match `C`. This redundancy is the
    /// point: it catches an IDR amount arriving in a USD field.
    /// [`ParseMoneyError::InvalidSyntax`] for malformed input;
    /// [`ParseMoneyError::ExcessPrecision`] for more than [`SCALE`] fractional digits;
    /// a sign-specific magnitude overflow when the canonical-unit value cannot fit `i128`;
    /// and [`ParseMoneyError::Amount`] outside the fixed domain.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // Split first, compare the currency, and only then read the digits — so
        // `"IDR not-a-number"` into a `Money<USD>` reports the currency, which is the useful
        // error at a boundary. Calling `parse` here instead would report the digits.
        let (code, amount) = split_tagged(s)?;
        if code != C::CODE {
            return Err(ParseMoneyError::WrongCurrency { expected: C::CODE, found: code });
        }

        let units = parse_amount(amount)?;
        Self::try_from_units(units).map_err(ParseMoneyError::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::iso::{IDR, USD};

    /// The parser reads the fraction by PADDING, not by scaling — so a short fraction and its
    /// zero-extended form must land on the same units. Mutation-check: change the `take` to
    /// `SCALE - 1` and this goes red.
    #[test]
    fn short_and_padded_fractions_agree() {
        let a = Money::<USD>::from_str("USD 1.5").unwrap();
        let b = Money::<USD>::from_str("USD 1.500000000000000000").unwrap();
        assert_eq!(a, b);
        assert_eq!(a.units(), 1_500_000_000_000_000_000);
    }

    #[test]
    fn the_currency_cross_check_fires_before_the_amount_is_read() {
        // "not a number" would also fail, but the CURRENCY error is the useful one to get
        // back: it names the actual mistake at an API boundary.
        assert_eq!(
            Money::<USD>::from_str("IDR not-a-number"),
            Err(ParseMoneyError::WrongCurrency { expected: Iso4217::USD, found: Iso4217::IDR })
        );
    }

    #[test]
    fn display_and_parse_agree_on_the_domain_edges() {
        for units in [crate::domain::DOMAIN_MAX, -crate::domain::DOMAIN_MAX, 0, 1, -1] {
            let m = Money::<IDR>::try_from_units(units).unwrap();
            assert_eq!(Money::<IDR>::from_str(&m.to_string()).unwrap(), m, "{units}");
        }
    }
}
