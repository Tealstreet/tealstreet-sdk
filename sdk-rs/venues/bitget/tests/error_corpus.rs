//! Recorded Bitget error replies (tests/fixtures/errors), each driven through the
//! production path that consumes it: the account reads after detection, an
//! order placement, public discovery, or the candles command.
mod support;
use safe_cex_engine::log::code;
use safe_cex_engine::test_support::error_corpus::{Fixture, Pinned, load};
use safe_cex_engine::{Command, ErrorKind, Outcome};
use serde_json::json;
use std::collections::BTreeMap;
use support::*;

fn corpus() -> BTreeMap<String, Fixture> {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/errors");
    load(&dir).into_iter().collect()
}

fn reply(h: &mut Harness, path: &str, fixture: &Fixture) {
    let request = h.take(path);
    h.reply(request.id, fixture.http_status.unwrap(), &fixture.body);
}

fn read_notice<'a>(h: &'a Harness, prefix_code: &str) -> &'a safe_cex_engine::log::LogEvent {
    h.events
        .iter()
        .find(|event| {
            (event.code == code::REQUEST_FAILED || event.code == code::AUTH_REJECTED)
                && event.fields.contains(&("venueCode", prefix_code.into()))
        })
        .unwrap_or_else(|| panic!("no read notice with {prefix_code}: {:?}", h.events))
}

#[test]
fn the_corpus_covers_every_recorded_case_with_its_production_path() {
    let names: Vec<_> = corpus().into_keys().collect();
    assert_eq!(
        names,
        [
            "accounts-bogus-key",
            "accounts-stale-timestamp",
            "contracts-invalid-product-type",
            "place-order-bogus-key",
            "positions-bogus-key",
            "spot-tickers-unknown-symbol",
            "usdc-accounts-bogus-key",
            "usdt-candles-invalid-granularity",
            "usdt-depth-unknown-symbol",
        ]
    );
    for (name, fixture) in corpus() {
        let expected = if name.starts_with("place") {
            Pinned::Write
        } else {
            Pinned::Read
        };
        assert_eq!(fixture.expect.path, expected, "{name}");
    }
}

/// Bitget checks the key before the clock: every keyed read answered 400 40037.
/// After detection, an account read reports it as a visible `auth.rejected`
/// Error with the table's `authInvalid` and the native code; a probe 40037 ends
/// startup instead.
#[test]
fn a_rejected_key_on_the_account_reads_is_classified_auth_invalid() {
    for (name, path) in [
        ("accounts-bogus-key", "/account/accounts"),
        ("accounts-stale-timestamp", "/account/accounts"),
        ("usdc-accounts-bogus-key", "/account/accounts"),
        ("positions-bogus-key", "/all-position"),
    ] {
        let fixture = &corpus()[name];
        assert!(fixture.body.contains(r#""code":"40037""#), "{name}");
        let mut h = Harness::private_started();
        h.respond(
            "/account/settings",
            200,
            r#"{"code":"40084","msg":"Classic Account mode","data":null}"#,
        );
        if path == "/all-position" {
            reply(&mut h, path, fixture);
        } else {
            h.respond("/all-position", 200, POSITION);
            reply(&mut h, path, fixture);
        }
        let event = read_notice(&h, "40037");
        assert_eq!(
            (event.code, event.level),
            (code::AUTH_REJECTED, safe_cex_engine::Level::Error),
            "{name}"
        );
        assert!(
            event.fields.contains(&("kind", "authInvalid".into())),
            "{name}: {:?}",
            event.fields
        );
        assert_eq!(fixture.expect.kind, Some(ErrorKind::AuthInvalid), "{name}");
    }
}

#[test]
fn a_rejected_key_on_an_order_is_a_rejected_auth_failure() {
    let fixture = &corpus()["place-order-bogus-key"];
    let mut h = Harness::classic();
    h.command(Command::PlaceOrders {
        command: 1,
        orders: vec![
            serde_json::from_value(
                json!({"symbol":"BTCUSDT","type":"limit","side":"buy","amount":0.0021,"price":90.04}),
            )
            .unwrap(),
        ],
    });
    reply(&mut h, "/batch-place-order", fixture);
    let [error] = std::mem::take(&mut h.failures).try_into().unwrap();
    assert_eq!(
        (error.kind, error.outcome, error.venue_code.as_deref()),
        (
            Some(ErrorKind::AuthInvalid),
            Some(Outcome::Rejected),
            Some("40037")
        )
    );
}

/// The candles command fails with the table's kind: 400171 (the six-digit
/// parameter-verification code) is `invalidInput`, carried with its code.
#[test]
fn an_invalid_granularity_fails_the_candles_command_as_invalid_input() {
    let fixture = &corpus()["usdt-candles-invalid-granularity"];
    let mut h = Harness::public();
    h.command(Command::FetchCandles {
        command: 4,
        symbol: "BTCUSDT".into(),
        interval: safe_cex_engine::model::Interval::parse("1m").unwrap(),
        limit: Some(10),
        from_ms: None,
        to_ms: None,
    });
    reply(&mut h, "/candles", fixture);
    let [error] = std::mem::take(&mut h.failures).try_into().unwrap();
    assert_eq!(
        (error.kind, error.venue_code.as_deref()),
        (Some(ErrorKind::InvalidInput), Some("400171"))
    );
    assert!(
        error.message.starts_with("Bitget 400171: "),
        "the text is unchanged"
    );
}

/// Public discovery keeps the engine's shared startup retry (not the table):
/// a recorded 40034 refusal is a retried startup read, with no publication.
#[test]
fn discovery_refusals_retry_public_startup() {
    for name in [
        "contracts-invalid-product-type",
        "usdt-depth-unknown-symbol",
        "spot-tickers-unknown-symbol",
    ] {
        let fixture = &corpus()[name];
        assert!(fixture.body.contains(r#""code":"40034""#), "{name}");
        let mut h = Harness::started(FixtureOptions {
            public_data_only: true,
            ..FixtureOptions::default()
        });
        reply(&mut h, "/contracts", fixture);
        assert!(h.failures.is_empty(), "{name}");
        assert!(
            h.events.iter().any(|e| e.code == code::REQUEST_FAILED
                && e.message == "Startup read failed; retry scheduled"),
            "{name}: {:?}",
            h.events
        );
        assert!(!h.timers.is_empty(), "{name}: a retry is armed");
        assert!(h.engine.store().markets().is_empty(), "{name}");
    }
}
