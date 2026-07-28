//! The native `kmoney` column, read through the Rust drivers. (DESIGN.md C8/C9; review F8)
//!
//! # The gap this closes
//!
//! Two halves were each proven, and the join between them was not. `kamu-money-pg`'s own suite shows
//! that `kmoney::text` equals the canonical text form, but it runs *inside* the backend through
//! SPI and so cannot exercise a client driver at all. The driver suites (`pg_roundtrip`,
//! `sqlx_roundtrip`) decode that same canonical text perfectly — from `text` columns, with no
//! extension installed. Nothing put a live `kamu-money-pg`, a native `kmoney` column, and a Rust
//! driver in one process.
//!
//! That join is exactly where the contract is easy to misread. "A Rust driver reads the native
//! column as text" (R2-F5) does **not** mean the client negotiates a text format: both adapters
//! delegate `accepts`/`compatible` to `&str`, so they reject by **OID before any parsing**. The
//! cast is what changes the OID, and it is therefore part of the query contract:
//!
//! ```sql
//! SELECT amount::text FROM ledger;   -- decodes into Money<C>
//! SELECT amount        FROM ledger;  -- type error: kmoney is not text-family
//! ```
//!
//! Both directions are asserted below. The negative one matters as much as the positive: without
//! it, the deliberate absence of a native codec is indistinguishable from an accidental
//! regression that happens to still work.
//!
//! # And the write half, added 2026-07-27
//!
//! The above is all READ. Until 2026-07-27 the checked-in contract stopped there, so the primary
//! write query — the thing a service does first — existed only as an inference from "the
//! adapters reject the native OID". The second half of this file executes it: a `Money<C>` bound
//! as a parameter, cast server-side into a typmod-pinned column, updated with SQL arithmetic,
//! and read back through the projection above. It also asserts the refusal that makes the
//! typmod worth declaring — a bound `Money<IDR>` does not reach a `kmoney('USD')` column.
//!
//! # Why this one is env-gated instead of using `testcontainers`
//!
//! Every other driver test owns its container as a value, so `Drop` tears it down through a
//! panic or a `Ctrl-C`. That is the better pattern and it is kept everywhere it can be. It
//! cannot be used here: this test needs a PostgreSQL with `kmoney` *installed*, which means
//! the pgrx build image, and running the test inside that image would mean nesting a Docker
//! daemon to spawn a sibling container.
//!
//! So the container lifetime moves out to `kamu-money-pg/native-driver-test.sh`, which owns it with
//! `trap cleanup EXIT INT TERM HUP` — the shell equivalent of the same rule — and hands the URL
//! in through `MONEY_PG_NATIVE_URL`. With the variable unset the test **skips loudly** rather
//! than failing, so `just test` stays green on a machine with no Docker.
//!
//! Run it with `just test-pg-driver`.

#![cfg(all(feature = "postgres", feature = "sqlx"))]

use kamu_money_core::Money;
use kamu_money_core::advanced::domain::DOMAIN_MAX;
use kamu_money_core::iso::{IDR, USD};
// `Row` is what brings `try_get` into scope: the sqlx negative half fetches the row first and
// decodes separately, so that "rejected by OID" cannot be confused with a failed query.
use sqlx::Row;

/// The URL of a PostgreSQL that already has `kmoney` available to `CREATE EXTENSION`.
const URL_VAR: &str = "MONEY_PG_NATIVE_URL";

fn native_url() -> Option<String> {
    match std::env::var(URL_VAR) {
        Ok(u) if !u.is_empty() => Some(u),
        _ => {
            println!("skipping: {URL_VAR} is unset — run `just test-pg-driver`");
            None
        }
    }
}

/// `CREATE EXTENSION` + a genuinely native column: `kmoney('USD')`, not `text`.
///
// Setup is spelled out per driver, as `'static` statements, for two reasons that are both
// enforced by the compiler rather than by discipline:
//
//   * each test must run these through its OWN driver — calling the sync `postgres::Client`
//     inside the `#[tokio::test]` panics with "Cannot start a runtime from within a runtime",
//     the same trap `kamu-money-core/Cargo.toml` already records for the sqlx suite; and
//   * sqlx 0.9's `query`/`raw_sql` take `SqlSafeStr`, which a runtime-formatted `String`
//     deliberately is not, so a `format!`-ed table name would not compile here at all.
//
// One statement per entry: `query` carries exactly one.
const SETUP_PT: [&str; 4] = [
    "CREATE EXTENSION IF NOT EXISTS kmoney",
    "DROP TABLE IF EXISTS native_pt",
    "CREATE TABLE native_pt (id int primary key, amount kmoney('USD'))",
    "INSERT INTO native_pt VALUES (1, 'USD 10.50'), (2, 'USD -0.000000000000000001')",
];

