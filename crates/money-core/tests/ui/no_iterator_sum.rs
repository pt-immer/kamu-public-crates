// `.sum()` must NOT exist for money. `Sum` folds through the panicking `Add`, so a partial
// total can leave the domain while the real total stays inside it -- which made the result
// depend on iteration order (and, in PostgreSQL, on the query plan). The crate removed the
// impl on purpose; the replacement is the fallible `Money::try_sum`, which accumulates wide
// and checks the domain once. This pins the removal so it cannot be reintroduced by reflex.
// (DESIGN.md R2-F4)

use kamu_money_core::iso::USD;
use kamu_money_core::money::Money;

fn main() {
    let balances = [
        Money::<USD>::try_from_units(1).unwrap(),
        Money::<USD>::try_from_units(2).unwrap(),
    ];
    // No `impl Sum for Money<C>`, so this does not resolve. Use `Money::try_sum(balances)`.
    let _total: Money<USD> = balances.into_iter().sum();
}
