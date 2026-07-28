//! Comparison, stable hashing, and the arithmetic `kmoney` is allowed to have.
//!
//! Split out of `lib.rs` on 2026-07-27. The code is UNCHANGED -- this file is
//! a relocation, verified by `just schema-hash`, which fingerprints the generated SQL surface
//! with pgrx's non-reproducible ordering normalised away (E21).
//!
//! Ordering refuses cross-currency; `=`/`<>` stay total. The guards themselves
//! (`same_currency`, `currency_or_error`, `describe`) stay in `lib.rs`: four modules use them,
//! so the root is where they belong rather than any one of their callers.

use super::{currency_or_error, describe, kmoney, same_currency};
use kamu_money_core::Iso4217;
use kamu_money_core::arith::UnitSum;
use pgrx::prelude::*;

// COMPARISON AND HASHING -- PREDICATES, NOT AN INDEX SURFACE
//
// `kmoney` is an amount SCALAR built for OLTP wallet/ledger schemas -- it is a column type, not
// a store. This crate implements no account, transaction, balance, or journal; "rows are keyed
// by account / txn id, never by amount" is the ASSUMED USAGE of the consuming schema, and the
// reason the surface below is safe to omit, not a guarantee made here.
//
// Given that assumption, it carries the comparison operators (`=`, `<>`, `<`, `<=`, `>`, `>=`)
// as PREDICATES -- `WHERE amount > 'USD 1.00'` is a sequential-scan filter -- but NO B-tree or
// hash operator class.
//
// The limitation stands on its own, independent of the assumption: with no default operator
// class there is no default sort operator, no value index, no `ORDER BY amount`, no `GROUP BY`
// / `DISTINCT` / `UNIQUE` on amount. A consumer who needs any of those needs another projection
// or type; that is a deliberate boundary of this scalar, not a property of wallets. (The absent
// default-opclass ordering is also the one surface YugabyteDB's planner would not resolve for a
// custom type, so removing it is what makes `kmoney` byte-exact on YB.)
//
// THE COMPARISON POLICY, in two halves. EQUALITY (`=`, `<>`) is TOTAL: cross-currency `=` is
// *false*, never an error -- "is USD 1.00 the same money as IDR 1.00" has the answer **no** --
// so `=`/`<>` are safe wherever PostgreSQL evaluates them, even on a column that holds several
// currencies. ORDERING (`<`, `<=`, `>`, `>=`) instead REFUSES cross-currency, erroring exactly
// like `+`: "is USD 1.00 less than IDR 1.00" has no meaningful answer, and answering it silently
// -- by ordering on `(currency, units)` -- would let `WHERE amount > 'USD 1.00'` return wrong
// rows on a column holding several currencies (a tiny ZWL amount would report *greater than*
// USD 1.00, since ZWL's ISO code outranks USD's). Within one currency the operators compare
// units directly. This is the same reason `kmoney_mixed` has no ordering at all.
//
// `kmoney_hash` (below) is retained without an opclass -- as the on-disk stable-hash contract
// and the sharpest byte-exactness signal in the ABI battery.
// =========================================================================================

#[pg_operator(immutable, parallel_safe, requires = ["kmoney_concrete"])]
#[opname(=)]
#[commutator(=)]
#[negator(<>)]
#[restrict(eqsel)]
#[join(eqjoinsel)]
const fn kmoney_eq(a: kmoney, b: kmoney) -> bool {
    a.code() == b.code() && a.units() == b.units()
}

#[pg_operator(immutable, parallel_safe, requires = ["kmoney_concrete"])]
#[opname(<>)]
#[commutator(<>)]
#[negator(=)]
#[restrict(neqsel)]
#[join(neqjoinsel)]
const fn kmoney_ne(a: kmoney, b: kmoney) -> bool {
    !kmoney_eq(a, b)
}

#[pg_operator(immutable, parallel_safe, requires = ["kmoney_concrete"])]
#[opname(<)]
#[commutator(>)]
#[negator(>=)]
#[restrict(scalarltsel)]
#[join(scalarltjoinsel)]
fn kmoney_lt(a: kmoney, b: kmoney) -> bool {
    same_currency(a, b, "<");
    a.units() < b.units()
}

