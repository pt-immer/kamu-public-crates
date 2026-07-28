//! The domain: what a Money is allowed to be.

use core::fmt;

use crate::iso::Iso4217;

/// Fractional digits. Fixed, structural, not a field — so it cannot drift. (DESIGN.md C1)
///
/// **18, and the same 18 for every fixed-point type in this crate — money and rates alike.**
/// Not because money needs 18 fractional digits: the deepest real minor unit is 3dp (BHD, KWD)
/// and 12 already exceeded that by nine orders. It is 18 because a *rate*'s useful precision is
/// relative rather than absolute, so a rate needs the depth — and because a schema holding both
/// `numeric(36,12)` and `numeric(36,18)` asks a human to remember which column is which, where
/// getting it wrong is a **silent factor of 10^6** and no type system reaches the migration, the
/// ad-hoc query, or the BI tool. One scale makes that mistake unrepresentable rather than
/// documented — the same reasoning that deleted `StaticCurrency::EXP`.
pub const SCALE: u32 = 18;

/// Total digits. The `36` in `NUMERIC(36,18)`.
///
/// `PRECISION` and [`SCALE`] together ARE the schema contract; both must be named, or half the
/// DDL lives as a magic number inside `DOMAIN_MAX`'s definition.
pub const PRECISION: u32 = 36;

/// `10^SCALE`. One whole currency unit, expressed in canonical units.
pub const POW10_SCALE: i128 = 10i128.pow(SCALE);

/// Largest representable magnitude, in canonical units.
///
/// `NUMERIC(36,18)` admits `|v| < 10^18` with 18 fractional digits, i.e. `< 10^36` units.
/// `i128::MAX` is ~1.7e38, leaving a ~170x margin. That margin is not waste: it is what
/// lets every operation compute first and range-check after, with no wrapping. (DESIGN.md C1)
///
/// This constant counts **units**, so it does not move with [`SCALE`]: the same `10^36 - 1`
/// and the same ~170x margin hold at 12 or at 18. Only where the decimal point sits changes,
/// which is why widening the scale cost no checking headroom. What it did cost is integer
/// range — `|v| < 10^18` rather than `< 10^24`. At IDR magnitudes one stored value at that cap
/// is ~$62.5 trillion, so no ledger row reaches it.
///
/// **CORRECTION (2026-07-22).** This comment previously read: *"That bounds a single stored
/// value, not an aggregate: `SUM()` widens past the column type in PG."* That is true of
/// PostgreSQL's `numeric`, and **false here**. `kmoney`'s aggregate declares
/// `STYPE = kmoney`, so the accumulator is the bounded type itself, and `Money`'s `Sum` is a
/// left fold through the bounded `Add`. Neither widens.
///
/// That premise produced a real defect: a total that is in-domain could fail because a
/// *transient partial sum* was not. `[MAX, MAX, -MAX]` panicked while `[MAX, -MAX, MAX]`
/// returned `MAX` — same multiset, same mathematical answer — and `PARALLEL = SAFE` made it
/// plan-dependent, so a valid ledger total could start failing after a planner or worker-count
/// change with no data change at all.
///
/// **FIXED (R2-F4).** The summing abstraction was removed rather than widened in place: there is
/// no `Sum` trait and no `sum(kmoney)` aggregate. `Money::try_sum` and the SQL
/// `kmoney_sum(VARIADIC)` accumulate in `I256` and range-check ONCE at the end (shared kernel
/// `arith::sum_units`), so the result is order- and plan-independent. The mistaken premise —
/// reasoning from `numeric`'s widening `SUM()` to a custom aggregate's — is kept above rather
/// than deleted, as the reasoning that produced the defect.
///
/// **REFINED (R2-F4b, 2026-07-25).** R2-F4's SQL remedy was over-broad, and the sentence above is
/// no longer true of PostgreSQL: `sum(kmoney)` is back. Without it the only way to total a column
/// was `kmoney_sum(VARIADIC array_agg(col))`, which materialises every row into one array before
/// any arithmetic — memory linear in the rows, on a type whose stated purpose is ledger columns.
/// The restored aggregate has a `bytea` transition state carrying 32 bytes of `I256` plus the
/// 2-byte ISO code, enforces the domain per term on the way in and once on the way out, and is
/// genuinely `PARALLEL = SAFE`. THE NARROW `STYPE = kmoney` STATE WAS THE DEFECT, NOT THE
/// AGGREGATE — which is exactly what the widening premise above was reaching for and got wrong
/// only in where it applied.
///
/// Rust's half stands unchanged: `Sum` stays unimplemented and `Money::try_sum` remains, because a
/// fold through `+` cannot be given a state wider than its element type and a SQL aggregate can.
/// See R2-F4b.
pub const DOMAIN_MAX: i128 = 10i128.pow(PRECISION) - 1;

