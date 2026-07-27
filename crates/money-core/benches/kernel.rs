//! What the money kernel costs, against `rust_decimal`. **Never a gate.**
//!
//! # Why this exists
//!
//! E20 measured these numbers and then threw the measurement code away, which left `specs.md`
//! §1 saying *"reproduce before trusting"* directly above an entry nobody could reproduce. An
//! external review called that out and was right: one run's observations on one host are a
//! reason to decide query shape and batching, not auditable facts. This is the fixture that
//! makes them auditable.
//!
//! # Why not `criterion`
//!
//! Because the job is to **reproduce E20**, not to produce a better-founded different number.
//! E20 reports best-of-N, so this reports best-of-N — swapping in criterion's bootstrapped mean
//! would give figures that cannot be compared against the entry they exist to corroborate, while
//! adding ~20 crates to a workspace that deliberately has few. Dispersion is reported alongside
//! (median, worst, and spread) so a reader can see whether a minimum is a stable floor or the
//! lucky tail of a noisy distribution — which is the actual thing criterion would buy here.
//!
//! # Why there is no threshold
//!
//! Same rule as `just bench-yb`: a limit invented before there is something to regress against
//! either never fires or fires on somebody else's hardware. This prints; it never fails. If a
//! number here is ever to become a gate, it needs a baseline recorded on known hardware first,
//! and that is a decision, not a default.
//!
//! ```text
//! just bench-rust
//! ```
//!
//! # What is deliberately asymmetric
//!
//! Two comparisons below are **not** like-for-like, and are labelled rather than quietly averaged
//! in: `text::parse` resolves an ISO currency and `text::render` emits one, which `Decimal` never
//! has to do. `parse_amount` is the digits-only counterpart, and it is the row to compare.
//!
//! And `div_int` forces the caller to deal with the residue (C5) where `Decimal::checked_div`
//! discards it silently. That is a correctness difference showing up as a time difference; the
//! first draft of this benchmark panicked because it dropped a `Residue`, which is the drop-bomb
//! doing its job.

use std::hint::black_box;
use std::num::NonZeroU32;
use std::time::Instant;

use kamu_money_core::iso::USD;
use kamu_money_core::{Money, Rounding, text};
use rust_decimal::Decimal;
use rust_decimal::prelude::FromPrimitive;

/// Samples per operation. Each sample times some number of iterations, so the per-op figure is a
/// mean over those and the reported statistics are over `SAMPLES` of those means.
const SAMPLES: usize = 9;

/// Iterations per sample, chosen per operation so every row takes roughly the same wall-clock.
///
/// A single constant does not work: a 4 ns add and a 1000-element fold differ by four orders of
/// magnitude, so one million iterations is a fraction of a second for the first and several
/// minutes for the second. The first draft used one million for everything and had to be killed.
const FAST: usize = 1_000_000; // single arithmetic ops, a few ns each
const MEDIUM: usize = 200_000; // parse, render, division -- tens to hundreds of ns
const FOLD: usize = 2_000; // 1000-element folds, so 2M element-operations

/// One measured operation.
struct Row {
    name: &'static str,
    /// Nanoseconds per iteration, one entry per sample, ascending.
    ns: Vec<f64>,
    note: &'static str,
}

impl Row {
    fn best(&self) -> f64 {
        self.ns[0]
    }
    fn median(&self) -> f64 {
        self.ns[self.ns.len() / 2]
    }
    fn worst(&self) -> f64 {
        self.ns[self.ns.len() - 1]
    }
    /// Worst as a multiple of best. A wide spread means the minimum is a lucky sample rather
    /// than a floor, and the reader should distrust small differences between rows.
    fn spread(&self) -> f64 {
        self.worst() / self.best()
    }
}

