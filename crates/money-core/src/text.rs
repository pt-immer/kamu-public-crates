//! The canonical text form: `"<ISO> <amount>"`, one trim rule, one parser.
//!
//! This module is **not** feature-gated and does not depend on serde. It is the single place
//! the crate turns money into characters, so that the `Display` a developer reaches for and
//! the wire a service emits cannot disagree; the serde codec delegates here.
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
//! The minimum is the **settlement** exponent, not a display one. Locale display policy stays
//! off the wire — IDR settles at 2 and renders at 0 — so using the
//! settlement number keeps this form canonical and independent of any locale.
//!
//! Padding up to the minimum is the only addition. Trimming never removes a significant digit.
//!
//! # Round-tripping, stated honestly
//!
//! Render is canonical; **parse is liberal**, accepting any exact decimal. So
//! `parse(render(v)) == v` holds for all `v`, but the converse does not — `"USD 10.5"` parses
//! and re-renders as `"USD 10.50"`. The pair is therefore a **retraction**, not a bijection.

use crate::Money;
use crate::Rate;
use crate::StaticCurrency;
use crate::domain_impl::{POW10_SCALE, SCALE, in_domain};
use crate::error_impl::{AmountError, ParseMoneyError, RateError};
use crate::iso::Iso4217;
use core::fmt;
use core::str::FromStr;

/// `10^SCALE` as `u128`, for the unsigned split. Same constant as [`POW10_SCALE`], widened.
const SCALE_U128: u128 = POW10_SCALE.unsigned_abs();

/// [`SCALE`] as `usize`, for string widths and byte offsets.
///
/// `SCALE as usize` would be lossless on every platform this crate can build for, but
/// `clippy::as_conversions` is denied crate-wide precisely so that "provably fine here" is
/// never the reason a cast ships. `usize::try_from` states the proof instead of assuming it,
/// and is not const, which [`parse_fixed_point`] needs. So the width is written once as a
/// literal and *tied* to [`SCALE`] by an assertion the compiler evaluates: move [`SCALE`] and
/// this fails to build, rather than parsing at a width that no longer matches the scale.
const SCALE_USIZE: usize = 18;
const _: () = assert!(SCALE == 18);

/// The magnitude of [`i128::MAX`], as the unsigned accumulator holds it.
///
/// The bound `i128::try_from` would check, written so that a const fn can check it too.
const I128_MAX_MAGNITUDE: u128 = i128::MAX.unsigned_abs();

/// One past a loop index already known to sit inside a slice.
///
/// `index + 1` is denied by `clippy::arithmetic_side_effects`, which cannot see that bound.
/// The `expect` is the bound, said out loud.
const fn next(index: usize) -> usize {
    index.checked_add(1).expect("an index inside a slice that exists cannot reach usize::MAX")
}

/// The value of an ASCII digit, widened for accumulation. `None` for anything else.
///
/// A match rather than `byte - b'0'` widened by a cast: `u128::from` is not const, and
/// `clippy::as_conversions` is denied crate-wide. Being **total** is the other half of the
/// bargain — the byte-subtraction form needed an `expect` to restate that its input really was
/// a digit, and this one cannot be reached with a byte it has not classified.
const fn digit_value(byte: u8) -> Option<u128> {
    Some(match byte {
        b'0' => 0,
        b'1' => 1,
        b'2' => 2,
        b'3' => 3,
        b'4' => 4,
        b'5' => 5,
        b'6' => 6,
        b'7' => 7,
        b'8' => 8,
        b'9' => 9,
        _ => return None,
    })
}

/// Which overflow a magnitude that outgrew its accumulator is, given the sign already read.
const fn magnitude_overflow(negative: bool) -> ParseMoneyError {
    if negative {
        ParseMoneyError::NegativeMagnitudeOverflow
    } else {
        ParseMoneyError::PositiveMagnitudeOverflow
    }
}

