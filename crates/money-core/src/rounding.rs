//! Rounding modes and the one primitive that rounds.

use ethnum::I256;

/// How to resolve a division that does not divide evenly.
///
/// There is no default. Every lossy operation takes one of these explicitly, because
/// a default rounding mode is a decision made by whoever wrote the library rather than
/// whoever owns the money.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
// A new rounding mode must not break a downstream `match` at compile time -- this is a published
// money crate, and adding a mode is additive. Downstream must carry a `_` arm; in-crate matches
// stay exhaustive.
#[non_exhaustive]
pub enum Rounding {
    /// Ties to even. Unbiased across many roundings; the IEEE-754 default.
    HalfEven,
    /// Ties away from zero. **This is what PostgreSQL's `round()` does** (DESIGN.md E9),
    /// and what many tax authorities mandate.
    HalfAwayFromZero,
    /// Ties toward zero.
    HalfTowardZero,
    /// Truncate.
    TowardZero,
    /// Always magnify.
    AwayFromZero,
    /// Toward negative infinity.
    Floor,
    /// Toward positive infinity.
    Ceil,
}

impl Rounding {
    /// Every mode, for exhaustive testing.
    pub const ALL: &'static [Self] = &[
        Self::HalfEven,
        Self::HalfAwayFromZero,
        Self::HalfTowardZero,
        Self::TowardZero,
        Self::AwayFromZero,
        Self::Floor,
        Self::Ceil,
    ];

    /// The canonical `snake_case` name, for a config file, a wire, or a SQL argument.
    ///
    /// Deliberately not `Display`: these are identifiers to be matched, not prose to be shown
    /// to anyone, and a `Display` impl invites them into user-facing text where they would
    /// need translating.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::HalfEven => "half_even",
            Self::HalfAwayFromZero => "half_away_from_zero",
            Self::HalfTowardZero => "half_toward_zero",
            Self::TowardZero => "toward_zero",
            Self::AwayFromZero => "away_from_zero",
            Self::Floor => "floor",
            Self::Ceil => "ceil",
        }
    }

    /// Parse a mode from its canonical name.
    ///
    /// `None` rather than a default, for the reason in this type's own documentation: a
    /// default rounding mode is a decision made by whoever wrote the library rather than
    /// whoever owns the money. An unrecognised name has to be an error at the boundary.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|m| m.as_str() == name)
    }

    /// Every canonical name, for an error message that can list the alternatives.
    #[must_use]
    pub fn names() -> String {
        Self::ALL.iter().map(|m| m.as_str()).collect::<Vec<_>>().join(", ")
    }
}