/// Time `f` over `inner` iterations, `SAMPLES` times, and keep every sample.
///
/// `black_box` on both the input and the result: without it the optimiser is entitled to hoist a
/// loop-invariant computation out, or delete it entirely for having no observable effect. E20
/// recorded three SQL benchmarks that measured nothing for the query-planner equivalent of that
/// mistake; the Rust equivalent is quieter and just as wrong.
fn bench<T>(name: &'static str, note: &'static str, inner: usize, mut f: impl FnMut() -> T) -> Row {
    // One untimed pass, so the first sample is not paying for cold caches and lazy statics.
    black_box(f());

    let mut ns: Vec<f64> = Vec::with_capacity(SAMPLES);
    for _ in 0..SAMPLES {
        let start = Instant::now();
        for _ in 0..inner {
            black_box(f());
        }
        let elapsed = start.elapsed();
        #[expect(
            clippy::cast_precision_loss,
            reason = "nanos as f64: a benchmark duration is nowhere near 2^53 ns (104 days)"
        )]
        ns.push(elapsed.as_nanos() as f64 / inner as f64);
    }
    ns.sort_by(f64::total_cmp);
    Row { name, ns, note }
}

fn main() {
    // ENVIRONMENT IDENTITY is printed by `just bench-rust` around this output, not built in
    // here: rustc's version, the target triple and the CPU model are the recipe's to state (it
    // can shell out), and a `build.rs` existing only to inject them into a non-gating benchmark
    // would be a build-time cost every ordinary `cargo build` paid. What the binary itself knows
    // is the profile, and that one matters enough to be an assertion rather than a line of text.
    println!("kamu-money-core kernel benchmark — NOT A GATE, no pass/fail threshold");
    println!();
    println!("  samples        {SAMPLES} per row; {FAST}/{MEDIUM}/{FOLD} iterations by cost class");
    println!();
    // `if cfg!(...)` rather than `assert!(!cfg!(...))`: the assert form is a constant expression
    // and clippy rejects it, and `compile_error!` under `#[cfg(debug_assertions)]` would be worse
    // still -- `just lint` runs clippy `--all-targets` in DEBUG, so a build-time refusal here
    // would break the gate rather than the benchmark. Refuse at run time, loudly, and exit.
    if cfg!(debug_assertions) {
        eprintln!(
            "run this in RELEASE. A debug build measures the absence of optimisation, and the \
             resulting numbers are not slow versions of the real ones -- they are about different \
             code. Use `just bench-rust`."
        );
        std::process::exit(2);
    }

    // VALUES INSIDE BOTH DOMAINS. E4 established that `Money` and `Decimal` are incomparable at
    // the edges, so a benchmark over `Money`'s full domain would be timing `rust_decimal`'s
    // failure path and reporting it as a win.
    let a = Money::<USD>::from_units(10_500_000_000_000_000_000).expect("in domain");
    let b = Money::<USD>::from_units(250_000_000_000_000_000).expect("in domain");
    let da = Decimal::from_f64(10.5).expect("representable");
    let db = Decimal::from_f64(0.25).expect("representable");
    let three = NonZeroU32::new(3).expect("nonzero");

    // CORRECTNESS FIRST, BEFORE ANY TIMING. A benchmark that measures a wrong implementation is
    // worse than no benchmark: it is an argument for shipping the wrong thing. These assertions
    // cost nothing here and mean every row below describes code that agrees with the contract.
    assert_eq!(
        a.checked_add(b).expect("in domain").units(),
        10_750_000_000_000_000_000,
        "checked_add disagrees with the value the timing rows are about"
    );
    assert_eq!(text::render(a.units(), a.code()).expect("renders"), "USD 10.50");
    assert_eq!(text::parse("USD 10.50").expect("parses").1, a.units());

    let mut rows = Vec::new();

    rows.push(bench("Money::checked_add", "", FAST, || black_box(a).checked_add(black_box(b))));
    rows.push(bench("Decimal::checked_add", "", FAST, || black_box(da).checked_add(black_box(db))));

    // THE FLOOR. Everything above it is the domain check, which is the whole reason E2/E3's
    // silent-corruption class cannot occur here — so this row is what that safety costs.
    rows.push(bench("i128::checked_add (floor)", "no domain check", FAST, || {
        black_box(10_500_000_000_000_000_000_i128).checked_add(black_box(250_000_000_000_000_000))
    }));

    // Summation, per element. Built once outside the timed closure: allocating the input inside
    // it would time the allocator.
    let money_terms: Vec<Money<USD>> = (0..1000)
        .map(|i| Money::<USD>::from_units(i128::from(i) * 1_000_000_000_000_000).expect("in domain"))
        .collect();
    let decimal_terms: Vec<Decimal> = (0..1000).map(Decimal::from).collect();
    let n = money_terms.len();
    {
        // Per-element figures, so divide the whole-fold time by the element count.
        let fold_money = bench("Money::try_sum (1000 terms)", "", FOLD, || {
            Money::<USD>::try_sum(black_box(&money_terms).iter().copied())
        });
        let fold_decimal = bench("Decimal sum (1000 terms)", "", FOLD, || {
            black_box(&decimal_terms).iter().copied().sum::<Decimal>()
        });
        #[expect(clippy::cast_precision_loss, reason = "1000 is exact in f64")]
        let per = n as f64;
        rows.push(Row {
            name: "Money::try_sum, per element",
            ns: fold_money.ns.iter().map(|s| s / per).collect(),
            note: "",
        });
        rows.push(Row {
            name: "Decimal sum, per element",
            ns: fold_decimal.ns.iter().map(|s| s / per).collect(),
            note: "",
        });
    }

    // ASYMMETRIC, AND THE ASYMMETRY IS THE POINT: `div_int` hands back a `Residue` the caller
    // must absorb, `checked_div` throws it away. `discard_deliberately()` is how a caller says
    // "yes, really" — dropping it undecided detonates, in every profile.
    rows.push(bench(
        "Money::div_int + take_residue",
        "returns a Residue the caller MUST handle (C5)",
        MEDIUM,
        || {
            let (q, r) = black_box(a).div_int(three, Rounding::HalfEven).take_residue();
            r.discard_deliberately();
            q
        },
    ));
    rows.push(bench("Decimal::checked_div", "discards the remainder silently", MEDIUM, || {
        black_box(da).checked_div(black_box(Decimal::from(3)))
    }));

    rows.push(bench("text::parse_amount (digits only)", "the like-for-like parse row", MEDIUM, || {
        text::parse_amount(black_box("10.50"))
    }));
    rows.push(bench("Decimal::from_str_exact", "the like-for-like parse row", MEDIUM, || {
        black_box("10.50").parse::<Decimal>()
    }));
    rows.push(bench(
        "text::parse (with ISO code)",
        "ASYMMETRIC: resolves a currency Decimal has no concept of",
        MEDIUM,
        || text::parse(black_box("USD 10.50")),
    ));

    rows.push(bench("text::render", "ASYMMETRIC: emits the ISO code as well as the digits", MEDIUM, || {
        text::render(black_box(a.units()), black_box(a.code()))
    }));
    rows.push(bench("Decimal::to_string", "", MEDIUM, || black_box(da).to_string()));

    // "note" is in the format string rather than an argument: a bare `{}` fed a literal is
    // clippy::print_literal, and the column needs no width because it is last.
    println!("{:<34} {:>10} {:>10} {:>10} {:>8}  note", "operation", "best ns", "median", "worst", "spread");
    let rule = "-".repeat(100);
    println!("{rule}");
    for r in &rows {
        println!(
            "{:<34} {:>10.2} {:>10.2} {:>10.2} {:>7.2}×  {}",
            r.name,
            r.best(),
            r.median(),
            r.worst(),
            r.spread(),
            r.note
        );
    }

    println!();
    println!("RAW SAMPLES (ns/iteration, ascending) — retained so the summary above is checkable:");
    for r in &rows {
        let samples: Vec<String> = r.ns.iter().map(|s| format!("{s:.2}")).collect();
        println!("  {:<34} {}", r.name, samples.join(" "));
    }
    println!();
    println!("Read a row with a spread above ~1.5× as noise, not signal: on a shared machine the");
    println!("minimum is the least-perturbed sample, and a wide spread means it was lucky rather");
    println!("than a floor. Re-run on a quiet host before quoting a small difference.");
}
