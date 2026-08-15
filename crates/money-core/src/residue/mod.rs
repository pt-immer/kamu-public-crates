//! Explicit accounting for units moved by integer division.
//!
//! [`Division`] keeps quotient and residue together. A caller cannot obtain the
//! quotient without choosing [`Division::take_residue`] or
//! [`Division::discard_deliberately`]. Dropping an undecided division is safe:
//! no quotient escaped.
//!
//! [`Residue`] is a `#[must_use]` accounting obligation, not a runtime trap.
//! Rust does not provide linear types, so code can still suppress the lint and
//! drop a bare residue. The API makes that choice visible without introducing a
//! panic in `Drop`, including during cancellation or unwinding.
//!
//! [`UntaggedDivision`] enforces the same thing at the same strength: it is
//! neither `Copy` nor `Clone`, so its quotient cannot be released a second time
//! after the residue has been decided. What it gives up is the currency, which
//! it carries nowhere.

mod division;
mod obligation;
mod untagged;

pub use division::Division;
pub use obligation::Residue;
pub use untagged::UntaggedDivision;
