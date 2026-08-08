//! Money through YugabyteDB using the canonical text form.
//!
//! # What this file covers, and what it does NOT
//!
//! This is the **text-adapter** route to YugabyteDB, not the whole of YugabyteDB support.
//!
//! This covers the portable text-adapter route. Native per-currency support has its own extension
//! gate. The text route needs only a text column and keeps arithmetic in Rust.
//!
//! # No `testcontainers-modules` image for YugabyteDB
//!
//! There is no module-specific image wrapper, so this drives a `GenericImage` directly:
//!
//! - `yugabyted` binds YSQL to the node's **advertised address**, never to loopback. Inside the
//!   container that is `hostname -i`; from the host it is the mapped port, which is what this
//!   uses. Connecting to `127.0.0.1:5433` *inside* the container gives ECONNREFUSED and reads
//!   like a slow start.
//! - Startup is slow (tens of seconds) and the port is listening before YSQL will answer, so
//!   readiness is a successful query, not an open socket.
//!
//! Run with `just test-yb`, which resolves the image identity through the pin file and passes it
//! in. A bare `cargo test` refuses for want of `KMONEY_YB_IMAGE` — see `yb_image` for why there
//! is no default. Ignored by default — it pulls a ~1.6GB image and takes about a minute.

#![cfg(feature = "postgres")]

use kamu_money_core::advanced::domain::{DOMAIN_MAX, POW10_SCALE};
use kamu_money_core::iso::{IDR, JPY, KWD, USD};
use kamu_money_core::{Money, Rate};
use postgres::{Client, NoTls};
use std::time::Duration;
use testcontainers::core::{IntoContainerPort, WaitFor};
use testcontainers::runners::SyncRunner;
use testcontainers::{GenericImage, ImageExt};

/// The YugabyteDB image this runs against, **supplied by the caller and recorded by it**.
///
/// `just test-yb` resolves the pinned identity through `yb/yb-image.sh`. Absence is an error;
/// this test has no mutable-tag fallback.
fn yb_image() -> (String, String) {
    let reference = std::env::var("KMONEY_YB_IMAGE").unwrap_or_else(|_| {
        panic!(
            "KMONEY_YB_IMAGE is not set.\n\
             This test runs against the YugabyteDB identity its caller resolved, checked against \
             the pin file, and recorded. It carries no default, because the default is what went \
             stale.\n\
             Run it as:  just test-yb"
        )
    });
    // `testcontainers` composes the reference as `format!("{name}:{tag}")`, so splitting on the
    // LAST colon round-trips both forms: `repo:tag`, and a digest `repo@sha256:<hex>` as
    // ("repo@sha256", "<hex>"). Checked against testcontainers 0.27.3,
    // `src/core/containers/request.rs::descriptor`.
    match reference.rsplit_once(':') {
        Some((name, tag)) if !name.is_empty() && !tag.is_empty() => (name.to_owned(), tag.to_owned()),
        _ => panic!("KMONEY_YB_IMAGE must be `repo:tag` or `repo@sha256:<hex>`, got {reference:?}"),
    }
}

struct Yb {
    client: Client,
    // Teardown order: client first so the connection closes, then the container. Never reorder.
    _container: testcontainers::Container<GenericImage>,
}

fn start() -> Yb {
    let (image, tag) = yb_image();
    let container = GenericImage::new(image, tag)
        .with_exposed_port(5433.tcp())
        // The log line is a coarse gate only; YSQL answers later than it appears.
        .with_wait_for(WaitFor::message_on_stdout("YugabyteDB Started"))
        .with_cmd(["bin/yugabyted", "start", "--background=false"])
        .start()
        .expect("docker must be available");

    let port = container.get_host_port_ipv4(5433).expect("mapped port");
    // 127.0.0.1 from the host side, against the MAPPED port. (Inside the container the address
    // would have to be `hostname -i` — yugabyted never binds loopback.)
    let url = format!("postgres://yugabyte@127.0.0.1:{port}/yugabyte");

    // Readiness is a query that succeeds, not a port that accepts. Roughly two minutes of
    // headroom: YugabyteDB bootstraps a cluster, not just a postmaster.
    let mut last = None;
    for _ in 0..120 {
        match Client::connect(&url, NoTls) {
            Ok(client) => {
                return Yb { client, _container: container };
            }
            Err(e) => last = Some(e),
        }
        std::thread::sleep(Duration::from_secs(1));
    }
    panic!("YugabyteDB never became ready: {last:?}");
}

