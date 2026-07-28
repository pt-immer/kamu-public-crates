//! The display form: symbol, grouping, and a **minimum** fraction width. (DESIGN.md C2)
//!
//! # Why this is not `Display`
//!
//! [`Display`](core::fmt::Display) is **frozen**. It backs five consumers — itself, the serde
//! wire (C7), `kmoney`'s input/output functions (C8), the `postgres`/`sqlx` stored form (C9),
//! and the phase-4/phase-5 differential asserting all of them agree. Changing its output would
//! change an on-disk format and a wire format at the same time, in one edit, silently.
//!
//! So locale-aware rendering is a **separate entry point** and never routes through `Display`.
//! What the two DO share is `text::fixed_point_parts` — the digits themselves — which is the
//! opposite of drift: the canonical form and every display form are allowed to disagree about
//! separators and decoration, and are structurally unable to disagree about the number.
//! (Deliberately not an intra-doc link: the function is `pub(crate)`, and linking public docs
//! to a private item is a strict-rustdoc error.)
//!
//! # The one rule that constrains everything here
//!
//! §0.1: **display pads, never rounds.** A policy owns the *minimum* fraction width; the
//! value's own significant digits own the *maximum*. This is not a stylistic limit — a
//! renderer that truncated to the display width would print a number that is not the stored
//! number, which is the "second number claiming to be the money" the axiom exists to reject.
//!
//! IDR is the case the spec names and the reason the two widths cannot be one field: it
//! **settles** at 2dp (ISO 4217) and **displays** at 0dp (market practice). A naive
//! "display exponent" would render `16000.50` as `Rp 16.000`, losing half a thousand rupiah
//! to a formatting decision.
//!
//! ```
//! use kamu_money_core::iso::IDR;
//! use kamu_money_core::locale::ID_IDR;
//! use kamu_money_core::advanced::domain::POW10_SCALE;
//! use kamu_money_core::Money;
//!
//! let m = Money::<IDR>::try_from_units(16_000 * POW10_SCALE + POW10_SCALE / 2).unwrap();
//! assert_eq!(m.to_string(), "IDR 16000.50");            // canonical: settles at 2
//! assert_eq!(ID_IDR.render(m).unwrap(), "Rp 16.000,5"); // display: 0 minimum, nothing lost
//! ```
//!
//! # What this module deliberately is not
//!
//! **Not a locale database.** The four constants below are worked examples, sized to cover
//! prefix and suffix symbols, both separator conventions, and a currency whose display width
//! is below its settlement width. Real locale data is CLDR's, it is large, and it changes on
//! someone else's schedule; baking a snapshot of it into a money crate would create a table
//! that rots silently. Build a [`LocalePolicy`] from whatever CLDR/ICU source the application
//! already carries.
//!
//! **No accounting parentheses, no `NegativeStyle`.** C2 scopes this to symbol, grouping and
//! minimum width, and §0.3 admits complexity only where it deletes a demonstrable failure. A
//! leading `-` is unambiguous and correct; `(1,234.50)` is a preference. It was considered and
//! cut, which is the same call that deleted `to_minor_units()` and `StaticCurrency::EXP`.

use crate::Money;
use crate::StaticCurrency;
use crate::error_impl::{AmountError, LocaleError};
use crate::iso::Iso4217;
use crate::text::fixed_point_parts;

/// Where the currency symbol sits relative to the digits.
///
/// Any space between symbol and digits belongs to the symbol string itself (`"Rp "`), so a
/// locale wanting a non-breaking space can spell one and this enum stays a position rather
/// than becoming a layout engine.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum SymbolPosition {
    /// `$1,234.50`
    Prefix,
    /// `1.234,50 €`
    Suffix,
}

/// A validated minimum fraction width for locale rendering.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug, Hash)]
pub struct FractionDigits(u8);

impl FractionDigits {
    /// No required fractional digits.
    pub const ZERO: Self = Self(0);

    /// The largest supported width, equal to the crate's fixed scale.
    pub const MAX: Self = Self(18);

