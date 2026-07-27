//! The residue token: money that rounding moved, which you are not allowed to ignore.

use crate::currency::StaticCurrency;
use crate::iso::Iso4217;
use crate::money::Money;
use core::marker::PhantomData;

/// The outcome of a division that may not have divided evenly: the quotient and the residue,
/// **bundled so they cannot be separated**.
///
/// This type exists because the previous signature — `-> (Money<C>, Residue<C>)` — was the
/// defect. A tuple hands the caller two independent values, so one can be kept and the other
/// dropped, and every guard the contract grew (`#[must_use]`, then a [`Drop`] bomb, then a
/// decision about release builds, then a `panicking()` guard) was policing that separation.
///
/// One value cannot be separated. There is no way to reach the quotient except through a
/// method that also decides the residue's fate, so **dropping a `Division` is safe**: no money
/// was handed out, therefore none left the ledger. That is strictly stronger than a runtime
/// bomb, and it is ordinary Rust.
///
/// | Caller writes | Old tuple API | Now |
/// |---|---|---|
/// | `let (share, _) = …` | nothing warns; runtime panic | **does not compile** |
/// | `m.div_int(3, mode);` | `#[must_use]` warns | `#[must_use]` warns |
/// | dropped mid-unwind | silent loss — C5's "one hole" | nothing was produced |
///
/// (specs.md C5)
#[must_use = "a Division holds money. Decide the residue: .take_residue() or .discard_deliberately()."]
pub struct Division<C: StaticCurrency> {
    quotient: i128,
    residue: i128,
    _c: PhantomData<C>,
}

// Hand-written for the reason at `money.rs:17`: a derive would bound the phantom parameter.
impl<C: StaticCurrency> core::fmt::Debug for Division<C> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "Division({} quotient, {} residue, {})", self.quotient, self.residue, C::CODE.alpha3())
    }
}

/// A [`Division`] whose currency is known only at run time.
///
/// The non-generic core of [`Money::div_int`], for adapters that cannot name a `C` — a
/// PostgreSQL type cannot be generic, and C9 requires the adapter to share this arithmetic
/// rather than restate it.
///
/// **It is a struct rather than a `(i128, i128)` tuple for the same reason [`Division`] is.** A
/// tuple return would be destructurable as `let (quotient, _) = …`, which warns about nothing
/// and is the pattern rustc actively suggests — so the caller must still name which exit they
/// are taking. What it cannot carry is the [`Residue`] drop bomb, because that needs a
/// currency; an adapter handing both numbers straight to another system is the one caller for
/// which that is an acceptable trade, and it is why this is not the API a Rust program should
/// reach for. Use [`Money::div_int`] there.
#[must_use = "an UntaggedDivision holds money. Decide the residue: .take_residue() or .discard_deliberately()."]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UntaggedDivision {
    quotient: i128,
    residue: i128,
}

impl UntaggedDivision {
    #[inline]
    pub(crate) const fn new(quotient: i128, residue: i128) -> Self {
        Self { quotient, residue }
    }

    /// Both halves, in canonical units. The caller is now responsible for the residue.
    #[must_use]
    pub const fn take_residue(self) -> (i128, i128) {
        (self.quotient, self.residue)
    }

    /// The quotient, having said in the name that the residue is being dropped.
    #[must_use]
    pub const fn discard_deliberately(self) -> i128 {
        self.quotient
    }

    /// Inspect the residue without consuming the division.
    #[must_use]
    pub const fn residue_units(&self) -> i128 {
        self.residue
    }
}

impl<C: StaticCurrency> Division<C> {
    #[inline]
    pub(crate) const fn new(quotient: i128, residue: i128) -> Self {
        Self { quotient, residue, _c: PhantomData }
    }

    /// The quotient. Private: reaching it must go through a residue decision.
    ///
    /// The `expect` cannot fire for any in-domain input — a quotient cannot exceed its
    /// dividend, so if this ever failed the domain invariant would already be broken, which
    /// is not a state a caller can provoke.
    #[inline]
    const fn quotient(&self) -> Money<C> {
        Money::<C>::from_units(self.quotient).expect("|quotient| <= |dividend| <= DOMAIN_MAX")
    }

    /// Take the residue and hold the obligation yourself.
    ///
    /// You asked for it, so from here the [`Residue`] rules apply in full: absorb it with
    /// [`Residue::take_units`] and post it, or say so with
    /// [`Residue::discard_deliberately`]. Letting it fall out of scope still detonates, in
    /// every profile — that backstop is why this exit is safe to offer.
    #[inline]
    pub const fn take_residue(self) -> (Money<C>, Residue<C>) {
        (self.quotient(), Residue::new(self.residue))
    }

    /// Throw the residue away, on purpose, on the record.
    ///
    /// No [`Residue`] is ever constructed on this path, so there is nothing to detonate.
    /// Named to be greppable: if you are calling this a lot, that is a finding.
    #[inline]
    #[must_use]
    pub const fn discard_deliberately(self) -> Money<C> {
        self.quotient()
    }

    /// The residue magnitude, **without** consuming the division. Inspection only.
    #[inline]
    #[must_use]
    pub const fn residue_units(&self) -> i128 {
        self.residue
    }
}

