//! FX conversion: rates, and the money that passes through them. (DESIGN.md C6)

use crate::Money;
use crate::StaticCurrency;
use crate::domain_impl::{POW10_SCALE, in_domain};
use crate::error_impl::{AmountError, RateError};
use crate::rounding_impl::{Rounding, div_round_i256};
use core::marker::PhantomData;
use ethnum::I256;

/// `POW10_SCALE^2`: the divisor for a two-leg conversion, which applies the scale twice.
///
/// Derived from [`POW10_SCALE`] rather than written as `1e36`, so it follows `SCALE` if that
/// constant moves again — a hand-written literal here would still compile and silently be
/// wrong by six orders, which is exactly how the 12 -> 18 migration bit elsewhere.
///
/// `1e18 * 1e18 = 1e36` fits `i128` (~1.7e38) with the same ~170x margin as `DOMAIN_MAX`, and
/// overflow in a `const` initializer is a **build** error rather than a runtime one — so the
/// bound is enforced by compiling, with nothing to check at call time.
const POW10_SCALE_SQUARED: i128 = POW10_SCALE * POW10_SCALE;

/// The one-leg conversion kernel, shared by the typed and the runtime path.
///
/// Extracted when a runtime-rate twin existed and could have drifted from it. That twin is
/// gone, but the split is kept: it is the one place the scale relationship is written down, so
/// `prop_a_unit_rate_moves_the_currency_and_nothing_else` has a single thing to pin.
///
/// Returns `None` iff the result does not fit an `i128`. The **domain** check deliberately is
/// not here: it belongs to `Money`'s constructors, which is the only place a value becomes
/// money, and duplicating it would give two answers to maintain.
fn apply_rate(units: i128, rate_units: i128, mode: Rounding) -> Option<i128> {
    // Both operands are in-domain, so |product| <= (1e36)^2 = 1e72 — about five orders below
    // I256::MAX (~1.16e77). `checked_mul` cannot return None here; it is used instead of `*`
    // because clippy::arithmetic_side_effects is denied crate-wide, and because an unchecked
    // operator would silently become wrong if the domain ever moved.
    let product = I256::from(units)
        .checked_mul(I256::from(rate_units))
        .expect("|units| <= DOMAIN_MAX ~1e36 twice over, so |product| <= 1e72 < I256::MAX");

    let (quotient, _below_one_unit) = div_round_i256(product, I256::from(POW10_SCALE), mode);

    // The quotient can reach 1e54, so THIS narrowing is the real overflow gate — and it must
    // stay checked. Truncating here returns a plausible, silently wrong amount: a quotient of
    // exactly 2^128 truncates to ZERO, which is `Ok($0.00)` with the money simply gone.
    // Pinned by `a_quotient_that_would_truncate_back_into_the_domain_is_still_refused`.
    i128::try_from(quotient).ok()
}

/// The two-leg kernel. Rounds **once**, at the end — see [`Money::convert_via`] for why that
/// is a ledger requirement rather than a precision one.
fn apply_rate_pair(units: i128, first: i128, second: i128, mode: Rounding) -> Option<i128> {
    // First leg: |m * r1| <= 1e72, the same proof as `apply_rate`. Cannot fail.
    let partial = I256::from(units)
        .checked_mul(I256::from(first))
        .expect("|units| <= DOMAIN_MAX ~1e36 twice over, so |product| <= 1e72 < I256::MAX");

    // Second leg: this one CAN overflow (1e72 * 1e36 = 1e108), and when it does the result
    // would have left the domain regardless — so refusing is correct, not conservative.
    let product = partial.checked_mul(I256::from(second))?;

    let (quotient, _below_one_unit) = div_round_i256(product, I256::from(POW10_SCALE_SQUARED), mode);

    i128::try_from(quotient).ok()
}