    /// Validate a minimum fraction width.
    ///
    /// # Errors
    ///
    /// Returns [`LocaleError::FractionDigitsOutOfRange`] above the fixed scale.
    pub const fn try_new(digits: u8) -> Result<Self, LocaleError> {
        if digits <= Self::MAX.0 {
            Ok(Self(digits))
        } else {
            Err(LocaleError::FractionDigitsOutOfRange { digits })
        }
    }

    /// Return the validated width.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }
}

const _: () = assert!(crate::SCALE == 18);

/// How one locale shows one currency.
///
/// A policy is the pairing of a locale (separators, grouping, symbol placement) with a
/// currency (the symbol, the display width) — which is why it carries an [`Iso4217`] and
/// [`render`](LocalePolicy::render) is fallible. An Indonesian reader viewing a US dollar
/// balance wants Indonesian separators around a dollar symbol; that is a different policy
/// from [`ID_IDR`], not the same one reused.
///
/// # Why rendering can fail
///
/// The currency is checked on the way out exactly as [`FromStr`](core::str::FromStr) checks it
/// on the way in. Without it, formatting a `Money<USD>` through [`ID_IDR`] would emit `Rp` in
/// front of dollars — a mislabelled amount reaching a human, which is the failure this crate
/// is organised around, arriving through the one surface a human actually reads.
///
/// # Why the lifetime, and why not `'static`
///
/// The module docs above tell an application to build a policy from whatever CLDR/ICU source it
/// already carries. Until 2026-07-27 that instruction was **not followable**: every field was
/// `&'static`, so locale data read from a file or a database at startup could only be used by
/// leaking it, by building an unrelated global cache, or by pasting it back into source. The
/// `'static` was there to keep the constructors `const`, which is a compile-time convenience —
/// and it had extended past the invariant it was protecting to break the documented runtime path.
/// Locale decoration does not need static lifetime to remain exact.
///
/// `LocalePolicy<'a>` fixes that at no cost to the shipped constants, which are simply
/// `LocalePolicy<'static>` and still `const`. There is deliberately no owned or `Cow` form: no
/// consumer needs one yet, and adding one before there is a real caller would be inventing an
/// API to sit beside a working one. (DESIGN.md C2)
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct LocalePolicy<'a> {
    currency: Iso4217,
    symbol: &'a str,
    symbol_position: SymbolPosition,
    group_separator: &'a str,
    decimal_separator: &'a str,
    grouping: &'a [u8],
    min_fraction_digits: FractionDigits,
}

impl<'a> LocalePolicy<'a> {
    /// A policy for `currency` with western defaults: `$1,234.50`.
    ///
    /// The default minimum is the currency's **ISO settlement exponent**, so a policy that is
    /// never further configured renders the canonical width and cannot surprise anyone. Only
    /// a currency whose market practice differs — IDR — needs
    /// [`with_min_fraction_digits`](Self::with_min_fraction_digits), and that difference then
    /// appears explicitly at the one site where it is true.
    #[must_use]
    pub const fn new(currency: Iso4217, symbol: &'a str) -> Self {
        Self {
            currency,
            symbol,
            symbol_position: SymbolPosition::Prefix,
            group_separator: ",",
            decimal_separator: ".",
            grouping: &[3],
            // `None` is "no minor unit" (gold), which is zero places, not unknown — the same
            // reading the canonical form uses. `unwrap_or` is not const-stable; `match` is.
            min_fraction_digits: match currency.exponent() {
                Some(e) => FractionDigits(e),
                None => FractionDigits::ZERO,
            },
        }
    }

    /// Place the symbol before or after the digits.
    #[must_use]
    pub const fn with_symbol_position(mut self, position: SymbolPosition) -> Self {
        self.symbol_position = position;
        self
    }

    /// Set the group and decimal separators.
    ///
    /// An empty group separator disables grouping.
    ///
    /// # Errors
    ///
    /// Returns [`LocaleError::EmptyDecimalSeparator`] for an empty decimal
    /// separator and [`LocaleError::AmbiguousSeparators`] when both non-empty
    /// separators are equal.
    pub fn try_with_separators(
        mut self,
        group_separator: &'a str,
        decimal_separator: &'a str,
    ) -> Result<Self, LocaleError> {
        if decimal_separator.is_empty() {
            return Err(LocaleError::EmptyDecimalSeparator);
        }
        if !group_separator.is_empty() && group_separator == decimal_separator {
            return Err(LocaleError::AmbiguousSeparators);
        }
        self.group_separator = group_separator;
        self.decimal_separator = decimal_separator;
        Ok(self)
    }