const SETUP_SQLX: [&str; 4] = [
    "CREATE EXTENSION IF NOT EXISTS kmoney",
    "DROP TABLE IF EXISTS native_sqlx",
    "CREATE TABLE native_sqlx (id int primary key, amount kmoney('USD'))",
    "INSERT INTO native_sqlx VALUES (1, 'USD 10.50'), (2, 'USD -0.000000000000000001')",
];

// The write suites get their own tables: the read suites above assert exact row contents, and
// sharing a table would let one test's INSERT decide another's assertion.
const SETUP_PT_WRITE: [&str; 3] = [
    "CREATE EXTENSION IF NOT EXISTS kmoney",
    "DROP TABLE IF EXISTS write_pt",
    "CREATE TABLE write_pt (id int primary key, amount kmoney('USD'))",
];

const SETUP_SQLX_WRITE: [&str; 3] = [
    "CREATE EXTENSION IF NOT EXISTS kmoney",
    "DROP TABLE IF EXISTS write_sqlx",
    "CREATE TABLE write_sqlx (id int primary key, amount kmoney('USD'))",
];

/// The values a write has to survive, as `(id, amount)`.
///
/// Zero, one canonical unit either way, and **both domain edges** — the last pair matters
/// because the 18-byte fixed datum has no varlena header to absorb a wrong width, so a value at
/// `DOMAIN_MAX` is where a truncation or a sign error in the parameter path would show up. They
/// are bound as PARAMETERS here, which is the whole point: the read suites reached these same
/// values through SQL literals, so the literal parser was doing the work.
fn write_cases() -> [(i32, Money<USD>); 5] {
    [
        (1, Money::<USD>::try_from_units(0).expect("zero is in domain")),
        (2, Money::<USD>::try_from_units(1).expect("one canonical unit")),
        (3, Money::<USD>::try_from_units(-1).expect("one negative unit")),
        (4, Money::<USD>::try_from_units(DOMAIN_MAX).expect("the top edge")),
        (5, Money::<USD>::try_from_units(-DOMAIN_MAX).expect("the bottom edge")),
    ]
}

/// The `postgres-types` adapter against a native column: the cast decodes, the bare column does
/// not. Both halves are the contract.
#[test]
fn postgres_types_reads_a_native_column_through_an_explicit_cast() {
    let Some(url) = native_url() else { return };
    let mut client = postgres::Client::connect(&url, postgres::NoTls).expect("connects");
    for stmt in SETUP_PT {
        client.batch_execute(stmt).expect("extension installs and the native table is created");
    }

    // Confirm the column really is `kmoney`, so a silent fallback to `text` cannot make this
    // test pass for the wrong reason.
    let typname: String = client
        .query_one(
            "SELECT format_type(atttypid, atttypmod) FROM pg_attribute
             WHERE attrelid = 'native_pt'::regclass AND attname = 'amount'",
            &[],
        )
        .expect("catalog query")
        .get(0);
    assert_eq!(typname, "kmoney('USD')", "the column must be native kmoney");

    let row =
        client.query_one("SELECT amount::text FROM native_pt WHERE id = 1", &[]).expect("cast query runs");
    let got: Money<USD> = row.get(0);
    assert_eq!(
        got,
        Money::<USD>::try_from_major(10).unwrap()
            + Money::<USD>::try_from_units(500_000_000_000_000_000).unwrap()
    );

    // The domain edge survives the whole path: extension -> text -> driver.
    let row =
        client.query_one("SELECT amount::text FROM native_pt WHERE id = 2", &[]).expect("cast query runs");
    let got: Money<USD> = row.get(0);
    assert_eq!(got.units(), -1, "one canonical unit, through the native type");

    // NEGATIVE: the bare column is rejected by OID, before parsing. This is the deliberate
    // absence of a native codec (R2-F5) — asserted so it cannot be mistaken for a regression.
    let row = client
        .query_one("SELECT amount FROM native_pt WHERE id = 1", &[])
        .expect("the query itself is valid SQL");
    let direct: Result<Money<USD>, _> = row.try_get(0);
    assert!(
        direct.is_err(),
        "a bare native kmoney column must NOT decode: the adapters accept text-family OIDs only, \
         so `SELECT amount::text` is required. If this ever succeeds, a native codec was added \
         and DESIGN.md C8 / R2-F5 need revisiting."
    );
}

