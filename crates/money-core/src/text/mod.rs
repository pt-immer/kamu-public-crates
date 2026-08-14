//! The canonical text form: `"<ISO> <amount>"`, one trim rule, one parser.
//!
//! This module is **not** feature-gated and does not depend on serde. It is the single place
//! the crate turns money into characters, so that the `Display` a developer reaches for and
//! the wire a service emits cannot disagree; the serde codec delegates here.
//!
//! # The rule
//!
//! Render at [`SCALE`] digits, strip trailing zeros, **stop at the currency's ISO settlement
//! exponent** (`None` for XAU/XDR/XXX, treated as 0). Never round.
//!
//! ```text
//! stored 10.500000000000000000   ->   USD 10.50   JPY 10.5   KWD 10.500   XAU 10.5
//! stored 10.000000000000000000   ->   USD 10.00   JPY 10     KWD 10.000   XAU 10
//! stored  0.000000000000000001   ->   USD 0.000000000000000001            (nothing is dropped)
//! ```
//!
//! The minimum is the **settlement** exponent, not a display one. Locale display policy stays
//! off the wire — IDR settles at 2 and renders at 0 — so using the
//! settlement number keeps this form canonical and independent of any locale.
//!
//! Padding up to the minimum is the only addition. Trimming never removes a significant digit.
//!
//! # Round-tripping, stated honestly
//!
//! Render is canonical; **parse is liberal**, accepting any exact decimal. So
//! `parse(render(v)) == v` holds for all `v`, but the converse does not — `"USD 10.5"` parses
//! and re-renders as `"USD 10.50"`. The pair is therefore a **retraction**, not a bijection.

use crate::domain::SCALE;

/// [`SCALE`] as `usize`, for string widths and byte offsets.
///
/// `SCALE as usize` would be lossless on every platform this crate can build for, but
/// `clippy::as_conversions` is denied crate-wide precisely so that "provably fine here" is
/// never the reason a cast ships. `usize::try_from` states the proof instead of assuming it,
/// and is not const, which `parse_fixed_point` needs. So the width is written once as a literal
/// and *tied* to [`SCALE`] by an assertion the compiler evaluates: move [`SCALE`] and this fails
/// to build, rather than parsing at a width that no longer matches the scale.
pub(crate) const SCALE_USIZE: usize = 18;
const _: () = assert!(SCALE == 18);

mod display;
mod parse;
mod render;

#[cfg(feature = "serde")]
pub(crate) use parse::parse_rate_amount;
pub use parse::{parse, parse_amount};
pub(crate) use render::fixed_point_parts;
#[cfg(feature = "serde")]
pub(crate) use render::render_rate;
pub use render::{render, render_amount};