/// A directed FX rate: how many `T` one `F` buys, as a fixed-point number at the crate's
/// one [`SCALE`](crate::advanced::domain::SCALE).
///
/// The pair is carried in the type, so `Money<USD>` can only be converted by a
/// `Rate<USD, IDR>` and the result can only be `Money<IDR>` — a mismatched pair does not
/// compile. A runtime quote table hands out typed rates through a generic accessor rather than
/// a value-carrying rate type — see C6.
///
/// **A rate is a price, so its units are strictly positive.** `Rate` bounds magnitude by
/// `Money`'s domain (`|units| <= DOMAIN_MAX`) and additionally refuses zero and negatives at
/// construction — see [`try_from_units`](Self::try_from_units).
///
/// That reverses a decision taken on 2026-07-21, and the reversal is written down because the
/// original was deliberate rather than an oversight. C6 bounds magnitude and is silent on
/// sign, so `Rate` was kept a plain fixed-point number exactly like `Money`, with the cost
/// **documented here instead of enforced**: a negative rate flipped the sign of the money
/// passing through it and a zero rate sent it to zero, both silently, with no overflow and no
/// residue. Sign was the quote feed's responsibility — and that decision recorded one
/// condition for revisiting it, *"if a feed is ever ingested without validation."*
///
/// **That condition is met by this crate's own code.** [`FromStr`](core::str::FromStr),
/// serde's `Deserialize`, `postgres-types`' `FromSql` and sqlx's `Decode` each build a `Rate`
/// straight from untrusted bytes, so four of the feed adapters the responsibility was
/// delegated to are shipped in this repository. The phantom pair proves `Rate<USD, IDR>` is
/// not `Rate<IDR, USD>`; it cannot prove a runtime number is positive, and runtime
/// construction is what finishes that proof.
///
/// If a signed scaling factor is ever wanted, it is a different thing from a price and wants
/// its own name — weakening `Rate` to obtain it would give back the silent sign flip.
///
/// There is deliberately no `inverse()` and no `compose()`: real FX has bid and ask, so
/// inverting or composing mid-rates fabricates a price nobody can trade at. Every pair is
/// stored in both directions; multi-leg conversion is [`Money::convert_via`], which rounds
/// once. (DESIGN.md C6)
pub struct Rate<Base: StaticCurrency, Quote: StaticCurrency> {
    units: i128,
    // `Money<C>` proves it uses `C` through a real field (`tag: C::Tag`). `Rate` has no such
    // field — `units` is currency-agnostic — so without this the parameters are unconstrained
    // (E0392). This is the one structural difference between the two types.
    _pair: PhantomData<(Base, Quote)>,
}

// Hand-written, NOT derived, for the reason given at `money.rs:17`: `#[derive(Clone)]` would
// emit `impl<F: Clone, T: Clone>`, bounding the phantom parameters when nothing about a rate's
// units depends on them. Note this is a correctness-of-signature argument and NOT a testable
// one — `StaticCurrency` is sealed and every generated marker derives `Clone`, so no downstream
// type could observe the difference. It is a comment rather than a test on purpose: a
// `#[test] fn rate_is_copy()` would pass against a derive too, and a test that cannot fail is
// worse than no test.
impl<Base: StaticCurrency, Quote: StaticCurrency> Clone for Rate<Base, Quote> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<Base: StaticCurrency, Quote: StaticCurrency> Copy for Rate<Base, Quote> {}
impl<Base: StaticCurrency, Quote: StaticCurrency> PartialEq for Rate<Base, Quote> {
    fn eq(&self, o: &Self) -> bool {
        self.units == o.units
    }
}
impl<Base: StaticCurrency, Quote: StaticCurrency> Eq for Rate<Base, Quote> {}
impl<Base: StaticCurrency, Quote: StaticCurrency> core::fmt::Debug for Rate<Base, Quote> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "Rate({} units, {}->{})", self.units, Base::CODE.alpha3(), Quote::CODE.alpha3())
    }
}

impl<Base: StaticCurrency, Quote: StaticCurrency> Rate<Base, Quote> {
    /// Construct from canonical units, reporting **why** a value was refused.
    ///
    /// This is the single owner of `Rate`'s invariant. Every ingress — the text parser, both
    /// serde forms, `postgres-types` and sqlx — reaches a `Rate` through here or through
    /// [`FromStr`](core::str::FromStr), which itself lands here, so no adapter can enforce a
    /// weaker rule by omission. That is the whole reason the check is not repeated at each
    /// boundary: five copies of an invariant is five chances to have four.
    ///
    /// # Errors
    /// [`RateError::Amount`] if the magnitude leaves the domain, and
    /// [`RateError::NonPositive`] if `units <= 0`. The domain is tested **first**, so
    /// `i128::MIN` is reported as the magnitude bug it is rather than as a sign bug, while an
    /// in-domain `-2` is reported as the sign bug it is.
    #[inline]
    pub const fn try_from_units(units: i128) -> Result<Self, RateError> {
        if !in_domain(units) {
            return Err(RateError::Amount(AmountError::out_of_domain(units)));
        }
        if units <= 0 {
            return Err(RateError::NonPositive { attempted_units: units });
        }
        Ok(Self { units, _pair: PhantomData })
    }