/// Everything `pg_roundtrip.rs` asserts against PostgreSQL, asserted again against YugabyteDB.
///
/// One test rather than five: the image is ~1.6GB and takes about a minute to bootstrap, so the
/// container is started once and the assertions share it. The properties are the same ones.
#[test]
#[ignore = "pulls a ~1.6GB image and takes about a minute; run explicitly"]
fn money_survives_yugabytedb_exactly_as_it_survives_postgresql() {
    let mut yb = start();

    let version: String = yb.client.query_one("SELECT version()", &[]).expect("query ran").get(0);
    assert!(version.contains("-YB-"), "this must be YugabyteDB, not a stock postgres: {version}");

    yb.client
        .execute("CREATE TABLE ledger (id int PRIMARY KEY, amount text NOT NULL)", &[])
        .expect("table created");

    // --- the domain edges round-trip -----------------------------------------------------
    let values = [
        Money::<USD>::try_from_units(0).unwrap(),
        Money::<USD>::try_from_units(10_500_000_000_000_000_000).unwrap(),
        Money::<USD>::try_from_units(1).unwrap(),
        Money::<USD>::try_from_units(-1).unwrap(),
        Money::<USD>::try_from_units(DOMAIN_MAX).unwrap(),
        Money::<USD>::try_from_units(-DOMAIN_MAX).unwrap(),
    ];
    for (i, value) in values.iter().enumerate() {
        yb.client
            .execute("INSERT INTO ledger VALUES ($1, $2)", &[&i32::try_from(i).unwrap(), value])
            .expect("inserted");
    }
    let rows = yb.client.query("SELECT amount FROM ledger ORDER BY id", &[]).expect("selected");
    let back: Vec<Money<USD>> = rows.iter().map(|r| r.get(0)).collect();
    assert_eq!(back, values.to_vec(), "every value must survive the trip");

    // --- the stored bytes are the canonical form, per currency ---------------------------
    yb.client
        .execute("CREATE TABLE shapes (id int PRIMARY KEY, amount text NOT NULL)", &[])
        .expect("table created");
    let half = 10_500_000_000_000_000_000; // 10.5, whatever the currency
    yb.client
        .execute(
            "INSERT INTO shapes VALUES (1, $1), (2, $2), (3, $3)",
            &[
                &Money::<USD>::try_from_units(half).unwrap(),
                &Money::<JPY>::try_from_units(half).unwrap(),
                &Money::<KWD>::try_from_units(half).unwrap(),
            ],
        )
        .expect("inserted");
    let shapes: Vec<String> = yb
        .client
        .query("SELECT amount FROM shapes ORDER BY id", &[])
        .expect("selected")
        .iter()
        .map(|r| r.get(0))
        .collect();
    assert_eq!(
        shapes,
        vec!["USD 10.50", "JPY 10.5", "KWD 10.500"],
        "the settlement-exponent rule is the codec's, not the database's"
    );

    // --- the currency cross-check still fires --------------------------------------------
    let idr_row = yb
        .client
        .query_one("SELECT $1::text", &[&Money::<IDR>::try_from_major(16_000).unwrap()])
        .expect("query ran");
    assert!(idr_row.try_get::<_, Money<USD>>(0).is_err(), "IDR must not decode as USD, on YugabyteDB too");

    // --- rates, both ends of the pair ----------------------------------------------------
    let rate = Rate::<USD, IDR>::try_from_units(16_000 * POW10_SCALE).unwrap();
    let rate_row = yb.client.query_one("SELECT $1::text", &[&rate]).expect("query ran");
    assert_eq!(rate_row.get::<_, String>(0), "USD/IDR/16000");
    assert_eq!(rate_row.get::<_, Rate<USD, IDR>>(0), rate);
    assert!(rate_row.try_get::<_, Rate<IDR, USD>>(0).is_err(), "a reversed pair would invert the price");

    // --- numeric is refused here too ------------------------------------------------------
    yb.client
        .execute("CREATE TABLE wrong_type (id int PRIMARY KEY, amount numeric(36,18))", &[])
        .expect("table created");
    let refused = yb
        .client
        .execute("INSERT INTO wrong_type VALUES (1, $1)", &[&Money::<USD>::try_from_major(10).unwrap()]);
    assert!(refused.is_err(), "a numeric column must not accept Money");
}