/// The same contract through `sqlx`, because C9 requires the two drivers to agree about what a
/// money column is — including about which columns they refuse.
#[tokio::test(flavor = "multi_thread")]
async fn sqlx_reads_a_native_column_through_an_explicit_cast() {
    let Some(url) = native_url() else { return };
    let pool = sqlx::postgres::PgPoolOptions::new().connect(&url).await.expect("connects");

    // Through sqlx, not the sync client — see the note on `SETUP_PT`.
    for stmt in SETUP_SQLX {
        sqlx::query(stmt).execute(&pool).await.expect("extension installs and the native table is created");
    }

    let got: Money<USD> = sqlx::query_scalar("SELECT amount::text FROM native_sqlx WHERE id = 1")
        .fetch_one(&pool)
        .await
        .expect("cast query decodes");
    assert_eq!(got.units(), 10_500_000_000_000_000_000);

    // NEGATIVE, matching the sync driver exactly — and separating EXECUTION from DECODE, so it
    // cannot pass merely because the connection died. Fetching the row succeeds (the query is
    // valid SQL); only the decode into `Money<USD>` fails, which is the claim being made.
    let raw = sqlx::query("SELECT amount FROM native_sqlx WHERE id = 1")
        .fetch_one(&pool)
        .await
        .expect("the query itself is valid SQL");
    let direct = raw.try_get::<Money<USD>, _>("amount");
    assert!(
        direct.is_err(),
        "sqlx must refuse the bare native column too — the two adapters have to agree about \
         which columns are money, and neither accepts the kmoney OID"
    );
}

// ============================================================================================
// THE WRITE HALF.
//
// Everything above is a READ. Until 2026-07-27 that was the entire checked-in contract for the
// native column: `amount::text` decodes, the bare column does not. How a value gets IN was
// documented nowhere and executed nowhere — a service had to infer the primary write query from
// the fact that the adapters reject the native OID, which is an argument, not a contract.
//
// The shape below is the canonical one, and it follows from R2-F5 rather than working around it.
// The adapters accept text-family OIDs, so the parameter travels as text and the SERVER casts:
//
//     INSERT INTO ledger (amount) VALUES (($1::text)::kmoney);
//     UPDATE ledger SET amount = amount + (($1::text)::kmoney) WHERE id = $2;
//
// `$1::text` fixes the parameter's OID to one the adapter writes; `::kmoney` runs the extension's
// own input function, which is the same codec `Display` uses. The column's typmod coercion then
// checks the currency, so a `Money<IDR>` bound at a `kmoney('USD')` column is refused by the
// database rather than by a convention someone has to remember. That refusal is asserted too —
// it is the half that makes the typmod worth declaring.
//
// This deliberately does NOT add a native-OID binary codec. The text codec stays the only one.
// ============================================================================================