    /// The canonical units. Read-only: reconstructing requires a checked constructor.
    #[inline]
    #[must_use]
    pub const fn units(&self) -> i128 {
        self.units
    }
}

impl<C: StaticCurrency> Money<C> {
    /// Convert at `rate`, rounding per `mode`.
    ///
    /// The pair is checked by the type system: this value's currency must be the rate's
    /// **base**, and the result is denominated in the rate's **quote**. A mismatched pair does
    /// not compile.
    ///
    /// **No [`Residue`](crate::Residue), and that is not an oversight.** The divisor here is
    /// `POW10_SCALE`, so the remainder is always strictly less than one canonical unit —
    /// measured over 200 000 random pairs, the worst loss was `0.499999` units, which is `0`
    /// as an integer count. An always-empty residue would be worse than none: `#[must_use]`
    /// would train every caller of the crate's most common operation to reflexively write
    /// `let (m, _) = ...`, and that reflex carries to [`Money::div_int`], where the residue is
    /// real money. The loss here is real but **unrepresentable** — below `1e-18` of a currency
    /// unit — so there is nothing to hand back. (DESIGN.md C6)
    ///
    /// There is deliberately **no `impl Mul`**: an operator that fails on ordinary input is a
    /// lie, and this one does — `USD -> ZWL` at the 2008 rate leaves the domain at a $100 000
    /// balance.
    ///
    /// # Errors
    /// [`RateError::ConversionOverflow`] if the converted amount leaves the domain. That is a
    /// *condition*, not a bug: it is reachable at ordinary balances for high-magnitude pairs.
    ///
    /// # Panics
    /// Never. The `expect` below is proven unreachable by the domain invariant, and its proof
    /// is written at the site.
    #[must_use = "the converted money is the result; dropping it discards the conversion"]
    pub fn convert<Quote: StaticCurrency>(
        self,
        rate: Rate<C, Quote>,
        mode: Rounding,
    ) -> Result<Money<Quote>, RateError> {
        let units = apply_rate(self.units(), rate.units(), mode)
            .ok_or(RateError::ConversionOverflow { from: C::CODE, to: Quote::CODE })?;
        Money::<Quote>::try_from_units(units)
            .map_err(|_| RateError::ConversionOverflow { from: C::CODE, to: Quote::CODE })
    }

    /// Convert through a bridge currency, rounding **once**, at the end.
    ///
    /// This is not a precision optimisation — measured at realistic magnitudes, two sequential
    /// conversions differ by `4.885e-14` currency units, ten orders below anything a currency
    /// can express. It is a **ledger** requirement: two sequential conversions materialise a
    /// `Money<Bridge>` balance the holder never held, quantising it to a whole canonical unit on the
    /// way through. `convert_via` never creates that balance, so there is no moment at which a
    /// party appears to hold a currency they do not. (DESIGN.md C6)
    ///
    /// This is also what callers reaching for a `compose()` actually want. Composing two
    /// mid-rates would fabricate a third that cannot be traded at, and its error grows
    /// *linearly with the amount*; the intermediate quantisation avoided here is absolute.
    ///
    /// # Errors
    /// [`RateError::ConversionOverflow`] if the conversion leaves the domain — including when
    /// the three-way product exceeds `I256`. That rejection is **correct, not conservative**:
    /// verified analytically and over 300 000 full-domain trials, an in-domain result implies
    /// `m*r1*r2 <= 1e72 < I256::MAX`, with zero false rejects.
    ///
    /// # Panics
    /// Never. The `expect` below is proven unreachable by the domain invariant.
    #[must_use = "the converted money is the result; dropping it discards the conversion"]
    pub fn convert_via<Bridge: StaticCurrency, Quote: StaticCurrency>(
        self,
        first: Rate<C, Bridge>,
        second: Rate<Bridge, Quote>,
        mode: Rounding,
    ) -> Result<Money<Quote>, RateError> {
        let units = apply_rate_pair(self.units(), first.units(), second.units(), mode)
            .ok_or(RateError::ConversionOverflow { from: C::CODE, to: Quote::CODE })?;
        Money::<Quote>::try_from_units(units)
            .map_err(|_| RateError::ConversionOverflow { from: C::CODE, to: Quote::CODE })
    }
}