    /// Set the digit grouping, read right-to-left, with the **last entry repeating**.
    ///
    /// `&[3]` is the western group-of-three. `&[3, 2]` is the Indian lakh/crore shape —
    /// `1,23,45,678` — which is the whole reason this is a slice and not a single number.
    /// `&[]` disables grouping.
    ///
    /// # Errors
    ///
    /// Returns [`LocaleError::ZeroGroupingWidth`] for a zero entry.
    pub const fn try_with_grouping(mut self, grouping: &'a [u8]) -> Result<Self, LocaleError> {
        let mut index = 0;
        while index < grouping.len() {
            if grouping[index] == 0 {
                return Err(LocaleError::ZeroGroupingWidth { index });
            }
            index = index.saturating_add(1);
        }
        self.grouping = grouping;
        Ok(self)
    }

    /// Set the **minimum** fraction width. It can never become a maximum.
    ///
    #[must_use]
    pub const fn with_min_fraction_digits(mut self, digits: FractionDigits) -> Self {
        self.min_fraction_digits = digits;
        self
    }

    /// The currency this policy is for.
    #[must_use]
    pub const fn currency(&self) -> Iso4217 {
        self.currency
    }

    /// The symbol, including any space it carries.
    #[must_use]
    pub const fn symbol(&self) -> &'a str {
        self.symbol
    }

    /// Where the symbol sits.
    #[must_use]
    pub const fn symbol_position(&self) -> SymbolPosition {
        self.symbol_position
    }

    /// The group separator.
    #[must_use]
    pub const fn group_separator(&self) -> &'a str {
        self.group_separator
    }

    /// The decimal separator.
    #[must_use]
    pub const fn decimal_separator(&self) -> &'a str {
        self.decimal_separator
    }

    /// The grouping sizes, right-to-left, last repeating.
    #[must_use]
    pub const fn grouping(&self) -> &'a [u8] {
        self.grouping
    }

    /// The **minimum** fraction width. Never a maximum.
    #[must_use]
    pub const fn min_fraction_digits(&self) -> FractionDigits {
        self.min_fraction_digits
    }

    /// Render a `Money<C>` under this policy.
    ///
    /// # Errors
    /// [`LocaleError::WrongCurrency`] if `C` is not this policy's currency.
    pub fn render<C: StaticCurrency>(&self, money: Money<C>) -> Result<String, LocaleError> {
        self.render_units(money.units(), C::CODE)
    }

    /// Render raw units for a currency known only at **run time**.
    ///
    /// The non-generic twin, for the same callers [`crate::text::render`] exists for: a
    /// database row or a wire message whose currency is a value rather than a type. Shares
    /// the generic path's implementation rather than resembling it.
    ///
    /// # Errors
    /// [`LocaleError::WrongCurrency`] if `currency` is not this policy's currency, and
    /// [`LocaleError::Amount`] if `units` is outside the domain.
    ///
    /// The domain arm matters more here than anywhere else in the crate: this is the one
    /// function whose output a **human reads and acts on**. Returning `Ok` for an amount the
    /// type cannot hold would put a number in front of a person that no other layer would
    /// accept — the failure this crate exists to prevent, arriving through its last surface.
    pub fn render_units(&self, units: i128, currency: Iso4217) -> Result<String, LocaleError> {
        if currency != self.currency {
            return Err(LocaleError::WrongCurrency { expected: self.currency, found: currency });
        }
        if !crate::domain_impl::in_domain(units) {
            return Err(AmountError::out_of_domain(units).into());
        }

        let (negative, whole, fraction) =
            fixed_point_parts(units, usize::from(self.min_fraction_digits.get()));

        let grouped = group(&whole, self.grouping, self.group_separator);
        let body = if fraction.is_empty() {
            grouped
        } else {
            format!("{grouped}{}{fraction}", self.decimal_separator)
        };

        let decorated = match self.symbol_position {
            SymbolPosition::Prefix => format!("{}{body}", self.symbol),
            SymbolPosition::Suffix => format!("{body}{}", self.symbol),
        };

        // The sign is outermost, ahead of a prefixed symbol: `-$1,234.50`. One rule for both
        // placements, so a reader never has to hunt for it.
        Ok(if negative { format!("-{decorated}") } else { decorated })
    }
}

