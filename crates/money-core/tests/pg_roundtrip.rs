//! Money through a real PostgreSQL, over the canonical text form. (DESIGN.md C9)
//!
//! # Why `testcontainers` and not a script
//!
//! The container's lifetime is owned by the test binary: the handle is a value, and `Drop`
//! removes the container — through a panic, through a failed assertion, through `Ctrl-C`. A
//! shell script's `trap` cannot promise that, and a hand-typed `docker run` promises nothing at
//! all. This repository has already paid for the difference once, in a background poller that
//! outlived its purpose by eight hours and images nobody owned. Teardown belongs to the thing
//! that needed the container, not to whoever remembers.
//!
//! Run with `cargo test -p kamu-money-core --features postgres --test pg_roundtrip`.
//! Requires a working Docker daemon; nothing is installed on the host.

#![cfg(feature = "postgres")]

use kamu_money_core::iso::{IDR, JPY, KWD, USD};
use kamu_money_core::rate::Rate;
use kamu_money_core::{DOMAIN_MAX, Money, POW10_SCALE};
use postgres::{Client, NoTls};
use testcontainers::runners::SyncRunner;
use testcontainers_modules::postgres::Postgres;

/// A live PostgreSQL, and the container that dies with it.
struct Pg {
    client: Client,
    // Field order is the teardown order: `client` drops first and closes the connection, then
    // the container is removed. Reversing these would tear the server out from under an open
    // socket. Never reorder.
    _container: testcontainers::Container<Postgres>,
}

fn start() -> Pg {
    let container = Postgres::default().start().expect("docker must be available");
    let port = container.get_host_port_ipv4(5432).expect("mapped port");
    // 127.0.0.1, never `localhost`: localhost resolves ::1 first while Docker publishes IPv4,
    // which yields ECONNREFUSED or a 60s hang.
    let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
    let client = Client::connect(&url, NoTls).expect("connects");
    Pg { client, _container: container }
}

/// The round trip is the whole contract: what goes in comes back identical.
#[test]
fn money_round_trips_through_a_text_column() {
    let mut pg = start();
    pg.client.execute("CREATE TABLE ledger (amount text NOT NULL)", &[]).expect("table created");

    let values = [
        Money::<USD>::try_from_units(0).unwrap(),
        Money::<USD>::try_from_units(10_500_000_000_000_000_000).unwrap(),
        Money::<USD>::try_from_units(1).unwrap(),
        Money::<USD>::try_from_units(-1).unwrap(),
        Money::<USD>::try_from_units(DOMAIN_MAX).unwrap(),
        Money::<USD>::try_from_units(-DOMAIN_MAX).unwrap(),
    ];

    for value in values {
        pg.client.execute("INSERT INTO ledger VALUES ($1)", &[&value]).expect("inserted");
    }

    let rows = pg.client.query("SELECT amount FROM ledger ORDER BY ctid", &[]).expect("selected");
    let back: Vec<Money<USD>> = rows.iter().map(|r| r.get(0)).collect();

    assert_eq!(back, values.to_vec(), "every value must survive the trip");
}

/// **The stored bytes are the canonical form**, readable as text by anything — including a
/// database that has never heard of this crate. Pinning the literal is the point: it is also
/// what `kmoney`'s output function emits and what the serde wire carries.
#[test]
fn the_stored_text_is_the_canonical_form() {
    let mut pg = start();
    pg.client.execute("CREATE TABLE shapes (amount text NOT NULL)", &[]).expect("table created");

    // One generic helper rather than a table of boxed closures: the currency has to vary at
    // the TYPE level, which a closure cannot carry.
    // `Sync` is required by ToSql's parameter slice (`&[&(dyn ToSql + Sync)]`), and reaches C
    // through Money's PhantomData. Every generated currency is a unit struct, so this is
    // always satisfied -- it just has to be said in a generic context.
    fn stored<C: kamu_money_core::StaticCurrency + Sync>(pg: &mut Pg, units: i128) -> String {
        pg.client.execute("DELETE FROM shapes", &[]).unwrap();
        let m = Money::<C>::try_from_units(units).unwrap();
        pg.client.execute("INSERT INTO shapes VALUES ($1)", &[&m]).unwrap();
        pg.client.query_one("SELECT amount FROM shapes", &[]).unwrap().get(0)
    }

    let half = 10_500_000_000_000_000_000; // 10.5, whatever the currency
    assert_eq!(stored::<USD>(&mut pg, half), "USD 10.50", "settles 2dp");
    assert_eq!(stored::<JPY>(&mut pg, half), "JPY 10.5", "settles 0dp");
    assert_eq!(stored::<KWD>(&mut pg, half), "KWD 10.500", "settles 3dp");
}

/// Reading a row into the wrong currency is an ERROR, not a silent reinterpretation. This is
/// the cross-check that catches a column being read as the wrong currency, where the type
/// system alone cannot help because both sides compile.
#[test]
fn a_row_cannot_be_read_as_the_wrong_currency() {
    let mut pg = start();
    pg.client.execute("CREATE TABLE mixed (amount text NOT NULL)", &[]).expect("table created");

    let idr = Money::<IDR>::try_from_major(16_000).unwrap();
    pg.client.execute("INSERT INTO mixed VALUES ($1)", &[&idr]).expect("inserted");

    let rows = pg.client.query("SELECT amount FROM mixed", &[]).unwrap();
    let wrong: Result<Money<USD>, _> = rows[0].try_get(0);
    assert!(wrong.is_err(), "IDR must not decode as USD");

    let right: Money<IDR> = rows[0].get(0);
    assert_eq!(right, idr);
}

/// **`numeric` is not accepted, deliberately.** Accepting it would let a schema drift onto the
/// one storage type this design rejects (E13: silent rounding on ingress, uncatchable by CHECK
/// or DOMAIN), and the failure would show up as a wrong amount rather than a type error.
#[test]
fn a_numeric_column_is_refused_rather_than_silently_used() {
    let mut pg = start();
    pg.client.execute("CREATE TABLE wrong_type (amount numeric(36,18))", &[]).expect("table created");

    let m = Money::<USD>::try_from_major(10).unwrap();
    let refused = pg.client.execute("INSERT INTO wrong_type VALUES ($1)", &[&m]);
    assert!(refused.is_err(), "a numeric column must not accept Money");
}

/// Rates travel the same road, in ISO 15022 field 92B's `BASE/QUOTE/RATE` shape, and both ends
/// of the pair are checked on the way back.
#[test]
fn rates_round_trip_and_check_both_ends_of_the_pair() {
    let mut pg = start();
    pg.client.execute("CREATE TABLE quotes (rate text NOT NULL)", &[]).expect("table created");

    let rate = Rate::<USD, IDR>::try_from_units(16_000 * POW10_SCALE).unwrap();
    pg.client.execute("INSERT INTO quotes VALUES ($1)", &[&rate]).expect("inserted");

    let rows = pg.client.query("SELECT rate FROM quotes", &[]).unwrap();
    let stored: String = rows[0].get(0);
    assert_eq!(stored, "USD/IDR/16000");

    let back: Rate<USD, IDR> = rows[0].get(0);
    assert_eq!(back, rate);

    // The reversed pair must not decode: accepting it would invert the price.
    let reversed: Result<Rate<IDR, USD>, _> = rows[0].try_get(0);
    assert!(reversed.is_err(), "USD/IDR must not decode as IDR/USD");
}