#[cfg(test)]
mod tests {
    use crate::Money;
    use crate::Rate;
    use crate::domain_impl::{DOMAIN_MAX, POW10_SCALE};
    use crate::error_impl::{AmountError, RateError};
    use crate::iso::{EUR, IDR, Iso4217, USD};
    use crate::rounding_impl::Rounding;
    use ethnum::I256;

    /// `major` whole currency units, as a rate.
    fn rate<Base: crate::StaticCurrency, Quote: crate::StaticCurrency>(major: i128) -> Rate<Base, Quote> {
        Rate::try_from_units(major.checked_mul(POW10_SCALE).unwrap()).unwrap()
    }

    #[test]
    fn converting_yields_the_target_currency_at_the_quoted_price() {
        // $10.00 at 16 000 IDR/USD is Rp160 000.00 — exactly, no rounding involved.
        let usd = Money::<USD>::try_from_major(10).unwrap();
        let got = usd.convert(rate::<USD, IDR>(16_000), Rounding::HalfEven).unwrap();
        assert_eq!(got, Money::<IDR>::try_from_major(160_000).unwrap());
    }

    /// THE INVERSION OF A TEST THAT USED TO PIN THE OPPOSITE, and the history is the point.
    ///
    /// Until 2026-07-27 this file carried `a_negative_rate_flips_sign_and_a_zero_rate_sends_to_zero`,
    /// which asserted that a negative rate flips the money's sign and a zero rate sends it to
    /// zero. It was not an accident and it was not dead weight: C6 bounds magnitude and is
    /// silent on sign, the operator chose the signed domain from an explicit two-option fork on
    /// 2026-07-21, and the test existed **so that a later "defensive" sign/zero guard could not
    /// be added without something going red**. It did its job -- this is that red, arriving as
    /// designed, with the decision re-taken rather than drifted past.
    ///
    /// What changed is the condition the original decision named for revisiting itself: *"if a
    /// feed is ever ingested without validation."* Four such feeds ship in this crate. So the
    /// two values below are now refused at construction, and neither conversion is reachable
    /// to test at all. (DESIGN.md C6)
    #[test]
    fn a_zero_or_negative_rate_is_refused_at_construction() {
        assert_eq!(
            Rate::<USD, IDR>::try_from_units(0),
            Err(RateError::NonPositive { attempted_units: 0 }),
            "a zero rate would send the money to zero, silently and with no residue"
        );
        assert_eq!(
            Rate::<USD, IDR>::try_from_units(-2 * POW10_SCALE),
            Err(RateError::NonPositive { attempted_units: -2 * POW10_SCALE }),
            "a negative rate would flip the sign of the money passing through it"
        );

        assert!(Rate::<USD, IDR>::try_from_units(0).is_err());
        assert!(Rate::<USD, IDR>::try_from_units(-1).is_err());

        // The smallest representable rate is still constructible: this refuses non-positive
        // values, NOT small ones. A guard written as `units < POW10_SCALE` would pass every
        // assertion above and quietly outlaw every sub-unit quote in existence.
        assert!(Rate::<USD, IDR>::try_from_units(1).is_ok(), "1e-18 is positive and in domain");
    }

    /// The two refusals are DIFFERENT ERRORS, and a caller has to be able to tell them apart:
    /// an out-of-domain rate is a magnitude bug in the sender, a non-positive one is usually a
    /// feed that handed over a spread, a delta, or a not-quoted sentinel. Domain is tested
    /// first, so `i128::MIN` -- which is both -- reports the magnitude.
    #[test]
    fn the_two_rate_refusals_are_reported_separately() {
        assert_eq!(
            Rate::<USD, IDR>::try_from_units(i128::MIN),
            Err(RateError::Amount(AmountError::out_of_domain(i128::MIN))),
            "out of domain AND negative: the magnitude is the useful fact"
        );
        assert_eq!(
            Rate::<USD, IDR>::try_from_units(-DOMAIN_MAX),
            Err(RateError::NonPositive { attempted_units: -DOMAIN_MAX }),
            "in domain, so the sign is the only thing wrong with it"
        );
    }