#[pg_operator(immutable, parallel_safe, requires = ["kmoney_concrete"])]
#[opname(<=)]
#[commutator(>=)]
#[negator(>)]
#[restrict(scalarlesel)]
#[join(scalarlejoinsel)]
fn kmoney_le(a: kmoney, b: kmoney) -> bool {
    same_currency(a, b, "<=");
    a.units() <= b.units()
}

#[pg_operator(immutable, parallel_safe, requires = ["kmoney_concrete"])]
#[opname(>)]
#[commutator(<)]
#[negator(<=)]
#[restrict(scalargtsel)]
#[join(scalargtjoinsel)]
fn kmoney_gt(a: kmoney, b: kmoney) -> bool {
    same_currency(a, b, ">");
    a.units() > b.units()
}

#[pg_operator(immutable, parallel_safe, requires = ["kmoney_concrete"])]
#[opname(>=)]
#[commutator(<=)]
#[negator(<)]
#[restrict(scalargesel)]
#[join(scalargejoinsel)]
fn kmoney_ge(a: kmoney, b: kmoney) -> bool {
    same_currency(a, b, ">=");
    a.units() >= b.units()
}

/// The stable hash of an `kmoney`, folded to `int4`.
///
/// There is NO hash operator class on `kmoney` (amount columns in an OLTP wallet/ledger schema are
/// assumed keyed by account/txn rather than by amount), so this is not an index support function. It is retained for two reasons:
/// it is the on-disk stable-hash contract any *persisted* use depends on (an application-side
/// shard key, a durable cache key), and it is the sharpest byte-exactness signal in the ABI
/// battery -- its pinned golden values go red at the SQL boundary if a fork reads the 18-byte
/// payload at a wrong offset.
///
/// THE ALGORITHM IS PART OF THE ON-DISK CONTRACT, which is why this does not use `Hash`. Rust's
/// `Hasher::write_i128` emits native-endian bytes, so a big-endian replica would disagree, and
/// `DefaultHasher` is documented as unstable across releases. [`kamu_money_core::stable_hash`]
/// specifies the algorithm (FNV-1a over the canonical little-endian payload, then `fmix64`),
/// pins it with golden vectors checked against an independent implementation, and carries the
/// version constant a change would have to bump. Going through `kamu-money-core` rather than
/// restating it here is C9's rule: the database and the Rust program cannot disagree about a
/// stored value if only one of them defines what it means.
#[pg_extern(immutable, parallel_safe, requires = ["kmoney_concrete"])]
fn kmoney_hash(value: kmoney) -> i32 {
    kamu_money_core::stable_hash::fold_to_i32(kamu_money_core::stable_hash::stable_hash(
        value.code(),
        value.units(),
    ))
}

/// A stored value that is ALREADY outside the domain is corruption, not an arithmetic outcome.
///
/// Every ingress validates (`kmoney_in`, `recv`, the typmod cast), so reaching this means a
/// payload was written by something that bypassed them. It is reported separately from a result
/// overflow because the two need different responses: one is "your sum is too big", the other is
/// "this row is not money".
///
/// NOT `#[pg_extern]`: a plain Rust helper. It takes `Iso4217`, which has no SQL representation,
/// so it must stay outside the operator attributes below.
fn stored_in_domain_or_error(v: kmoney, currency: Iso4217, op: &str) {
    if !kamu_money_core::domain::in_domain(v.units()) {
        error!(
            "kmoney: a stored {} value is outside the domain |units| <= 10^36 - 1 and cannot be used with {op}",
            currency.alpha3()
        );
    }
}

#[pg_operator(immutable, parallel_safe, requires = ["kmoney_concrete"])]
#[opname(+)]
#[commutator(+)]
fn kmoney_add(a: kmoney, b: kmoney) -> kmoney {
    let currency = same_currency(a, b, "+");
    // Delegates to kamu-money-core's `add_units` kernel (shared with `Money::checked_add`), so the SQL
    // and Rust surfaces cannot disagree about addition.
    //
    // The kernel returns `None` for EITHER an out-of-domain operand or an out-of-domain result --
    // it enforces the precondition rather than assuming it. So the operands are checked here
    // first and reported separately: attributing a corrupt STORED value to "the result" names
    // the wrong thing and sends whoever reads it to audit the arithmetic instead of the row.
    stored_in_domain_or_error(a, currency, "+");
    stored_in_domain_or_error(b, currency, "+");
    let Some(units) = kamu_money_core::arith::add_units(a.units(), b.units()) else {
        error!(
            "kmoney: the result of {0} + {0} is outside the domain |units| <= 10^36 - 1",
            currency.alpha3()
        );
    };
    kmoney::new(units, currency.numeric())
}