// The checking margin is a compile-time property of these constants, so enforce it at
// compile time: editing DOMAIN_MAX to break the invariant fails the BUILD, not a test run
// that might never be executed. This is what lets every operation compute-then-check
// instead of pre-checking, so it must never silently regress.
const _: () = assert!(DOMAIN_MAX < i128::MAX);
const _: () = assert!(DOMAIN_MAX.checked_add(DOMAIN_MAX).is_some());
const _: () = assert!(i128::MAX / DOMAIN_MAX >= 100);

/// Every way a Money operation can refuse.
///
/// `#[non_exhaustive]`, for exactly the reason [`Iso4217`] carries it. An error enum in a money
/// library grows: this crate is heading for publication, and every later phase (wire, PostgreSQL,
/// the drivers) has failures it has not named yet. Without this, adding one is a **breaking
/// change** for every downstream `match`. Downstream crates must carry a wildcard arm, which
/// costs nothing now and costs a major version later.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum MoneyError {
    /// `|units| > DOMAIN_MAX`. Means ~1e18 currency units. In practice: a bug, not a condition.
    DomainOverflow {
        /// The value that was attempted, in canonical units. Compare against [`DOMAIN_MAX`].
        attempted_units: i128,
    },
    /// A text form whose currency does not match the target type.
    ///
    /// The currency in `"USD 10.50"` is redundant with `Money<USD>` ON PURPOSE: it is a
    /// cross-check that catches an IDR value landing in a USD field at an API boundary,
    /// which is precisely where the type system cannot help.
    WrongCurrency {
        /// The currency the target type requires.
        expected: Iso4217,
        /// The currency the text actually named.
        found: Iso4217,
    },
    /// Text that is not a money literal at all: bad syntax, or an unknown alpha-3 code.
    MalformedText,
    /// More fractional digits than the canonical scale can represent.
    ///
    /// **Refused, never rounded**, and it has its own variant so the refusal is greppable.
    /// This is the exact failure that disqualified `rust_decimal` for this crate: its
    /// `from_str` silently rounded out-of-domain input and returned `Ok` (E2). A money
    /// parser that rounds is a money parser that loses money quietly.
    ExcessPrecision {
        /// How many fractional digits the input carried. The limit is [`SCALE`].
        digits: u32,
    },
    // NOTE. `CurrencyMismatch` (arithmetic on two `Money<Dyn>` of different currencies) was
    // deleted with the runtime-currency variant and has NOT returned: currency identity in
    // arithmetic is settled entirely by the type system now, so no call site can reach it.
    //
    // `WrongCurrency` was deleted at the same time and came back one commit later, for the
    // text parser above. That is the `#[non_exhaustive]` bet paying out exactly as argued:
    // dropping a variant with no caller cost nothing, and re-adding it when a real caller
    // appeared was additive rather than breaking. (DESIGN.md §0.3, C3)
    /// An FX conversion left the domain.
    ///
    /// Names the **pair**, not the attempted value, and that is deliberate: the attempted
    /// quotient can reach `1e54`, which does not fit `i128`. Reporting a saturated or wrapped
    /// number would be a lie about what was computed, so this variant reports the only two
    /// facts that survive intact. Compare [`MoneyError::DomainOverflow`], where the attempted
    /// value does fit and is therefore carried.
    ConversionOverflow {
        /// The currency converted from.
        from: Iso4217,
        /// The currency converted to.
        to: Iso4217,
    },
    /// An FX rate was constructed from units that are zero or negative.
    ///
    /// A rate is a **price** — how many quote one base buys — and prices are strictly
    /// positive. That makes positivity the value's own invariant rather than a quote feed's
    /// policy, which is what it used to be: [`crate::Rate`] reused this domain *sign
    /// included* and said so in prose, so a zero quote silently destroyed the converted
    /// amount and a negative one silently reversed its sign, with no overflow and no residue
    /// to notice. Naming a hazard in a doc comment is not the same as making it
    /// unrepresentable, and every other type in this crate takes the second road.
    ///
    /// Separate from [`DomainOverflow`](Self::DomainOverflow) because the two are different
    /// mistakes: an out-of-domain rate is a magnitude bug, while a non-positive one is
    /// usually a feed that has handed over a spread, a delta, or a not-quoted sentinel.
    /// (DESIGN.md C6)
    NonPositiveRate {
        /// The value that was attempted, in canonical units.
        attempted_units: i128,
    },
    /// An allocation was asked to distribute across no weights, or across weights that are
    /// all zero. There is no meaningful distribution, and returning `[]` would destroy the
    /// whole amount.
    ///
    /// This used to be an `assert!`, which meant [`crate::allocate::allocate_units`] had two
    /// failure protocols at once: `Err` for an out-of-domain amount, and a panic for bad
    /// weights — in a function that already handed the caller a `Result` to handle. Weights
    /// routinely arrive from a request body or a config file, so "pre-validate or the process
    /// dies" was the wrong contract to offer. Adding the variant was free, which is the
    /// `#[non_exhaustive]` bet paying out again.
    UnallocatableWeights {
        /// How many weights were supplied. Zero means the slice was empty; otherwise every
        /// weight was zero.
        weights: usize,
    },
}