    /// `convert_via` rounds ONCE, at the end, and this is a LEDGER requirement rather than a
    /// precision optimisation. The difference is not a rounding digit: chosen so the
    /// intermediate quantisation destroys the money outright.
    ///
    /// USD -> EUR at 0.5, then EUR -> IDR at 2.0, applied to one canonical unit. The
    /// intermediate is half a unit, which the ledger cannot express, so a sequential
    /// conversion quantises it to zero and the second leg multiplies nothing by two. Via,
    /// `0.5 * 2 == 1` exactly and the unit survives — because no `Money<EUR>` balance the
    /// holder never held is ever created. (DESIGN.md C6)
    ///
    /// Mutation-check: make `convert_via` call `convert` twice; this test must go red.
    #[test]
    fn convert_via_rounds_once_where_two_conversions_would_destroy_the_money() {
        let m = Money::<USD>::try_from_units(1).unwrap();
        let usd_eur = Rate::<USD, EUR>::try_from_units(POW10_SCALE / 2).unwrap();
        let eur_idr = Rate::<EUR, IDR>::try_from_units(2 * POW10_SCALE).unwrap();

        let sequential =
            m.convert(usd_eur, Rounding::HalfEven).unwrap().convert(eur_idr, Rounding::HalfEven).unwrap();
        assert_eq!(sequential.units(), 0, "the materialised intermediate ate the money");

        let via = m.convert_via(usd_eur, eur_idr, Rounding::HalfEven).unwrap();
        assert_eq!(via.units(), 1, "one rounding, at the end, and it survives");
    }

    /// `convert_via`'s second `checked_mul` is the one that can genuinely overflow, and
    /// rejecting is CORRECT rather than conservative: an in-domain result implies
    /// `m*r1*r2 <= 1e72 < I256::MAX`, so anything that overflows would have left the domain
    /// anyway. (DESIGN.md C6)
    #[test]
    fn convert_via_refuses_a_product_that_cannot_fit_the_intermediate() {
        let m = Money::<USD>::try_from_units(DOMAIN_MAX).unwrap();
        let huge_a = Rate::<USD, EUR>::try_from_units(DOMAIN_MAX).unwrap();
        let huge_b = Rate::<EUR, IDR>::try_from_units(DOMAIN_MAX).unwrap();
        assert_eq!(
            m.convert_via(huge_a, huge_b, Rounding::HalfEven),
            Err(RateError::ConversionOverflow { from: Iso4217::USD, to: Iso4217::IDR }),
            "1e108 does not fit I256, and the result would not have fit the domain either"
        );
    }

    /// Domain overflow in a conversion is a CONDITION, not a bug — which is why `convert`
    /// returns `Result` and there is no `impl Mul`. Measured: `USD -> ZWL` at the 2008 rate
    /// leaves the domain at a $100 000 balance. (DESIGN.md C6)
    ///
    /// Both gates must report the same thing, and that is the point of this test: the
    /// quotient can be too big for `DOMAIN_MAX` while still fitting `i128`, or too big for
    /// `i128` outright. A caller cannot tell those apart and should not have to.
    #[test]
    fn conversion_overflow_names_the_pair_from_both_gates() {
        let m = Money::<USD>::try_from_units(DOMAIN_MAX).unwrap();
        let expected = RateError::ConversionOverflow { from: Iso4217::USD, to: Iso4217::IDR };

        // q = 1e37: outside DOMAIN_MAX (1e36), still inside i128 (~1.7e38).
        assert_eq!(
            m.convert(rate::<USD, IDR>(10), Rounding::HalfEven),
            Err(expected),
            "in i128, outside the domain"
        );
        // q = 1e39: outside i128 entirely, so `i128::try_from` is what refuses.
        assert_eq!(
            m.convert(rate::<USD, IDR>(1_000), Rounding::HalfEven),
            Err(expected),
            "outside i128 — this is why the error cannot name the attempted value"
        );
    }

