//! The sealed contract binding one SQL type to one compile-time currency.
//!
//! A pinned type carries its currency in the PostgreSQL catalog rather than in
//! the value. `kmoney_usd` and `kmoney_idr` are different SQL types, so
//! `kmoney_usd + kmoney_idr` has no operator and fails while the query is
//! parsed -- `42883` -- instead of reaching a currency check inside the
//! operator.
//!
//! This trait is the contract. It is deliberately not a document: the sealing,
//! the associated currency, and the name agreement are things the compiler
//! checks, and `tests/ui/` pins the parts a reader would otherwise take on
//! trust.

use kamu_money_core::advanced::arithmetic::UnitSum;
use kamu_money_core::{Money, StaticCurrency, text};
use pgrx::prelude::*;

use super::payload::{PinnedPayload, validate_pinned};

/// Seals [`PinnedCurrency`] against implementations this crate did not generate.
///
/// Reachable inside the crate so `pinned_money_type!` can implement it from
/// `lib.rs`, and nameable nowhere else.
pub(crate) mod sealed {
    /// The supertrait no code outside this crate can name.
    pub(crate) trait Sealed {}
}

/// A PostgreSQL type whose currency is fixed by the type itself.
///
/// # Why the ISO code is not a second associated constant
///
/// It is reachable as `<Self::Currency as StaticCurrency>::CODE`. Restating it
/// here would create a second source of truth that a generator could set apart
/// from the first.
pub(crate) trait PinnedCurrency: Copy + Sized + sealed::Sealed {
    /// `kamu-money-core`'s compile-time marker, e.g. `kamu_money_core::iso::USD`.
    type Currency: StaticCurrency;

    /// The permanent SQL type name.
    ///
    /// The generator sets this with `stringify!`, so it agrees with the Rust
    /// struct name by construction rather than by assertion -- and that
    /// agreement is what lets `rust_regtypein::<Self>()` resolve the OID, since
    /// `SqlTranslatable::TYPE_IDENT` is the same `stringify!`.
    const SQL_NAME: &'static str;

    /// Canonical units at the fixed scale of 18.
    ///
    /// Infallible. The 16-byte payload is the entire value, and a pinned type
    /// stores no currency code that could fail to resolve.
    fn units(self) -> i128;

    /// Rebuild from canonical units.
    fn from_units(units: i128) -> Self;
}

/// Parse the text form of a pinned type.
///
/// Accepts the bare amount `"10.50"`, because the column's type already names
/// the currency. Also accepts the tagged form `"USD 10.50"` — and **refuses it
/// when the tag disagrees**. That refusal is the point of the whole design:
/// without it a correctly formed value of the *wrong* currency would be
/// accepted as this one, which is the wire error per-currency typing exists to
/// prevent.
pub(crate) fn parse_pinned<T: PinnedCurrency>(input: &str) -> T {
    let expected = <T::Currency as StaticCurrency>::CODE;
    let units = if input.contains(' ') {
        match text::parse(input) {
            Ok((found, units)) if found == expected => units,
            Ok((found, _)) => {
                error!("{}: expected {}, got {}", T::SQL_NAME, expected.alpha3(), found.alpha3())
            }
            Err(e) => error!("{}: {e}, in {input:?}", T::SQL_NAME),
        }
    } else {
        match text::parse_amount(input) {
            Ok(units) => units,
            Err(e) => error!("{}: {e}, in {input:?}", T::SQL_NAME),
        }
    };
    T::from_units(units)
}

/// Render a pinned value as bare digits.
///
/// No currency prefix: the column's type carries it, so repeating it here would
/// restate a fact the catalog already guarantees — and one that could then
/// disagree with it. Goes through `Money<C>` so the digits come from the same
/// renderer the Rust type uses, rather than an adapter-local rule.
pub(crate) fn render_pinned<T: PinnedCurrency>(value: T) -> String {
    let money = Money::<T::Currency>::try_from_units(value.units())
        .unwrap_or_else(|e| error!("{}: stored amount cannot be rendered: {e}", T::SQL_NAME));
    text::render_amount(money)
}

/// The binary `SEND` payload: 16 bytes, validated exactly as the text path is.
///
/// Binary output is derived from stored bytes, so the domain check belongs here
/// too — a value that text egress would refuse must not escape through the
/// binary protocol instead.
pub(crate) fn send_pinned<T: PinnedCurrency>(value: T) -> Vec<u8> {
    let amount = validate_pinned(PinnedPayload::from_units(value.units()))
        .unwrap_or_else(|e| error!("{}: {e}", T::SQL_NAME));
    PinnedPayload::from_units(amount.units()).to_bytes().to_vec()
}

/// Width of a pinned `sum()` transition state.
///
/// Two bytes narrower than the erased type's, which appends an ISO code. There
/// is no code to carry here: the aggregate's own argument type names the
/// currency, so a state that disagreed with it could not be constructed.
pub(crate) const SUM_STATE_BYTES: usize = UnitSum::ENCODED_BYTES;