/// Shift one decimal place and add `byte`'s digit.
///
/// Every step is checked: the text is untrusted, and an overflow here would otherwise be the
/// silent corruption this crate exists to prevent.
const fn push_digit(magnitude: u128, byte: u8, negative: bool) -> Result<u128, ParseMoneyError> {
    let Some(digit) = digit_value(byte) else {
        return Err(ParseMoneyError::InvalidSyntax);
    };
    let Some(shifted) = magnitude.checked_mul(10) else {
        return Err(magnitude_overflow(negative));
    };
    match shifted.checked_add(digit) {
        Some(value) => Ok(value),
        None => Err(magnitude_overflow(negative)),
    }
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

/// Render `units` at [`SCALE`] places, trimmed to `min_dp`, with its sign. Shared by every
/// text form in the crate so `Money` and `Rate` cannot drift apart on the digits.
fn render_fixed_point(units: i128, min_dp: usize) -> String {
    let (negative, whole, digits) = fixed_point_parts(units, min_dp);
    let sign = if negative { "-" } else { "" };

    if digits.is_empty() { format!("{sign}{whole}") } else { format!("{sign}{whole}.{digits}") }
}

/// Parse a fixed-point decimal into canonical units. Liberal: any exact decimal.
///
/// Refuses excess precision rather than rounding.
///
/// `const`, so that a literal can be validated where it is written. Const evaluation admits no
/// iterators, no closures, no `?` and no `TryFrom`, so this reads bytes by index and matches
/// every `Result` explicitly. It remains the crate's **only** parser: a const twin could accept
/// a literal the runtime parser rejects, and nothing would notice until a golden moved.
pub(crate) const fn parse_fixed_point(text: &str) -> Result<i128, ParseMoneyError> {
    let bytes = text.as_bytes();
    let len = bytes.len();

    let negative = len > 0 && bytes[0] == b'-';
    let body_start = if negative { 1 } else { 0 };

    // One scan locates the point and counts the fraction. `split_once` took the *first* point
    // and let any second one fail the digit check below it; scanning reaches both verdicts
    // directly.
    let mut point = len; // `len` reads as "no point", so nothing has to unwrap an `Option`
    let mut supplied: u32 = 0;
    let mut index = body_start;
    while index < len {
        let byte = bytes[index];
        if byte == b'.' {
            // Belt and braces, and knowingly so: mutation-testing this guard away leaves every
            // verdict unchanged, because `point` would then track the LAST point and the
            // earlier one would fall inside the whole part, where `push_digit` rejects it. It
            // stays because refusing a second point *here* says why, at the byte that caused
            // it, rather than as a digit that turned out not to be one.
            if point != len {
                return Err(ParseMoneyError::InvalidSyntax);
            }
            point = index;
        } else if !byte.is_ascii_digit() {
            return Err(ParseMoneyError::InvalidSyntax);
        } else if point != len {
            // Counted as `u32` while scanning rather than narrowed from a `usize` afterwards:
            // `u32::try_from` is not const, and a fraction too long to count must keep its
            // original verdict of `InvalidSyntax` rather than becoming an overflow.
            supplied = match supplied.checked_add(1) {
                Some(count) => count,
                None => return Err(ParseMoneyError::InvalidSyntax),
            };
        }
        index = next(index);
    }

    // An empty whole part is refused, so `"-"`, `""` and `".5"` are all invalid syntax.
    if point == body_start {
        return Err(ParseMoneyError::InvalidSyntax);
    }
    if supplied > SCALE {
        return Err(ParseMoneyError::ExcessPrecision { digits: supplied });
    }

    // Right-pad the fraction to the canonical scale, then read whole and fraction as one
    // integer.
    let mut magnitude: u128 = 0;
    let mut whole = body_start;
    while whole < point {
        magnitude = match push_digit(magnitude, bytes[whole], negative) {
            Ok(value) => value,
            Err(error) => return Err(error),
        };
        whole = next(whole);
    }

    let frac_start = if point == len { len } else { next(point) };
    let frac_len = len.checked_sub(frac_start).expect("the fraction starts at or before the end");
    let mut place = 0;
    while place < SCALE_USIZE {
        // The supplied digits first, then zero padding out to the canonical scale.
        let byte = if place < frac_len {
            bytes[frac_start.checked_add(place).expect("the fraction lies inside the input")]
        } else {
            b'0'
        };
        magnitude = match push_digit(magnitude, byte, negative) {
            Ok(value) => value,
            Err(error) => return Err(error),
        };
        place = next(place);
    }

    // `i128::MIN` has no positive counterpart, which is why the accumulator is unsigned.
    if negative && magnitude == i128::MIN.unsigned_abs() {
        return Ok(i128::MIN);
    }
    if magnitude > I128_MAX_MAGNITUDE {
        return Err(magnitude_overflow(negative));
    }
    // The bound above is exactly what `i128::try_from` checks, so the two-complement bit
    // pattern is the value. Stated as a re-read of the bytes because the conversion is not
    // const and a cast is denied crate-wide; the guard is the proof, in code rather than prose.
    let units = i128::from_le_bytes(magnitude.to_le_bytes());
    if negative {
        Ok(units.checked_neg().expect("a positive i128 magnitude can always be negated"))
    } else {
        Ok(units)
    }
}

/// Parse the bare decimal carried by a structured rate representation.
///
/// Unlike [`parse_amount`], this does not apply the money domain. [`Rate`]'s
/// constructor owns both the domain and strictly-positive checks, so every rate
/// ingress reaches the same validation edge.
#[cfg(feature = "serde")]
pub(crate) const fn parse_rate_amount(text: &str) -> Result<i128, ParseMoneyError> {
    parse_fixed_point(text)
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
/// The non-generic twin of [`Money`]'s [`Display`](core::fmt::Display), used by runtime-currency
/// boundaries such as PostgreSQL. Sharing this implementation prevents adapter-specific trim
/// rules.
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
/// `const`, so a literal can be checked where it is written — which is what
/// [`money!`](crate::money!) is built on. It remains the crate's only parser: a const twin could
/// accept a literal this one rejects, and nothing would notice until a golden moved.
pub const fn parse_amount(text: &str) -> Result<i128, ParseMoneyError> {
    let units = match parse_fixed_point(text) {
        Ok(units) => units,
        Err(error) => return Err(error),
    };
    // parse_fixed_point stops at what an i128 can hold; the DOMAIN is narrower. The generic
    // path got this check from `Money::try_from_units`, so a caller taking raw units needs it
    // applied here or the two paths would not agree on what is in range. Same `in_domain`
    // that `try_from_units` calls, rather than a restatement of the bound.
    if !in_domain(units) {
        // `Into` is not const, so the variant `#[from]` generates is named directly.
        return Err(ParseMoneyError::Amount(AmountError::out_of_domain(units)));
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

/// Honour width and alignment without letting formatter precision truncate the canonical text.
///
/// Currency policy owns fractional digits. With no formatting options, `pad` is byte-identical
/// to `write_str`.
fn pad_without_truncating(f: &mut fmt::Formatter<'_>, rendered: &str) -> fmt::Result {
    if f.precision().is_some() { f.write_str(rendered) } else { f.pad(rendered) }
}

/// A rate is a RATIO, not an amount, so no currency's minor unit governs it: it trims all the
/// way down to the last significant digit. `USD/IDR/16000`, never `USD/IDR/16000.00`.
///
/// Shape is ISO 15022 field 92B's: `:4!c//3!a/3!a/15d`, e.g. `:92B::EXCH//GBP/USD/1,619`
/// meaning 1,00 GBP = 1,619 USD — first code the BASE, second the QUOTE. One deviation,
/// deliberate: the decimal separator is a point, not SWIFT's comma, for exact decimal transport
/// into JavaScript, where `parseFloat("1,619")`
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
mod legacy_equivalence {
    //! The pre-`const` parser, kept as a reference, and differentially compared.
    //!
    //! [`parse_fixed_point`] was rewritten from iterators and `?` into indexed loops so that it
    //! could be `const`. A rewrite of the crate's only parser is exactly the change that can
    //! pass every hand-written case and still have moved a boundary: which of two errors is
    //! reported first, whether a trailing point is still accepted, where a second `.` is caught.
    //!
    //! So the old body is preserved verbatim below and the two are compared on every string
    //! over a money-shaped alphabet up to length five — 19,608 inputs, exhaustive rather than
    //! sampled — plus a property run over long and large-magnitude inputs that short strings
    //! cannot reach. Agreement is on the **full** `Result`, so a changed error variant fails
    //! just as loudly as a changed value.

    use super::{ParseMoneyError, SCALE, SCALE_USIZE, parse_fixed_point};
    use proptest::prelude::*;

    /// The parser exactly as it stood before the `const` rewrite.
    ///
    /// Not tidied, not modernised. Its value is being the previous behaviour, so any edit to it
    /// destroys the only thing it is for.
    #[allow(clippy::arithmetic_side_effects, clippy::as_conversions)]
    fn legacy_parse_fixed_point(text: &str) -> Result<i128, ParseMoneyError> {
        let (negative, digits) = match text.strip_prefix('-') {
            Some(rest) => (true, rest),
            None => (false, text),
        };

        let (whole_text, frac_text) = match digits.split_once('.') {
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

        let mut magnitude: u128 = 0;
        for byte in
            whole_text.bytes().chain(frac_text.bytes().chain(core::iter::repeat(b'0')).take(SCALE as usize))
        {
            let digit = u128::from(
                byte.checked_sub(b'0').expect("bytes were verified as ASCII digits, or are padding zeros"),
            );
            magnitude = magnitude.checked_mul(10).and_then(|shifted| shifted.checked_add(digit)).ok_or(
                if negative {
                    ParseMoneyError::NegativeMagnitudeOverflow
                } else {
                    ParseMoneyError::PositiveMagnitudeOverflow
                },
            )?;
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

    /// Every structural shape a short input can take: both signs, the point in every position,
    /// missing parts, repeated separators, and a non-digit to force the syntax arm.
    const ALPHABET: [u8; 7] = [b'0', b'1', b'9', b'-', b'.', b'a', b' '];

    #[test]
    fn the_rewrite_agrees_with_the_previous_parser_on_every_short_input() {
        let mut checked = 0_u32;
        for length in 0..=5_usize {
            let count = ALPHABET.len().pow(u32::try_from(length).expect("length is at most five"));
            for mut code in 0..count {
                let mut text = String::with_capacity(length);
                for _ in 0..length {
                    text.push(char::from(ALPHABET[code % ALPHABET.len()]));
                    code /= ALPHABET.len();
                }
                assert_eq!(
                    parse_fixed_point(&text),
                    legacy_parse_fixed_point(&text),
                    "the const rewrite changed the verdict for {text:?}",
                );
                checked = checked.checked_add(1).expect("the corpus is far below u32::MAX");
            }
        }
        // A positive control on the loop itself: a silent off-by-one in the odometer would
        // otherwise let this test pass having compared almost nothing.
        assert_eq!(checked, 19_608, "the exhaustive corpus did not have the size it claims");
    }

    /// Every fraction width from none to well past the scale, both signs.
    ///
    /// The exhaustive pass above stops at five characters, so it cannot reach an eighteen-digit
    /// fraction — the boundary `supplied > SCALE` decides. Leaving that to the property run
    /// would leave it to chance: a `>` weakened to `>=` changes the verdict for exactly one
    /// width out of the forty-two swept here.
    #[test]
    fn the_rewrite_agrees_at_every_fraction_width_around_the_scale() {
        for sign in ["", "-"] {
            for width in 0..=20_usize {
                let text = format!("{sign}1.{}", "7".repeat(width));
                assert_eq!(
                    parse_fixed_point(&text),
                    legacy_parse_fixed_point(&text),
                    "the const rewrite changed the verdict at fraction width {width}",
                );
            }
        }
        // The boundary is where it is claimed to be, in case both parsers were rewritten
        // together and agreed on the wrong answer.
        let at_scale = format!("1.{}", "7".repeat(SCALE_USIZE));
        let past_scale = format!("1.{}", "7".repeat(SCALE_USIZE + 1));
        assert!(parse_fixed_point(&at_scale).is_ok(), "the scale itself must parse");
        assert_eq!(
            parse_fixed_point(&past_scale),
            Err(ParseMoneyError::ExcessPrecision { digits: SCALE + 1 }),
        );
    }

    /// The exact `i128` limits, which no generated corpus reaches by chance.
    ///
    /// Two branches turn on a single value each — the `i128::MIN` special case, and the
    /// `magnitude > i128::MAX` narrowing guard — and both need a 39-digit magnitude with the
    /// point eighteen places from the right. Mutation-testing the harness found that weakening
    /// either comparison to `>=`, or deleting the `i128::MIN` arm outright, survived every
    /// other test in this module. These are the literals that kill those mutants.
    ///
    /// `parse_amount` rejects all four as out of domain, which is exactly why they belong
    /// *here*: this is the layer where the difference is observable.
    #[test]
    fn the_rewrite_agrees_on_the_exact_i128_limits() {
        for text in [
            // i128::MAX, and one canonical unit either side of it.
            "170141183460469231731.687303715884105727",
            "170141183460469231731.687303715884105726",
            "170141183460469231731.687303715884105728",
            // i128::MIN, whose magnitude has no positive counterpart, and its neighbours.
            "-170141183460469231731.687303715884105728",
            "-170141183460469231731.687303715884105727",
            "-170141183460469231731.687303715884105729",
        ] {
            assert_eq!(
                parse_fixed_point(text),
                legacy_parse_fixed_point(text),
                "the const rewrite changed the verdict at an i128 limit: {text:?}",
            );
        }
        // Pinned absolutely as well, so both parsers cannot drift here together.
        assert_eq!(parse_fixed_point("170141183460469231731.687303715884105727"), Ok(i128::MAX));
        assert_eq!(parse_fixed_point("-170141183460469231731.687303715884105728"), Ok(i128::MIN));
        assert_eq!(
            parse_fixed_point("170141183460469231731.687303715884105728"),
            Err(ParseMoneyError::PositiveMagnitudeOverflow),
        );
        assert_eq!(
            parse_fixed_point("-170141183460469231731.687303715884105729"),
            Err(ParseMoneyError::NegativeMagnitudeOverflow),
        );
    }

    /// Magnitudes stepping across the accumulator's limits, which short inputs cannot reach.
    #[test]
    fn the_rewrite_agrees_where_the_accumulator_runs_out() {
        for digits in 1..=45_usize {
            for sign in ["", "-"] {
                let text = format!("{sign}{}", "9".repeat(digits));
                assert_eq!(
                    parse_fixed_point(&text),
                    legacy_parse_fixed_point(&text),
                    "the const rewrite changed the verdict at {digits} whole digits",
                );
            }
        }
    }

    proptest! {
        /// Long inputs, which the exhaustive pass cannot reach: fractions at and past the
        /// scale, and magnitudes that exhaust the accumulator.
        #[test]
        fn the_rewrite_agrees_on_long_and_large_inputs(
            text in r"-?[0-9]{0,40}\.?[0-9]{0,40}",
        ) {
            prop_assert_eq!(parse_fixed_point(&text), legacy_parse_fixed_point(&text));
        }

        /// Arbitrary bytes, so the syntax arms are compared on input neither parser was
        /// designed around, including multi-byte UTF-8.
        #[test]
        fn the_rewrite_agrees_on_arbitrary_text(text in ".{0,24}") {
            prop_assert_eq!(parse_fixed_point(&text), legacy_parse_fixed_point(&text));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::iso::{IDR, USD};

    /// Short and zero-extended fractional forms must produce the same units.
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
        for units in [crate::domain_impl::DOMAIN_MAX, -crate::domain_impl::DOMAIN_MAX, 0, 1, -1] {
            let m = Money::<IDR>::try_from_units(units).unwrap();
            assert_eq!(Money::<IDR>::from_str(&m.to_string()).unwrap(), m, "{units}");
        }
    }
}