#[pg_operator(immutable, parallel_safe, requires = ["kmoney_concrete"])]
#[opname(-)]
fn kmoney_sub(a: kmoney, b: kmoney) -> kmoney {
    let currency = same_currency(a, b, "-");
    // Operands checked before the result, for the reason given on `kmoney_add`.
    stored_in_domain_or_error(a, currency, "-");
    stored_in_domain_or_error(b, currency, "-");
    let Some(units) = kamu_money_core::arith::sub_units(a.units(), b.units()) else {
        error!(
            "kmoney: the result of {0} - {0} is outside the domain |units| <= 10^36 - 1",
            currency.alpha3()
        );
    };
    kmoney::new(units, currency.numeric())
}

/// `kmoney_sum(VARIADIC kmoney[])` -- sum an explicit list of amounts, exactly.
///
/// The explicit-values form, and the counterpart of Rust's `Money::try_sum`. To total a COLUMN,
/// use the `sum(kmoney)` aggregate (see `kmoney_sum_accum`) -- do not reach for
/// `kmoney_sum(VARIADIC array_agg(col))`, which materialises every row into one array before any
/// arithmetic happens and is linear in the number of rows.
///
/// This once stood in for a `sum(kmoney)` aggregate whose transition state was `kmoney` itself.
/// That aggregate inherited the `+` operator's cross-currency refusal, which was correct, but it
/// also inherited its narrow state: each partial sum had to stay in the domain, so a running
/// total that transiently left the domain and returned -- `[MAX, MAX, -MAX]` -- failed on the
/// transient. Because PostgreSQL may scan rows and combine parallel partials in any order, the
/// same multiset could sum or fail depending on the plan (R2-F4). The aggregate is back with a
/// WIDE state; this function is not what makes column totals possible any more, and it stays
/// because summing explicit values is its own operation. A variadic function has no transition
/// state and no parallel partials: it receives every argument in one call and sums them wide in
/// `kamu_money_core::arith::sum_units` (I256, one domain check at the end), so it is a function of
/// the values alone.
///
/// Currency is checked at run time here -- the fastest check, a `u16` compare against the first
/// operand's code -- because unlike Rust the currency is only known at run time. `+` keeps its
/// own identical refusal; this shares the rule, not the code path.
///
/// Returns NULL for an empty or all-NULL argument list: there is no currency-free zero in this
/// design, so an empty sum has no currency to carry, exactly as the old aggregate returned NULL
/// on no rows.
// `VariadicArray` by value, not by reference: pgrx's `#[pg_extern]` ABI takes the owned argument
// type to build the SQL wrapper, so clippy::needless_pass_by_value cannot be honoured here.
#[allow(clippy::needless_pass_by_value)]
#[pg_extern(immutable, parallel_safe, requires = ["kmoney_concrete"])]
fn kmoney_sum(values: VariadicArray<kmoney>) -> Option<kmoney> {
    let mut reference: Option<u16> = None;
    // STREAMS through `UnitSum` rather than collecting into a `Vec<i128>` and handing that to
    // `sum_units`. The array is already materialised by PostgreSQL -- that part is inherent to
    // `VARIADIC` -- but the second copy was not, and this function is the one somebody reaches for
    // with `array_agg` over a big table despite the doc comment above telling them not to.
    // `sum_units` is itself a fold over this accumulator, so the arithmetic, the per-term domain
    // check and the single check on the total are identical; only the intermediate `Vec` is gone.
    let mut acc = UnitSum::ZERO;

    // `flatten()` drops SQL NULL elements, matching the old aggregate's NULL-skipping.
    for value in values.iter().flatten() {
        match reference {
            None => reference = Some(value.code()),
            // The fastest check available: a raw `u16` compare, before anything else runs.
            Some(code) if code != value.code() => {
                let (left, right) = (describe(code), describe(value.code()));
                error!("kmoney: cannot sum {left} and {right}: different currencies");
            }
            Some(_) => {}
        }
        // NOT "sum is outside the domain": `UnitSum` enforces the domain per TERM as well as on
        // the total, and its error names the offending value in `attempted_units`. A prefix
        // asserting the SUM overflowed while the number quoted is an input would contradict the
        // message it wraps -- so the prefix only says which function refused.
        acc = acc.add_units(value.units()).unwrap_or_else(|e| error!("kmoney_sum: {e}"));
    }

    // No non-NULL operand -> no currency -> NULL, never a currencyless zero.
    let code = reference?;
    // A stored code that kamu_money_core does not know is corruption, not a currency.
    let _ = currency_or_error(code, "kmoney_sum");
    let total = acc.finish().unwrap_or_else(|e| error!("kmoney_sum: {e}"));
    Some(kmoney::new(total, code))
}