/// `postgres-types`: bind, cast, update with SQL arithmetic, read back, and be refused on a
/// currency the column does not accept.
#[test]
fn postgres_types_writes_a_native_column_through_a_bound_parameter() {
    let Some(url) = native_url() else { return };
    let mut client = postgres::Client::connect(&url, postgres::NoTls).expect("connects");
    for stmt in SETUP_PT_WRITE {
        client.batch_execute(stmt).expect("extension installs and the native table is created");
    }

    // WRITE: every value bound as a parameter, never as a literal.
    for (id, amount) in write_cases() {
        client
            .execute("INSERT INTO write_pt (id, amount) VALUES ($1, ($2::text)::kmoney)", &[&id, &amount])
            .expect("the canonical write shape must work");
    }

    // READ BACK through the documented projection. This is the join the repository was missing:
    // a value that left Rust as a bound parameter, returning as the same `Money<USD>`.
    for (id, amount) in write_cases() {
        let row = client
            .query_one("SELECT amount::text FROM write_pt WHERE id = $1", &[&id])
            .expect("cast query runs");
        let got: Money<USD> = row.get(0);
        assert_eq!(got, amount, "id={id} did not survive the parameter round trip");
    }

    // The column really is native, so none of the above passed against a `text` fallback.
    let typname: String = client
        .query_one(
            "SELECT format_type(atttypid, atttypmod) FROM pg_attribute
             WHERE attrelid = 'write_pt'::regclass AND attname = 'amount'",
            &[],
        )
        .expect("catalog query")
        .get(0);
    assert_eq!(typname, "kmoney('USD')");

    // UPDATE with SQL-side arithmetic on a bound parameter. `+` here is the extension's
    // operator over the shared Rust kernel, not a client-side add — which is the reason the
    // native type exists at all.
    let ten = Money::<USD>::try_from_major(10).unwrap();
    client
        .execute("UPDATE write_pt SET amount = amount + (($1::text)::kmoney) WHERE id = $2", &[&ten, &2i32])
        .expect("the canonical update shape must work");
    let row =
        client.query_one("SELECT amount::text FROM write_pt WHERE id = 2", &[]).expect("cast query runs");
    let got: Money<USD> = row.get(0);
    assert_eq!(
        got,
        ten + Money::<USD>::try_from_units(1).unwrap(),
        "the database added a bound parameter to a stored value and kept the unit"
    );

    // NEGATIVE: the typmod refuses a foreign currency, and the DATABASE is what refuses it.
    let idr = Money::<IDR>::try_from_major(1).unwrap();
    let refused =
        client.execute("INSERT INTO write_pt (id, amount) VALUES (99, ($1::text)::kmoney)", &[&idr]);
    let err = refused.expect_err("a Money<IDR> must not reach a kmoney('USD') column");
    // Through the STRUCTURED error, not `Display`. `postgres::Error` renders as the string
    // "db error" and nothing else, so asserting on `to_string()` would have passed for any
    // failure at all — including a typo in the SQL above. Measured: it did, and this assertion
    // is what caught it.
    let message = err
        .as_db_error()
        .expect("the refusal must come from the server, not from the client")
        .message()
        .to_owned();
    assert!(
        message.contains("kmoney") && message.contains("USD"),
        "the refusal must name the column's declared currency, got: {message}"
    );
}

/// The same contract through `sqlx`. C9 requires the two drivers to agree about what a money
/// column is; H2 extends that to how a value is written to one.
#[tokio::test(flavor = "multi_thread")]
async fn sqlx_writes_a_native_column_through_a_bound_parameter() {
    let Some(url) = native_url() else { return };
    let pool = sqlx::postgres::PgPoolOptions::new().connect(&url).await.expect("connects");
    for stmt in SETUP_SQLX_WRITE {
        sqlx::query(stmt).execute(&pool).await.expect("extension installs and the native table is created");
    }

    for (id, amount) in write_cases() {
        sqlx::query("INSERT INTO write_sqlx (id, amount) VALUES ($1, ($2::text)::kmoney)")
            .bind(id)
            .bind(amount)
            .execute(&pool)
            .await
            .expect("the canonical write shape must work");
    }

    for (id, amount) in write_cases() {
        let got: Money<USD> = sqlx::query_scalar("SELECT amount::text FROM write_sqlx WHERE id = $1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .expect("cast query decodes");
        assert_eq!(got, amount, "id={id} did not survive the parameter round trip");
    }

    let ten = Money::<USD>::try_from_major(10).unwrap();
    sqlx::query("UPDATE write_sqlx SET amount = amount + (($1::text)::kmoney) WHERE id = $2")
        .bind(ten)
        .bind(2i32)
        .execute(&pool)
        .await
        .expect("the canonical update shape must work");
    let got: Money<USD> = sqlx::query_scalar("SELECT amount::text FROM write_sqlx WHERE id = 2")
        .fetch_one(&pool)
        .await
        .expect("cast query decodes");
    assert_eq!(got, ten + Money::<USD>::try_from_units(1).unwrap());

    // NEGATIVE, matching the sync driver: the typmod is the database's rule, so both drivers
    // must hit it identically. A contract only one driver enforces is not a contract.
    let idr = Money::<IDR>::try_from_major(1).unwrap();
    let refused = sqlx::query("INSERT INTO write_sqlx (id, amount) VALUES (99, ($1::text)::kmoney)")
        .bind(idr)
        .execute(&pool)
        .await;
    let err = refused.expect_err("a Money<IDR> must not reach a kmoney('USD') column");
    // Structured here too, so the two drivers are asserted against the SAME server message
    // rather than against whatever each one's `Display` happens to include — the sync driver's
    // is "db error", and a test that accepted it proved nothing.
    let message = err
        .as_database_error()
        .expect("the refusal must come from the server, not from the client")
        .message()
        .to_owned();
    assert!(
        message.contains("kmoney") && message.contains("USD"),
        "the refusal must name the column's declared currency, got: {message}"
    );
}