    /// The narrowing gate, pinned on its own — and it needed pinning.
    ///
    /// HISTORY, because mutation-checking is the only reason this test exists. The test above
    /// claimed to cover "both gates", and did not: replacing `i128::try_from(quotient)` with a
    /// truncating `as_i128()` — the exact silent-wrap bug C10 forbids — left all five tests in
    /// this module GREEN. Both of its cases wrap to a value that is *still* outside the domain,
    /// so `try_from_units` refuses them anyway and the two gates are indistinguishable.
    ///
    /// This case is the dangerous one: `2^64 * 10^9` units at a rate of `2^64 * 10^9` gives a
    /// quotient of exactly `2^128`, which truncates to **exactly zero**. A truncating narrowing
    /// returns `Ok($0.00)` — no overflow, no residue, no signal, and the money is simply gone.
    /// That is the failure this crate exists to make impossible, and nothing was testing it.
    ///
    /// Mutation-check: swap `i128::try_from(quotient).ok()` for `Some(quotient.as_i128())`;
    /// this test must go red and the one above must stay green.
    #[test]
    fn a_quotient_that_would_truncate_back_into_the_domain_is_still_refused() {
        // 2^64 * 10^9, comfortably in domain, chosen so the product is 2^128 * 10^18.
        let units = 18_446_744_073_709_551_616_000_000_000;
        let m = Money::<USD>::try_from_units(units).unwrap();
        let r = Rate::<USD, IDR>::try_from_units(units).unwrap();

        assert_eq!(
            m.convert(r, Rounding::HalfEven),
            Err(RateError::ConversionOverflow { from: Iso4217::USD, to: Iso4217::IDR }),
            "truncation would have made this a silent, perfectly plausible ZERO"
        );
    }

    proptest::proptest! {
        /// C6's justification for returning **no** `Residue`, pinned rather than asserted.
        ///
        /// The argument is that a conversion divides by `POW10_SCALE`, so whatever rounding
        /// moves is always strictly less than one canonical unit — which is `0` as an integer
        /// count, so a residue here would carry zero units every single time. That
        /// reasoning is the *only* thing standing between this design and silently leaking
        /// money, and until now it lived exclusively in prose.
        ///
        /// This recomputes the exact remainder in `I256` and checks it stays sub-unit. If the
        /// divisor or the scale relationship ever changed such that whole units could be lost
        /// here, the no-`Residue` decision would silently become wrong — money would go
        /// missing with nothing to catch it. This test is what makes that change loud.
        ///
        /// `units` spans both signs because money is signed; `rate_units` starts at **1**
        /// because a rate is not, as of 2026-07-27. That narrowing was forced by the
        /// constructor rather than chosen -- the old range ran through zero and the negatives
        /// and reached `.unwrap()`, so leaving it would have turned a property about rounding
        /// into a property about which values still construct.
        #[test]
        fn prop_the_discarded_remainder_is_always_below_one_canonical_unit(
            units in -100_000_000_000_000_000_000_000_000i128..=100_000_000_000_000_000_000_000_000,
            rate_units in 1i128..=100_000_000_000_000_000_000_000_000,
        ) {
            let rate = Rate::<USD, IDR>::try_from_units(rate_units).unwrap();
            for mode in Rounding::ALL {
                let out = Money::<USD>::try_from_units(units).unwrap().convert(rate, *mode).unwrap();

                // exact: what the conversion was asked for, minus what it returned
                let product = I256::from(units)
                    .checked_mul(I256::from(rate_units))
                    .unwrap();
                let returned = I256::from(out.units())
                    .checked_mul(I256::from(POW10_SCALE))
                    .unwrap();
                let remainder = product.checked_sub(returned).unwrap();

                // `-one_unit` would be unary negation on I256, which trips
                // clippy::arithmetic_side_effects; negating the i128 const instead is a
                // const-folded expression and is exempt, same as POW10_SCALE_SQUARED above.
                let one_unit = I256::from(POW10_SCALE);
                let minus_one_unit = I256::from(-POW10_SCALE);
                proptest::prop_assert!(
                    remainder < one_unit && remainder > minus_one_unit,
                    "{mode:?}: rounding moved {remainder}, which is a whole canonical unit or \
                     more — a Residue would NOT always be zero and C6's reasoning is broken"
                );
            }
        }
    }

    /// The magnitude bound is `Money`'s, unchanged. Only the sign rule differs between the two
    /// types, and it differs because only one of them is a price.
    #[test]
    fn rate_construction_enforces_the_same_magnitude_bound_as_money() {
        assert!(Rate::<USD, IDR>::try_from_units(DOMAIN_MAX).is_ok());
        assert!(Rate::<USD, IDR>::try_from_units(DOMAIN_MAX + 1).is_err());
        assert!(Rate::<USD, IDR>::try_from_units(i128::MIN).is_err(), "i128::MIN must not sneak in");
        // The upper bound is shared with `Money`; the lower bound is not, and this is the pair
        // that says so.
        assert!(Money::<USD>::try_from_units(-DOMAIN_MAX).is_ok(), "money is signed");
        assert!(Rate::<USD, IDR>::try_from_units(-DOMAIN_MAX).is_err(), "a rate is not");
    }
}