/// Divide `num` by `den`, rounding per `mode`.
///
/// Returns `(quotient, residue)` where **`quotient * den + residue == num`, exactly, always**.
/// The residue is precisely what rounding moved — never discarded, never inferred.
///
/// # Panics
/// If `den <= 0`.
///
/// # Preconditions (why this is `pub(crate)`)
/// `den` must be small enough that `2 * |num % den|` cannot overflow `I256` — i.e. roughly
/// `den < I256::MAX / 2`. Crate callers satisfy this by ~68 orders of magnitude (`div_int`
/// passes a `NonZeroU32`; `allocate` passes a sum of `u32` weights). Exposed publicly this
/// would be unsound: `div_round_i256(I256::MAX - 1, I256::MAX, HalfEven)` panics in debug and
/// **wraps silently in release**. Measured, not hypothesised.
//
// clippy::arithmetic_side_effects fires on every `I256` operator below (`/`, `-`, `*`, unary
// `-`, `%`) — measured on this toolchain, it is not limited to primitive integers; it flags
// operator syntax on ANY type, including third-party ones like `ethnum::I256`. The `assert!`
// above already rules out div-by-zero, and this primitive's contract is exact bounded
// division: overflow would mean `num`/`den` already violated the caller's domain (Money's
// i128, DOMAIN_MAX ~1e36) by dozens of orders of magnitude before ever reaching I256's
// ~1.16e77 range. `ethnum::I256` does provide a full `checked_*`/`wrapping_*` suite (verified
// by reading its source), so a lint-clean rewrite is possible — but this function is called
// out as pre-verified and meant to be transcribed as-is, so the operators are left exactly
// as specified and the lint is silenced here instead of restyling the arithmetic.
//
#[allow(clippy::arithmetic_side_effects)]
#[must_use]
pub(crate) fn div_round_i256(num: I256, den: I256, mode: Rounding) -> (I256, I256) {
    assert!(den > I256::ZERO, "div_round_i256: denominator must be positive, got {den}");

    let q = num / den; // truncates toward zero
    let r = num - q * den; // sign follows num
    if r == I256::ZERO {
        return (q, r);
    }

    let neg = r < I256::ZERO;
    let two = I256::from(2u8);
    let twice = (if neg { -r } else { r }) * two;

    let bump = match mode {
        Rounding::TowardZero => false,
        Rounding::AwayFromZero => true,
        Rounding::Floor => neg,
        Rounding::Ceil => !neg,
        Rounding::HalfAwayFromZero => twice >= den,
        Rounding::HalfTowardZero => twice > den,
        Rounding::HalfEven => twice > den || (twice == den && q % two != I256::ZERO),
    };

    if bump {
        let adj = if neg { -I256::ONE } else { I256::ONE };
        (q + adj, r - adj * den)
    } else {
        (q, r)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ethnum::I256;

    fn q(n: i128, d: i128, m: Rounding) -> i128 {
        i128::try_from(div_round_i256(I256::from(n), I256::from(d), m).0).unwrap()
    }

    #[test]
    fn half_even_is_unbiased() {
        assert_eq!(q(5, 2, Rounding::HalfEven), 2, "2.5 -> 2");
        assert_eq!(q(3, 2, Rounding::HalfEven), 2, "1.5 -> 2");
        assert_eq!(q(-5, 2, Rounding::HalfEven), -2);
        assert_eq!(q(-3, 2, Rounding::HalfEven), -2);
    }

    #[test]
    fn half_away_from_zero_matches_postgres() {
        // PG measured: round(0.5)=1 round(1.5)=2 round(2.5)=3 round(3.5)=4. (DESIGN.md E9)
        assert_eq!(q(1, 2, Rounding::HalfAwayFromZero), 1);
        assert_eq!(q(3, 2, Rounding::HalfAwayFromZero), 2);
        assert_eq!(q(5, 2, Rounding::HalfAwayFromZero), 3);
        assert_eq!(q(7, 2, Rounding::HalfAwayFromZero), 4);
        assert_eq!(q(-5, 2, Rounding::HalfAwayFromZero), -3);
    }

    #[test]
    fn directed_modes() {
        assert_eq!(q(-5, 2, Rounding::Floor), -3);
        assert_eq!(q(5, 2, Rounding::Ceil), 3);
        assert_eq!(q(5, 2, Rounding::TowardZero), 2);
        assert_eq!(q(-5, 2, Rounding::TowardZero), -2);
        assert_eq!(q(1, 3, Rounding::AwayFromZero), 1);
    }

    /// THE conservation invariant, at the primitive level: whatever rounding moved,
    /// the residue holds. Verified exhaustively over ~2500 combinations. (DESIGN.md C5)
    #[test]
    fn residue_identity_holds_for_every_mode() {
        for n in -25i128..=25 {
            for d in 1i128..=7 {
                for m in Rounding::ALL {
                    let (quo, res) = div_round_i256(I256::from(n), I256::from(d), *m);
                    assert_eq!(quo * I256::from(d) + res, I256::from(n), "n={n} d={d} {m:?}");
                }
            }
        }
    }

    #[test]
    #[should_panic(expected = "denominator must be positive")]
    fn zero_denominator_panics() {
        let _ = div_round_i256(I256::from(1), I256::ZERO, Rounding::HalfEven);
    }
}