#[cfg(any(test, feature = "pg_test"))]
#[pg_schema]
mod tests {
    use pgrx::prelude::*;

    // -----------------------------------------------------------------------------------
    // Arithmetic, and the deliberate absence of it.
    // -----------------------------------------------------------------------------------

    /// `+` and `-` delegate to `kamu_money_core::arith::add_units`/`sub_units` -- the same kernel
    /// `Money::checked_add`/`checked_sub` run -- in the backend. No numeric, no base-10000
    /// limbs, no scale to lose.
    #[pg_test]
    fn addition_within_one_currency_is_exact() {
        let sum = Spi::get_one::<String>("SELECT ('USD 10.50'::kmoney + 'USD 0.50'::kmoney)::text")
            .expect("query ran")
            .expect("not null");
        assert_eq!(sum, "USD 11.00");

        let difference = Spi::get_one::<String>("SELECT ('USD 10.50'::kmoney - 'USD 0.50'::kmoney)::text")
            .expect("query ran")
            .expect("not null");
        assert_eq!(difference, "USD 10.00");
    }

    /// Exactness at the smallest representable step, which is where a float or a rounded
    /// numeric would already have given up.
    #[pg_test]
    fn addition_is_exact_at_one_unit_of_the_eighteenth_decimal() {
        let sum = Spi::get_one::<String>(
            "SELECT ('IDR 999999999999999999.999999999999999998'::kmoney
                     + 'IDR 0.000000000000000001'::kmoney)::text",
        )
        .expect("query ran")
        .expect("not null");
        assert_eq!(sum, "IDR 999999999999999999.999999999999999999");
    }

    #[pg_test(error = "kmoney: cannot compute USD + IDR: different currencies")]
    fn addition_across_currencies_is_refused_at_runtime() {
        Spi::get_one::<String>("SELECT ('USD 1.00'::kmoney + 'IDR 1.00'::kmoney)::text").ok();
    }

    /// Overflowing the domain is an error, never a wrap and never a saturation.
    #[pg_test(error = "kmoney: the result of IDR + IDR is outside the domain |units| <= 10^36 - 1")]
    fn addition_past_the_domain_top_is_refused() {
        Spi::get_one::<String>(
            "SELECT ('IDR 999999999999999999.999999999999999999'::kmoney
                     + 'IDR 0.000000000000000001'::kmoney)::text",
        )
        .ok();
    }

    #[pg_test]
    fn kmoney_sum_adds_an_explicit_list_within_one_currency() {
        let total = Spi::get_one::<String>("SELECT kmoney_sum('USD 10.50', 'USD 0.25', 'USD 0.25')::text")
            .expect("query ran")
            .expect("not null");
        assert_eq!(total, "USD 11.00");
    }

    /// The R2-F4 property, at the SQL boundary. `MAX + MAX` transiently leaves the domain while
    /// the total (`MAX`) is inside it. The removed `sum` aggregate, whose transition state was a
    /// plain `kmoney`, checked the domain on every partial, so it failed or succeeded depending
    /// on the order PostgreSQL combined rows — and `PARALLEL = SAFE` made that a plan decision.
    /// `kmoney_sum` receives every argument at once and accumulates in `I256`, so the result is
    /// the multiset's, not the plan's.
    #[pg_test]
    fn kmoney_sum_is_order_independent_across_a_domain_edge_transient() {
        let max = "USD 999999999999999999.999999999999999999";
        let neg = "USD -999999999999999999.999999999999999999";
        for expr in [
            format!("kmoney_sum('{max}', '{max}', '{neg}')"),
            format!("kmoney_sum('{max}', '{neg}', '{max}')"),
            format!("kmoney_sum('{neg}', '{max}', '{max}')"),
        ] {
            let total = Spi::get_one::<String>(&format!("SELECT ({expr})::text"))
                .expect("query ran")
                .expect("not null");
            assert_eq!(total, max, "every order of one multiset must give the same total");
        }
    }

    /// An empty (or all-NULL) argument list has no currency to carry, so it is NULL — never a
    /// currencyless zero. The same rule the removed aggregate followed for an empty input.
    #[pg_test]
    fn kmoney_sum_of_nothing_is_null() {
        // An explicit empty array rather than a bare `kmoney_sum()`: PostgreSQL will not resolve
        // a zero-argument call to a VARIADIC function (`function kmoney_sum() does not exist`),
        // and an empty array is the honest way to say "sum of no values" anyway.
        let total =
            Spi::get_one::<String>("SELECT kmoney_sum(VARIADIC ARRAY[]::kmoney[])::text").expect("query ran");
        assert!(total.is_none(), "expected NULL, got {total:?}");
    }

    /// Currency is checked at run time — the fastest check, a `u16` compare against the first
    /// operand — and a mismatched argument is refused, exactly as `+` refuses it.
    #[pg_test(error = "kmoney: cannot sum USD and IDR: different currencies")]
    fn kmoney_sum_rejects_a_mixed_currency_argument() {
        Spi::get_one::<String>("SELECT kmoney_sum('USD 1.00', 'IDR 1.00')::text").ok();
    }

    /// The domain check fires once, at the end, on the true total — `MAX` plus one unit is
    /// `10^36`, one past the domain top. The exact attempted value is in the message because
    /// `10^36` fits an `i128`; a sum too large even for `i128` would report a saturated bound.
    #[pg_test(
        error = "kmoney_sum: money domain overflow: 1000000000000000000000000000000000000 units is outside the domain |units| <= 999999999999999999999999999999999999 (NUMERIC(36,18) admits |v| < 10^18)"
    )]
    fn kmoney_sum_rejects_a_total_that_leaves_the_domain() {
        Spi::get_one::<String>(
            "SELECT kmoney_sum('USD 999999999999999999.999999999999999999', \
             'USD 0.000000000000000001')::text",
        )
        .ok();
    }

    /// Equality is currency-aware and TOTAL: `USD 1.00 = IDR 1.00` is *false*, never an error,
    /// so `=`/`<>` are safe as predicates everywhere -- even on a column holding several
    /// currencies. Ordering (`<`/`<=`/`>`/`>=`) instead refuses cross-currency (see
    /// `ordering_refuses_cross_currency`); within one currency it filters normally. There is no
    /// opclass, value index, `ORDER BY` or `UNIQUE` on `kmoney` by design.
    #[pg_test]
    fn equality_is_currency_aware_and_never_raises() {
        Spi::run("CREATE TABLE cmp (amount kmoney)").expect("table created");
        Spi::run("INSERT INTO cmp VALUES ('USD 1.00'), ('USD 2.00'), ('IDR 1.00'), ('USD 1.00')")
            .expect("rows inserted");

        // Equality is currency-aware and does NOT raise across currencies.
        let same = Spi::get_one::<bool>("SELECT 'USD 1.00'::kmoney = 'IDR 1.00'::kmoney")
            .expect("query ran")
            .expect("row");
        assert!(!same, "USD 1.00 is not the same money as IDR 1.00 — false, not an error");

        let equal = Spi::get_one::<bool>("SELECT 'USD 1.00'::kmoney = 'USD 1.00'::kmoney")
            .expect("query ran")
            .expect("row");
        assert!(equal);

        // `=` is TOTAL, so it filters the mixed column without raising: two rows equal USD 1.00,
        // and the IDR row simply does not match.
        let usd_ones = Spi::get_one::<i64>("SELECT count(*) FROM cmp WHERE amount = 'USD 1.00'::kmoney")
            .expect("query ran")
            .expect("row");
        assert_eq!(usd_ones, 2, "= is total: two USD 1.00 rows match, the IDR row does not");

        // Ordering within ONE currency works as a predicate (a sequential-scan filter), which is
        // all a wallet -- whose columns are typmod-pinned to one currency -- asks of it.
        let gt = Spi::get_one::<bool>("SELECT 'USD 2.00'::kmoney > 'USD 1.00'::kmoney")
            .expect("query ran")
            .expect("row");
        assert!(gt, "USD 2.00 > USD 1.00 within one currency");
    }

    /// Cross-currency ORDERING refuses rather than silently answering. Comparing `<`/`>` across
    /// currencies would order by ISO numeric code, so `WHERE amount > 'USD 1.00'` on a column
    /// that happens to hold several currencies could report a tiny foreign amount as *greater
    /// than* a dollar. Ordering therefore errors exactly like `+`; equality stays total.
    #[pg_test(error = "kmoney: cannot compute IDR > USD: different currencies")]
    fn ordering_refuses_cross_currency() {
        Spi::get_one::<bool>("SELECT 'IDR 1.00'::kmoney > 'USD 1.00'::kmoney").ok();
    }

    /// The opclass removal is guarded at the CATALOG, not by matching version-specific planner
    /// error text: `kmoney` and `kmoney_mixed` carry NO operator class, so `ORDER BY amount`,
    /// a value index, `GROUP BY`/`DISTINCT`/`UNIQUE` on amount all fail. If anyone re-adds an
    /// opclass, this count goes non-zero and the test goes red.
    #[pg_test]
    fn neither_type_has_an_operator_class() {
        let kmoney =
            Spi::get_one::<i64>("SELECT count(*) FROM pg_opclass WHERE opcintype = 'kmoney'::regtype")
                .expect("query ran")
                .expect("row");
        assert_eq!(kmoney, 0, "kmoney must carry no btree or hash operator class");
        let mixed =
            Spi::get_one::<i64>("SELECT count(*) FROM pg_opclass WHERE opcintype = 'kmoney_mixed'::regtype")
                .expect("query ran")
                .expect("row");
        assert_eq!(mixed, 0, "kmoney_mixed must carry no operator class either");
    }

    /// The stable hash pinned to exact numbers -- the on-disk contract, not merely consistent.
    ///
    /// `kmoney` has no hash opclass or index, but `kmoney_hash` remains the stable
    /// hash any persisted use would rely on, and pinning it is the sharpest byte-exactness
    /// signal we have: an in-process test that hashed and re-read in the same binary could never
    /// fail, which is precisely not the case that matters. What breaks a persisted hash is a
    /// REBUILD under a future toolchain producing different numbers than the ones on disk.
    ///
    /// So the numbers themselves are the assertion. They come from `kamu_money_core::stable_hash`,
    /// whose golden vectors were cross-checked against an independent implementation; these are
    /// the same values after the fold to `int4`. If a Rust upgrade, a refactor, or a return to
    /// `Hash` changes them, this goes red at the SQL boundary rather than in a customer's
    /// query results.
    #[pg_test]
    fn the_persisted_hash_values_are_pinned_not_merely_consistent() {
        for (literal, expected) in [
            ("USD 0.00", 702_888_007_i32),
            ("USD 1.00", -1_388_235_877),
            ("IDR 1.00", -129_968_833),
            ("USD -1.00", 1_671_845_669),
        ] {
            let got = Spi::get_one::<i32>(&format!("SELECT kmoney_hash('{literal}'::kmoney)"))
                .expect("query ran")
                .expect("row");
            assert_eq!(
                got, expected,
                "kmoney_hash('{literal}') changed. kmoney has no hash opclass or index, but any \
                 store that persisted the old value (a shard key, a durable cache key) is now \
                 silently wrong. This needs a kamu_money_core::stable_hash::STABLE_HASH_VERSION bump \
                 and a re-hash of any such store."
            );
        }

        // The same payload under the mixed type must hash identically -- they share a codec, so
        // two implementations would be free to drift while each looked right on its own.
        let native =
            Spi::get_one::<i32>("SELECT kmoney_hash('USD 1.00'::kmoney)").expect("query ran").expect("row");
        let mixed = Spi::get_one::<i32>("SELECT kmoney_mixed_hash('USD 1.00'::kmoney_mixed)")
            .expect("query ran")
            .expect("row");
        assert_eq!(native, mixed, "identical payloads must hash identically");
    }
}