/// Insert `separator` into `digits` per `sizes`, read right-to-left, last size repeating.
fn group(digits: &str, sizes: &[u8], separator: &str) -> String {
    if sizes.is_empty() || separator.is_empty() {
        return digits.to_owned();
    }

    // `digits` comes from the fixed-point formatter and is ASCII, so byte offsets are char
    // boundaries here and no slice below can split a character.
    //
    // The two `expect`s below were `unwrap_or_default()`, which is the one spelling that
    // turns a broken invariant into SILENTLY DROPPED DIGITS in a number a human reads --
    // this crate's headline failure mode, reached through its own formatter. Neither can
    // fire today; stating the proof is what makes a future edit that breaks it fail loudly
    // instead of quietly rendering less money than there is.
    let mut chunks: Vec<&str> = Vec::new();
    let mut end = digits.len();
    let mut step: usize = 0;

    while end > 0 {
        let size = sizes.get(step).or_else(|| sizes.last()).map_or(0, |s| usize::from(*s));
        if size == 0 {
            // A zero size would consume nothing and loop forever. Stop and emit the rest
            // ungrouped, which is what a caller writing `&[0]` can only have meant.
            break;
        }
        let start = end.saturating_sub(size);
        chunks.push(digits.get(start..end).expect("start..end is inside an all-ASCII digit string"));
        end = start;
        step = step.saturating_add(1);
    }
    if end > 0 {
        chunks.push(digits.get(..end).expect("end is inside an all-ASCII digit string"));
    }

    chunks.reverse();
    chunks.join(separator)
}

// ---------------------------------------------------------------------------------------
// Worked examples. NOT a locale database -- see the module docs.
// ---------------------------------------------------------------------------------------

/// `$1,234.50` — US English, US dollar.
pub const EN_USD: LocalePolicy<'static> = LocalePolicy::new(Iso4217::USD, "$");

/// `Rp 16.000` — Indonesian, rupiah. **Displays at 0dp while ISO settles at 2**, which is
/// C2's motivating case and the reason display width and settlement width are two numbers.
pub const ID_IDR: LocalePolicy<'static> = LocalePolicy {
    currency: Iso4217::IDR,
    symbol: "Rp ",
    symbol_position: SymbolPosition::Prefix,
    group_separator: ".",
    decimal_separator: ",",
    grouping: &[3],
    min_fraction_digits: FractionDigits::ZERO,
};

/// `1.234,50 €` — German, euro. Separators swapped, symbol trailing.
pub const DE_EUR: LocalePolicy<'static> = LocalePolicy {
    currency: Iso4217::EUR,
    symbol: " €",
    symbol_position: SymbolPosition::Suffix,
    group_separator: ".",
    decimal_separator: ",",
    grouping: &[3],
    min_fraction_digits: FractionDigits(2),
};

/// `￥1,234` — Japanese, yen. Needs no special case: JPY's ISO exponent is already 0.
pub const JA_JPY: LocalePolicy<'static> = LocalePolicy::new(Iso4217::JPY, "￥");

#[cfg(test)]
mod tests {
    use super::*;

    /// Grouping is exercised through the public API in `tests/locale.rs`; these pin the
    /// degenerate inputs that have no reachable public spelling.
    #[test]
    fn grouping_handles_its_degenerate_inputs() {
        assert_eq!(group("1234", &[], ","), "1234", "no sizes");
        assert_eq!(group("1234", &[3], ""), "1234", "no separator");
        assert_eq!(group("", &[3], ","), "", "no digits");
        assert_eq!(group("12", &[3], ","), "12", "shorter than one group");
        assert_eq!(group("123", &[3], ","), "123", "exactly one group, no leader");
        // A zero size must terminate rather than spin, and must not lose digits.
        assert_eq!(group("1234", &[0], ","), "1234");
        assert_eq!(group("1234567", &[3, 0], ","), "1234,567");
    }
}