impl fmt::Display for MoneyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DomainOverflow { attempted_units } => write!(
                f,
                "money domain overflow: {attempted_units} units is outside the domain \
                 |units| <= {DOMAIN_MAX} (NUMERIC(36,18) admits |v| < 10^18)"
            ),
            Self::WrongCurrency { expected, found } => {
                write!(f, "wrong currency: expected {}, found {}", expected.alpha3(), found.alpha3())
            }
            Self::MalformedText => write!(f, "not a money literal: expected \"<ISO> <amount>\""),
            Self::ExcessPrecision { digits } => write!(
                f,
                "{digits} fractional digits exceeds the canonical scale of {SCALE}; refused \
                 rather than rounded, because rounding here would lose money silently"
            ),
            Self::ConversionOverflow { from, to } => write!(
                f,
                "{} -> {} conversion left the domain |units| <= {DOMAIN_MAX}; the result is \
                 not reported because it does not fit an i128",
                from.alpha3(),
                to.alpha3()
            ),
            Self::NonPositiveRate { attempted_units } => write!(
                f,
                "an FX rate must be strictly positive, and {attempted_units} units is not: a \
                 zero rate sends the converted amount to zero and a negative one reverses its \
                 sign, both silently. A signed scaling factor is a different thing from a \
                 price and wants a differently named type"
            ),
            Self::UnallocatableWeights { weights: 0 } => {
                f.write_str("cannot allocate across an empty set of weights")
            }
            Self::UnallocatableWeights { weights } => write!(
                f,
                "cannot allocate across {weights} weights that are all zero — the amount \
                 would have nowhere to go"
            ),
        }
    }
}

