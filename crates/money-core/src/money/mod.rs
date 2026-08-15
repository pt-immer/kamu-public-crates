//! The canonical representation.

use crate::StaticCurrency;
use crate::iso::Iso4217;
use core::marker::PhantomData;

/// A monetary quantity: `units` counts `10^-18` of a currency unit.
///
/// Scale is **fixed at 18 and structural** — it is not a field, so it cannot drift.
/// Invariant: `|units| <= DOMAIN_MAX`. Raw units are read-only; reconstruction
/// requires a checked constructor.
///
/// # What this type proves
///
/// - **Currency identity, at compile time.** `Money<USD>` and `Money<IDR>` are distinct types, so
///   a cross-currency operation does not typecheck rather than failing at run time.
/// - **Domain.** `|units| <= DOMAIN_MAX` holds for every value that exists: the field is private
///   and every constructor is checked.
/// - **Scale.** Exactly 18, and structural rather than stored, so two values cannot disagree
///   about it.
/// - **Exactness.** Addition and subtraction are exact or refused — never rounded.
/// - **Width.** Sixteen bytes, because the currency marker is zero-sized.
///
/// # What this type does not prove
///
/// - **It does not decide policy.** Exact integer representation removes representation error. It
///   settles no question of tax, FX, allocation or rounding. [`Rounding`](crate::Rounding) has no
///   `Default`, so a division cannot be taken without naming one, and `allocate` distributes the
///   weights its caller supplies:
///
///   ```
///   use core::num::NonZeroU32;
///   use kamu_money_core::{Money, Rounding, iso::USD};
///
///   let thirds = NonZeroU32::new(3).unwrap();
///   let two = Money::<USD>::try_from_major(2)?;
///
///   // Both answers are correct. The type refuses to choose between them.
///   let down = two.div_int(thirds, Rounding::TowardZero).discard_deliberately();
///   let up = two.div_int(thirds, Rounding::AwayFromZero).discard_deliberately();
///   assert_ne!(down, up);
///   # Ok::<(), Box<dyn core::error::Error>>(())
///   ```
///
/// - **Representable is not correct.** The domain check proves an amount can be held, never that
///   it is the right amount.
/// - **Nothing ties `C` to reality at a boundary.** A database row decoded into `Money<USD>` is
///   USD because the caller said so. The type carries that claim faithfully; it cannot originate
///   it.
/// - **Text is a retraction, not a bijection.** `parse(render(v)) == v` for every `v`, but the
///   converse fails: `"USD 10.5"` re-renders as `"USD 10.50"`. Stated in full under
///   [the canonical text form](crate::text).
pub struct Money<C: StaticCurrency> {
    units: i128,
    // The currency marker is zero-sized, so Money<C> has the width of i128.
    _c: PhantomData<C>,
}

mod compare;
mod construct;
impl<C: StaticCurrency> Money<C> {
    /// The currency of this value. Always `C::CODE` — it cannot be anything else.
    #[inline]
    #[must_use]
    pub const fn code(&self) -> Iso4217 {
        C::CODE
    }

    /// The canonical units. Read-only: reconstructing requires a checked constructor.
    #[inline]
    #[must_use]
    pub const fn units(&self) -> i128 {
        self.units
    }

    /// `true` iff this is exactly zero. Sign-agnostic; there is no negative zero.
    #[inline]
    #[must_use]
    pub const fn is_zero(&self) -> bool {
        self.units == 0
    }
}

#[cfg(test)]
mod tests {
    use crate::Money;
    use crate::iso::{IDR, Iso4217, USD};

    /// The compile-time currency is zero-sized.
    #[test]
    fn the_compile_time_currency_costs_nothing() {
        assert_eq!(size_of::<Money<USD>>(), 16);
        assert_eq!(size_of::<Money<USD>>(), size_of::<i128>());
    }
    #[test]
    fn code_comes_from_the_type() {
        assert_eq!(Money::<USD>::try_from_units(1).unwrap().code(), Iso4217::USD);
        assert_eq!(Money::<IDR>::try_from_units(1).unwrap().code(), Iso4217::IDR);
    }
    /// `is_zero` inspects magnitude; the generic type retains currency identity.
    #[test]
    fn is_zero_asks_only_about_magnitude() {
        assert!(Money::<USD>::ZERO.is_zero());
        assert!(Money::<IDR>::ZERO.is_zero());
        assert!(!Money::<USD>::try_from_units(1).unwrap().is_zero());
    }
}