/// Encode a pinned transition state.
///
/// `bytea` for the same reason the erased aggregate uses it: `internal` would
/// need a serialize/deserialize pair before `PARALLEL = SAFE` could be declared
/// and would put raw pointers in an aggregate context, while a bespoke type
/// would add a catalog entry whose text form is meaningless.
pub(crate) fn sum_state_encode(acc: UnitSum) -> Vec<u8> {
    acc.to_le_bytes().to_vec()
}

/// Decode a pinned transition state, refusing anything that is not one.
///
/// The state type is `bytea`, so these functions are callable by hand with
/// arbitrary bytes. A forged state must be an error rather than a misread of
/// whatever was passed -- the same reasoning as the binary `RECEIVE` path.
pub(crate) fn sum_state_decode<T: PinnedCurrency>(state: &[u8]) -> UnitSum {
    let Ok(bytes) = <[u8; SUM_STATE_BYTES]>::try_from(state) else {
        error!(
            "sum({}): transition state must be exactly {SUM_STATE_BYTES} bytes, got {}",
            T::SQL_NAME,
            state.len()
        );
    };
    UnitSum::from_le_bytes(bytes)
}

/// The largest number of parts one `allocate` call will distribute into.
///
/// Same bound the erased form applies. A distribution wider than this belongs in
/// the application rather than in a single SQL call.
const MAX_ALLOCATE_PARTS: usize = 1 << 16;

/// Divide a pinned amount into `parts`, returning the quotient and its residue.
///
/// The erased form resolves a stored ISO code first and carries it into both
/// results. There is no code to resolve, and the currency of the results is the
/// type they are returned as -- so a quotient in one currency and a residue in
/// another is not a mistake this can make.
///
/// The domain check on the stored amount stays: those bytes came from a column.
pub(crate) fn divide_pinned<T: PinnedCurrency>(amount: T, parts: i32, rounding: &str) -> (T, T) {
    let units = validate_pinned(PinnedPayload::from_units(amount.units()))
        .unwrap_or_else(|e| error!("{}_div: {e}", T::SQL_NAME))
        .units();

    let Ok(parts) = u32::try_from(parts) else {
        error!("{}_div: cannot divide into {parts} parts", T::SQL_NAME);
    };
    let Some(parts) = core::num::NonZeroU32::new(parts) else {
        error!("{}_div: cannot divide into zero parts", T::SQL_NAME);
    };

    // The caller selects the rounding policy; there is no default worth guessing.
    let Some(mode) = kamu_money_core::Rounding::from_name(rounding) else {
        error!(
            "{}_div: {rounding:?} is not a rounding mode; expected one of: {}",
            T::SQL_NAME,
            kamu_money_core::Rounding::names()
        );
    };

    let (quotient, residue) = kamu_money_core::advanced::arithmetic::div_int_units(units, parts, mode)
        .unwrap_or_else(|e| error!("{}_div: stored amount cannot be divided: {e}", T::SQL_NAME))
        .take_residue();

    (T::from_units(quotient), T::from_units(residue))
}

/// Distribute a pinned amount across integer weights, conserving the total.
///
/// Every share is returned in the same type, so the sum of the results is in the
/// same currency as the input by construction rather than by check.
pub(crate) fn allocate_pinned<T: PinnedCurrency>(amount: T, weights: &[Option<i32>]) -> Vec<T> {
    let units = validate_pinned(PinnedPayload::from_units(amount.units()))
        .unwrap_or_else(|e| error!("{}_allocate: {e}", T::SQL_NAME))
        .units();

    // Reject size before any per-element work or allocation.
    if weights.is_empty() {
        error!(
            "{}_allocate: weights must not be empty -- there is no way to split an amount \
             into no parts without destroying it",
            T::SQL_NAME
        );
    }
    if weights.len() > MAX_ALLOCATE_PARTS {
        error!(
            "{}_allocate: {} weights exceeds the limit of {MAX_ALLOCATE_PARTS}; a distribution \
             that large belongs in the application, not in one SQL call",
            T::SQL_NAME,
            weights.len()
        );
    }

    let mut checked = Vec::with_capacity(weights.len());
    for weight in weights {
        let Some(weight) = *weight else {
            error!("{}_allocate: NULL weight -- a share of nothing is not a share of zero", T::SQL_NAME);
        };
        let Ok(weight) = u32::try_from(weight) else {
            error!(
                "{}_allocate: weight {weight} is negative; a negative share is not a distribution",
                T::SQL_NAME
            );
        };
        checked.push(weight);
    }
    if checked.iter().all(|&w| w == 0) {
        error!("{}_allocate: weights sum to zero -- the amount would have nowhere to go", T::SQL_NAME);
    }

    kamu_money_core::advanced::arithmetic::allocate_units(units, &checked)
        .unwrap_or_else(|e| error!("{}_allocate: stored amount cannot be allocated: {e}", T::SQL_NAME))
        .into_iter()
        .map(T::from_units)
        .collect()
}
