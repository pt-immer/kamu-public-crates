//! The policy: what a locale decides about showing one currency.

use crate::errors::LocaleError;
use crate::iso::Iso4217;

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

const _: () = assert!(crate::domain::SCALE == 18);

/// How one locale shows one currency.
///
/// A policy pairs locale decoration with one [`Iso4217`] currency.
///
/// # Why rendering can fail
///
/// [`render`](LocalePolicy::render) rejects a mismatched `Money<C>` instead of attaching the
/// wrong symbol.
///
/// # Why the lifetime, and why not `'static`
///
/// Borrowed fields allow policies built from runtime locale data. Shipped constants use
/// `LocalePolicy<'static>`.
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
    /// `&[3]` groups by three. `&[3, 2]` renders the Indian lakh/crore shape
    /// `1,23,45,678`. `&[]` disables grouping.
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
}

// Representative policies, not a locale database.
//
// `EN_USD` and `JA_JPY` are built through `new`. The other two are struct literals because
// `try_with_separators` cannot be `const` — `str` equality is not const-stable — so a `const`
// item has no way to reach it.

/// `$1,234.50` — US English, US dollar.
pub const EN_USD: LocalePolicy<'static> = LocalePolicy::new(Iso4217::USD, "$");

/// `Rp 16.000` — Indonesian rupiah. Displays at 0dp while ISO settles at 2.
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
    use crate::Money;
    use crate::domain::POW10_SCALE;
    use crate::iso::{IDR, USD};

    /// 16 000.50 IDR.
    fn idr_16000_50() -> Money<IDR> {
        Money::<IDR>::try_from_units(16_000 * POW10_SCALE + POW10_SCALE / 2).expect("in domain")
    }

    /// Heap-owned inputs prove that policies accept non-`'static` locale data.
    #[test]
    fn a_policy_can_be_built_from_data_loaded_at_run_time() {
        // Stand-ins for a CLDR/ICU table read at startup. `String`, not `&'static str`.
        let symbol = String::from("Rp ");
        let group = String::from(".");
        let decimal = String::from(",");
        let grouping: Vec<u8> = vec![3];

        let policy = LocalePolicy::new(Iso4217::IDR, &symbol)
            .try_with_separators(&group, &decimal)
            .unwrap()
            .try_with_grouping(&grouping)
            .unwrap()
            .with_min_fraction_digits(FractionDigits::ZERO);

        assert_eq!(policy.render(idr_16000_50()).unwrap(), "Rp 16.000,5");

        // And it is the same policy the crate ships as a constant, reached the other way round:
        // borrowed data and `'static` data are one type at two lifetimes, not two behaviours.
        assert_eq!(policy.render(idr_16000_50()).unwrap(), ID_IDR.render(idr_16000_50()).unwrap());
    }
    #[test]
    fn a_minimum_past_the_scale_is_rejected() {
        assert_eq!(FractionDigits::try_new(19), Err(LocaleError::FractionDigitsOutOfRange { digits: 19 }));
        let maximum = FractionDigits::try_new(18).unwrap();
        let policy = LocalePolicy::new(Iso4217::USD, "$").with_min_fraction_digits(maximum);
        assert_eq!(maximum.get(), 18);
        assert_eq!(policy.min_fraction_digits(), maximum);
    }
    #[test]
    fn degenerate_locale_policies_are_rejected() {
        assert_eq!(
            LocalePolicy::new(Iso4217::USD, "$").try_with_grouping(&[3, 0]),
            Err(LocaleError::ZeroGroupingWidth { index: 1 })
        );
        assert_eq!(
            LocalePolicy::new(Iso4217::USD, "$").try_with_separators(".", "."),
            Err(LocaleError::AmbiguousSeparators)
        );
        assert_eq!(
            LocalePolicy::new(Iso4217::USD, "$").try_with_separators(",", ""),
            Err(LocaleError::EmptyDecimalSeparator)
        );

        let ungrouped = LocalePolicy::new(Iso4217::USD, "$").try_with_separators("", ".").unwrap();
        assert_eq!(ungrouped.render(Money::<USD>::try_from_major(1_000).unwrap()).unwrap(), "$1000.00");
    }
}
