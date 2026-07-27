//! Money through `sqlx`, and the cross-driver agreement that makes "one codec" testable.
//! (specs.md C9)
//!
//! The round-trip half is the same contract `pg_roundtrip.rs` asserts for `postgres-types`. The
//! part that could not be tested until both existed is the **differential**: a value written by
//! one driver must read back through the other, byte for byte. Two adapters that each round-trip
//! correctly can still disagree with each other; only writing with one and reading with the
//! other catches that.
//!
//! Run with `cargo test -p kamu-money-core --features postgres,sqlx --test sqlx_roundtrip`.

#![cfg(all(feature = "sqlx", feature = "postgres"))]

use kamu_money_core::iso::{IDR, JPY, USD};
use kamu_money_core::rate::Rate;
use kamu_money_core::{DOMAIN_MAX, Money, POW10_SCALE};
use sqlx::{Row, postgres::PgPoolOptions};
use testcontainers::runners::AsyncRunner;
use testcontainers_modules::postgres::Postgres as PgImage;

/// A live PostgreSQL and the container that dies with it.
///
/// The container handle is returned alongside the URL and must be held for the whole test: drop
/// it early and the server disappears mid-query. `Drop` is the teardown, through a panic or a
/// failed assertion alike.
// AsyncRunner, not SyncRunner: the sync runner blocks internally, and blocking inside a
// #[tokio::test] panics with "Cannot start a runtime from within a runtime".
async fn start() -> (testcontainers::ContainerAsync<PgImage>, String) {
    let container = PgImage::default().start().await.expect("docker must be available");
    let port = container.get_host_port_ipv4(5432).await.expect("mapped port");
    // 127.0.0.1, never `localhost`: localhost resolves ::1 first while Docker publishes IPv4.
    let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
    (container, url)
}

#[tokio::test(flavor = "multi_thread")]
async fn money_round_trips_through_sqlx() {
    let (_container, url) = start().await;
    let pool = PgPoolOptions::new().connect(&url).await.expect("connects");

    sqlx::query("CREATE TABLE ledger (amount text NOT NULL)").execute(&pool).await.expect("table created");

    let values = [
        Money::<USD>::from_units(0).unwrap(),
        Money::<USD>::from_units(10_500_000_000_000_000_000).unwrap(),
        Money::<USD>::from_units(1).unwrap(),
        Money::<USD>::from_units(-1).unwrap(),
        Money::<USD>::from_units(DOMAIN_MAX).unwrap(),
        Money::<USD>::from_units(-DOMAIN_MAX).unwrap(),
    ];

    for value in values {
        sqlx::query("INSERT INTO ledger VALUES ($1)").bind(value).execute(&pool).await.expect("inserted");
    }

    let rows =
        sqlx::query("SELECT amount FROM ledger ORDER BY ctid").fetch_all(&pool).await.expect("selected");
    let back: Vec<Money<USD>> = rows.iter().map(|r| r.get(0)).collect();

    assert_eq!(back, values.to_vec(), "every value must survive the trip");
}

/// Reading a row into the wrong currency is an error here too — the check lives in `FromStr`,
/// which both drivers go through, so neither can skip it.
#[tokio::test(flavor = "multi_thread")]
async fn a_row_cannot_be_read_as_the_wrong_currency() {
    let (_container, url) = start().await;
    let pool = PgPoolOptions::new().connect(&url).await.expect("connects");
    sqlx::query("CREATE TABLE mixed (amount text NOT NULL)").execute(&pool).await.expect("table created");

    let idr = Money::<IDR>::from_major(16_000).unwrap();
    sqlx::query("INSERT INTO mixed VALUES ($1)").bind(idr).execute(&pool).await.expect("inserted");

    let row = sqlx::query("SELECT amount FROM mixed").fetch_one(&pool).await.unwrap();
    assert!(row.try_get::<Money<USD>, _>(0).is_err(), "IDR must not decode as USD");
    assert_eq!(row.get::<Money<IDR>, _>(0), idr);
}