/// Money that a rounding operation moved.
///
/// In a ledger a residue must be **absorbed**: carried forward, posted to a rounding account,
/// or handed to one of the parties. Absorbing it means consuming this value through
/// [`Residue::take_units`] or [`Residue::discard_deliberately`]. Letting it fall out of scope is
/// not absorption — it is money leaving the ledger — and it is a **hard error in every profile**,
/// release included.
///
/// **You only hold one of these because you asked for it.** [`Division::take_residue`] is the
/// sole route from a division to a bare `Residue`; [`Division::discard_deliberately`] never
/// constructs one. So the bomb below is a backstop on an opt-in path rather than the primary
/// enforcement it used to be.
///
/// | Caller writes | What catches it |
/// |---|---|
/// | `let (share, _) = m.div_int(..)` | **the compiler** — `div_int` returns a [`Division`], not a tuple |
/// | `m.div_int(3, HalfEven);` | `#[must_use]` on [`Division`], at compile time |
/// | `let (share, residue) = div.take_residue();` then never using `residue` | only an `unused_variables` hint — rustc *suggests* the `_` prefix that erases it |
/// | `let (share, _) = div.take_residue();` | **nothing at compile time.** The [`Drop`] panic below is the only backstop |
///
/// The last row is why this type keeps its bomb: once you have deliberately taken the residue
/// out of the bundle, it is a free-standing value again and Rust has no linear types to stop
/// you dropping it. That limit is documented rather than papered over. (specs.md C5)
#[must_use = "this residue is MONEY. absorb it: .take_units() and post it, add it back, or .discard_deliberately()."]
pub struct Residue<C: StaticCurrency> {
    units: i128,
    ack: bool,
    _c: PhantomData<C>,
}

impl<C: StaticCurrency> Residue<C> {
    /// Create a residue.
    ///
    /// `pub`, not `pub(crate)`: this crate's own lossy operations are not the only source.
    /// specs.md's adapter pattern — `quantize(dp, mode) -> (Money, Residue)` "at the adapter"
    /// — has code outside `kamu-money-core` (a wire/Postgres boundary, say) minting a `Residue` too,
    /// so the constructor cannot be crate-private without also blocking that. That it also lets
    /// tests build one directly is a consequence of this, not the reason for it.
    ///
    /// This does mean a caller can fabricate a `Residue` claiming a loss that never happened
    /// — exactly as `Money::from_units` lets a caller fabricate an amount from nothing.
    /// `Residue` polices what happens to a loss once produced; it cannot police provenance.
    #[inline]
    pub const fn new(units: i128) -> Self {
        Self { units, ack: false, _c: PhantomData }
    }

    /// The residue magnitude, in canonical units, **without** absorbing it.
    ///
    /// Inspection only. The residue is still unabsorbed after this call and will still panic on
    /// drop; use [`Residue::take_units`] to absorb it.
    #[inline]
    #[must_use]
    pub const fn units(&self) -> i128 {
        self.units
    }

    /// The currency this residue is denominated in.
    #[inline]
    #[must_use]
    pub const fn code(&self) -> Iso4217 {
        C::CODE
    }

    /// Absorb: consume the residue and yield its units, for the caller to post somewhere.
    ///
    /// This is the normal path. Taking the units is a promise that they land in the ledger.
    #[inline]
    // Load-bearing, not decoration: `r.take_units();` as a bare statement absorbs the residue
    // and then throws the units on the floor, which is the loss this type exists to prevent
    // wearing the shape of the approved path. i128 is not itself `#[must_use]`, so nothing
    // else catches it.
    #[must_use = "these units ARE the money that rounding moved. Post them somewhere."]
    pub fn take_units(mut self) -> i128 {
        self.ack = true;
        self.units
    }

    /// Throw this money away, on purpose, on the record.
    ///
    /// Strictly this is an acknowledged **loss**, not an absorption — the money does not reach
    /// the ledger, the caller has simply accepted that. It exists as an escape hatch and is
    /// named to be greppable. If you are calling this a lot, that is a finding.
    #[inline]
    pub fn discard_deliberately(mut self) {
        self.ack = true;
    }
}

impl<C: StaticCurrency> Drop for Residue<C> {
    /// Panics if this residue is nonzero and was never absorbed — in **every** profile.
    ///
    /// An unabsorbed residue is money that left the ledger. Reporting it after the fact is not
    /// a remedy, which is why there is no counter here: either the value was absorbed, or this
    /// is a bug that must stop the program.
    fn drop(&mut self) {
        if self.ack || self.units == 0 {
            return;
        }
        // THE ONE HOLE, and it is unavoidable in Rust: panicking while already unwinding aborts
        // the process, which is strictly worse than this loss going unreported. So a residue
        // dropped DURING an unwind vanishes silently. Defensible — the operation that produced
        // it is already failing, and a ledger that rolls back never posts the residue either —
        // but it is a hole, and it is written down rather than glossed over.
        if std::thread::panicking() {
            return;
        }
        panic!(
            "unabsorbed Residue of {} units ({}) — it must go somewhere: .take_units() and \
             post it, add it back, or .discard_deliberately()",
            self.units,
            self.code().alpha3()
        );
    }
}

impl<C: StaticCurrency> core::fmt::Debug for Residue<C> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "Residue({} units, {})", self.units, self.code().alpha3())
    }
}