impl core::error::Error for MoneyError {}

/// `true` iff `units` is inside the domain.
#[must_use]
#[inline]
pub const fn in_domain(units: i128) -> bool {
    units <= DOMAIN_MAX && units >= -DOMAIN_MAX
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn domain_max_is_the_numeric_36_18_bound() {
        // 18 fractional digits within 36 total => integer part < 10^18, whole domain 10^36
        // units. DOMAIN_MAX counts units, so it is the same 10^36 - 1 it was at scale 12.
        assert_eq!(DOMAIN_MAX, 10i128.pow(PRECISION) - 1);
    }

    /// Pins the schema literals. The previous version of this test asserted
    /// `POW10_SCALE == 10i128.pow(SCALE)` — the same expression on both sides, so it could
    /// never fail, and stayed green for seven tasks while testing nothing.
    #[test]
    fn schema_literals_are_pinned() {
        assert_eq!(PRECISION, 36, "the 36 in NUMERIC(36,18)");
        assert_eq!(SCALE, 18, "the 18 in NUMERIC(36,18)");
        assert_eq!(POW10_SCALE, 1_000_000_000_000_000_000);
    }

    /// Verifies the DERIVATION rather than restating a definition: the integer part spans
    /// `10^(PRECISION - SCALE)`, so in units of `10^-SCALE` the whole domain is
    /// `10^(PRECISION-SCALE) * 10^SCALE = 10^PRECISION`. This ties `DOMAIN_MAX`, `PRECISION`,
    /// `SCALE` and `POW10_SCALE` together — break any one and this fails.
    ///
    /// PROVENANCE, stated precisely because it changed twice.
    ///
    /// PostgreSQL 18.4 was first measured saying, verbatim, *"A field with precision 36, scale
    /// 12 must round to an absolute value less than 10^24"* (DESIGN.md E9), against
    /// `numeric(36,12)`. The `10^18` here was that rule applied to scale 18 — a derivation,
    /// not a quote — and this comment used to say so, adding that confirming it was an open
    /// item for phase 4.
    ///
    /// **It is no longer open.** E13 measured `numeric(36,18)` against a live PostgreSQL 18.4,
    /// which states it verbatim: *"A field with precision 36, scale 18 must round to an
    /// absolute value less than 10^18."* The derivation was right. E13 also recorded something
    /// the derivation could not have found — PostgreSQL silently ROUNDS over-precise input on
    /// the way in, where no `CHECK` or `DOMAIN` can reach it — which is why this crate refuses
    /// excess precision rather than accepting it.
    #[test]
    fn domain_max_derives_from_the_pg_stated_bound() {
        let pg_integer_bound = 10i128.pow(PRECISION - SCALE);
        assert_eq!(
            pg_integer_bound,
            10i128.pow(18),
            "derived, and since measured against a live PostgreSQL 18.4 (DESIGN.md E13)"
        );
        assert_eq!(DOMAIN_MAX + 1, pg_integer_bound * POW10_SCALE);
    }

    #[test]
    fn domain_boundary_is_inclusive_at_the_max() {
        assert!(in_domain(0));
        assert!(in_domain(DOMAIN_MAX));
        assert!(in_domain(-DOMAIN_MAX));
        assert!(!in_domain(DOMAIN_MAX + 1));
        assert!(!in_domain(-DOMAIN_MAX - 1));
        assert!(!in_domain(i128::MAX));
        assert!(!in_domain(i128::MIN));
    }

    #[test]
    fn display_reports_the_attempted_value_not_just_the_bound() {
        let e = MoneyError::DomainOverflow { attempted_units: DOMAIN_MAX + 1 };
        let s = e.to_string();
        assert!(s.contains(&(DOMAIN_MAX + 1).to_string()), "must name the offending value: {s}");
        assert!(s.contains(&DOMAIN_MAX.to_string()), "must name the bound too: {s}");
    }
}