/// The **fifth ingress** for `Rate`'s positivity rule (H1; specs.md C6).
///
/// The other four — raw constructor, text parser, and serde's two forms — are proven offline in
/// `rate_ingress.rs`, along with `postgres-types`, whose `FromSql` is a pure function of bytes
/// and an OID. sqlx's `Decode` is not: it wants a `PgValueRef`, and hand-building one would
/// prove a decoder works on a value the server never produces. So this half is proven against a
/// real column on a real server, which is the thing the claim is actually about.
///
/// Note how the bad rows have to be inserted as **raw text**. There is no longer any way to
/// produce them through the typed path — `Rate::try_from_units` refuses to build the value in
/// the first place — so forging the row is the only way to test the read direction at all. That
/// awkwardness is the invariant working, not a gap in the test.
#[tokio::test(flavor = "multi_thread")]
async fn sqlx_refuses_a_non_positive_rate_from_a_column() {
    let (_container, url) = start().await;
    let pool = PgPoolOptions::new().connect(&url).await.expect("connects");
    sqlx::query("CREATE TABLE quotes (rate text NOT NULL)").execute(&pool).await.expect("table created");
    sqlx::query("INSERT INTO quotes VALUES ('USD/IDR/-2'), ('USD/IDR/0'), ('USD/IDR/16000')")
        .execute(&pool)
        .await
        .expect("inserted");

    for bad in ["USD/IDR/-2", "USD/IDR/0"] {
        // EXECUTION and DECODE are separated, as in the native-column suite: fetching the row
        // must succeed, so that "rejected by the codec" cannot be confused with a failed query.
        let row = sqlx::query("SELECT rate FROM quotes WHERE rate = $1")
            .bind(bad)
            .fetch_one(&pool)
            .await
            .expect("the query itself is valid SQL");
        assert!(
            row.try_get::<Rate<USD, IDR>, _>(0).is_err(),
            "{bad} decoded into a Rate -- sqlx is weaker than the constructor"
        );
    }

    let row = sqlx::query("SELECT rate FROM quotes WHERE rate = 'USD/IDR/16000'")
        .fetch_one(&pool)
        .await
        .expect("the query itself is valid SQL");
    assert_eq!(
        row.get::<Rate<USD, IDR>, _>(0),
        Rate::<USD, IDR>::from_units(16_000 * POW10_SCALE).unwrap(),
        "a real quote still decodes, so the refusals above are about the value"
    );
}

/// **THE DIFFERENTIAL.** A value written by `sqlx` read back by `postgres-types`, and the
/// reverse. Each adapter round-tripping correctly on its own does not prove they agree with
/// each other; this does.
#[tokio::test(flavor = "multi_thread")]
async fn the_two_drivers_agree_byte_for_byte() {
    let (_container, url) = start().await;
    let pool = PgPoolOptions::new().connect(&url).await.expect("connects");
    sqlx::query("CREATE TABLE shared (tag text NOT NULL, amount text NOT NULL)")
        .execute(&pool)
        .await
        .expect("table created");

    let value = Money::<USD>::from_units(10_500_000_000_000_000_000).unwrap();
    let jpy = Money::<JPY>::from_units(10_500_000_000_000_000_000).unwrap();
    let rate = Rate::<USD, IDR>::from_units(16_000 * POW10_SCALE).unwrap();

    // sqlx writes.
    sqlx::query("INSERT INTO shared VALUES ('usd', $1)").bind(value).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO shared VALUES ('jpy', $1)").bind(jpy).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO shared VALUES ('rate', $1)").bind(rate).execute(&pool).await.unwrap();
    drop(pool);

    // postgres-types reads, on a fresh SYNCHRONOUS connection -- inside spawn_blocking, because
    // the sync client blocks and blocking on a runtime thread panics with "Cannot start a
    // runtime from within a runtime". Same trap as SyncRunner above, mirrored.
    let sync_url = url.clone();
    let written_back = Money::<USD>::from_units(-1).unwrap();
    tokio::task::spawn_blocking(move || {
        let mut client = postgres::Client::connect(&sync_url, postgres::NoTls).expect("connects");

        let usd_row = client.query_one("SELECT amount FROM shared WHERE tag = 'usd'", &[]).unwrap();
        assert_eq!(usd_row.get::<_, Money<USD>>(0), value, "sqlx -> postgres");
        assert_eq!(usd_row.get::<_, String>(0), "USD 10.50", "stored form");

        let jpy_row = client.query_one("SELECT amount FROM shared WHERE tag = 'jpy'", &[]).unwrap();
        assert_eq!(jpy_row.get::<_, String>(0), "JPY 10.5", "settlement dp");

        let rate_row = client.query_one("SELECT amount FROM shared WHERE tag = 'rate'", &[]).unwrap();
        assert_eq!(rate_row.get::<_, Rate<USD, IDR>>(0), rate);
        assert_eq!(rate_row.get::<_, String>(0), "USD/IDR/16000");

        // ...and postgres-types writes, for sqlx to read below.
        client.execute("INSERT INTO shared VALUES ('neg', $1)", &[&written_back]).unwrap();
    })
    .await
    .expect("blocking half completed");

    // sqlx reads what postgres-types wrote.
    let pool = PgPoolOptions::new().connect(&url).await.expect("reconnects");
    let row = sqlx::query("SELECT amount FROM shared WHERE tag = 'neg'").fetch_one(&pool).await.unwrap();
    assert_eq!(row.get::<Money<USD>, _>(0), written_back, "postgres -> sqlx");
    assert_eq!(
        row.get::<String, _>(0),
        "USD -0.000000000000000001",
        "the smallest representable value survives both drivers"
    );
}
