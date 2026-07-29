//! Compare money-kernel costs with `rust_decimal`. This benchmark is not a gate.
//!
//! # Method
//!
//! The fixture reports best, median, worst, and spread over repeated samples. It stays
//! dependency-light instead of adding Criterion for a non-gating diagnostic.
//!
//! # Why there is no threshold
//!
//! Hardware-dependent results print without pass/fail thresholds. A future gate needs a
//! baseline from named hardware.
//!
//! ```text
//! just bench-rust
//! ```
//!
//! # What is deliberately asymmetric
//!
//! `text::parse` resolves a currency and `text::render` emits one; `Decimal` does neither.
//! `parse_amount` is the digits-only comparison.
//!
//! `div_int` also returns a residue that the benchmark must acknowledge.

use std::hint::black_box;
use std::num::NonZeroU32;
use std::time::Instant;

use kamu_money_core::iso::USD;
use kamu_money_core::{Money, Rounding, text};
use rust_decimal::Decimal;
use rust_decimal::prelude::FromPrimitive;

/// Samples per operation.
const SAMPLES: usize = 9;

/// Iterations per sample, selected by operation cost.
const FAST: usize = 1_000_000; // single arithmetic ops, a few ns each
const MEDIUM: usize = 200_000; // parse, render, division -- tens to hundreds of ns
const FOLD: usize = 2_000; // 1000-element folds, so 2M element-operations

/// One benchmark row.
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
    /// Worst sample as a multiple of the best.
    fn spread(&self) -> f64 {
        self.worst() / self.best()
    }
}

/// Time `f` over `inner` iterations, `SAMPLES` times, and keep every sample.
///
/// `black_box` protects both input and output from loop hoisting or dead-code elimination.
fn bench<T>(name: &'static str, note: &'static str, inner: usize, mut f: impl FnMut() -> T) -> Row {
    // Warm caches and lazy statics before sampling.
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
    // `just bench-rust` prints compiler, target, and host identity.
    println!("kamu-money-core kernel benchmark — diagnostic only; no threshold");
    println!();
    println!("  samples        {SAMPLES} per row; {FAST}/{MEDIUM}/{FOLD} iterations by cost class");
    println!();
    // Refuse at runtime so `clippy --all-targets` can still compile this target.
    if cfg!(debug_assertions) {
        eprintln!("run this in release mode; debug results are not comparable. Use `just bench-rust`.");
        std::process::exit(2);
    }

    // Use values inside both domains so neither side benchmarks its failure path.
    let a = Money::<USD>::try_from_units(10_500_000_000_000_000_000).expect("in domain");
    let b = Money::<USD>::try_from_units(250_000_000_000_000_000).expect("in domain");
    let da = Decimal::from_f64(10.5).expect("representable");
    let db = Decimal::from_f64(0.25).expect("representable");
    let three = NonZeroU32::new(3).expect("nonzero");

    // Validate the fixture before timing it.
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

    // Raw checked arithmetic is the comparison floor; Money adds its domain check.
    rows.push(bench("i128::checked_add (floor)", "no domain check", FAST, || {
        black_box(10_500_000_000_000_000_000_i128).checked_add(black_box(250_000_000_000_000_000))
    }));

    // Summation, per element. Built once outside the timed closure: allocating the input inside
    // it would time the allocator.
    let money_terms: Vec<Money<USD>> = (0..1000)
        .map(|i| Money::<USD>::try_from_units(i128::from(i) * 1_000_000_000_000_000).expect("in domain"))
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

    // Money returns a residue; Decimal's checked division discards its remainder.
    rows.push(bench(
        "Money::div_int + take_residue",
        "returns a Residue the caller must handle",
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
    rows.push(bench("text::parse (with ISO code)", "also resolves a currency", MEDIUM, || {
        text::parse(black_box("USD 10.50"))
    }));

    rows.push(bench("text::render", "also emits the ISO code", MEDIUM, || {
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
