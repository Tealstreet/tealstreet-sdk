//! Recorded Bybit error replies (tests/fixtures/errors), each driven through the
//! production path that consumes it: the account snapshot reads, the market
//! refresh command, or an order placement.
mod support;

use safe_cex_engine::log::code;
use safe_cex_engine::test_support::error_corpus::{Fixture, Pinned, load};
use safe_cex_engine::{Command, ErrorKind, Event, Notice, Outcome, VenueAction, VenueError};
use safe_cex_venue_bybit::Variant;
use std::collections::BTreeMap;
use support::{Harness, Options};

const EMPTY: &str = r#"{"retCode":0,"retMsg":"OK","result":{"list":[],"nextPageCursor":""}}"#;

fn corpus() -> BTreeMap<String, Fixture> {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/errors");
    load(&dir).into_iter().collect()
}

fn reply(h: &mut Harness, path: &str, fixture: &Fixture) {
    let request = h.take(path);
    h.reply(request.id, fixture.http_status.unwrap(), &fixture.body);
}

/// A keyed account through startup and private auth, with its account
/// snapshot reads still outstanding.
fn awaiting_snapshot(variant: Variant) -> Harness {
    let mut h = Harness::started(Options {
        variant,
        key: Some("key".into()),
        secret: Some("secret".into()),
        ..Options::default()
    });
    if h.has_request("/v5/account/info") {
        h.respond("/v5/account/info", 200, support::ACCOUNT_INFO);
    }
    if h.has_request("/v5/account/fee-rate") {
        h.respond("/v5/account/fee-rate", 200, EMPTY);
    }
    if h.has_request("status=PreLaunch") {
        h.respond("status=PreLaunch", 200, EMPTY);
    }
    h.respond("/v5/market/instruments-info", 200, support::INSTRUMENTS);
    h.respond("/v5/market/tickers", 200, support::TICKERS);
    let public: Vec<_> = h
        .sockets
        .iter()
        .filter(|(_, url)| url.contains("/v5/public"))
        .map(|(socket, _)| *socket)
        .collect();
    for socket in public {
        h.event(Event::WsOpened { socket });
    }
    let socket = h.socket("/v5/private").unwrap();
    h.event(Event::WsOpened { socket });
    h.message(
        "/v5/private",
        r#"{"success":true,"ret_msg":"","op":"auth","conn_id":"corpus"}"#,
    );
    h
}

/// The snapshot read's notice: still `request.failed`, now carrying the kind.
fn read_notice(h: &Harness, prefix: &str, kind: ErrorKind) {
    let event = h
        .notices
        .iter()
        .find_map(|notice| match notice {
            Notice::Event(event) if event.message.starts_with(prefix) => Some(event),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no {prefix} notice: {:?}", h.notices));
    // A read refused for the key is a visible auth.rejected Error.
    let auth = matches!(
        kind,
        ErrorKind::AuthInvalid | ErrorKind::AuthExpired | ErrorKind::AuthPermission
    );
    assert_eq!(
        event.code,
        if auth {
            code::AUTH_REJECTED
        } else {
            code::REQUEST_FAILED
        },
        "{prefix}"
    );
    assert_eq!(event.level, safe_cex_engine::Level::Error, "{prefix}");
    assert!(
        event.fields.contains(&("kind", kind.as_str().into())),
        "{:?}",
        event.fields
    );
}

fn command_failure(h: &Harness, command: u32) -> &VenueError {
    h.notices
        .iter()
        .find_map(|notice| match notice {
            Notice::CommandFailed { command: id, error } if *id == command => Some(error),
            _ => None,
        })
        .unwrap_or_else(|| panic!("command {command} did not fail: {:?}", h.notices))
}

#[test]
fn the_corpus_covers_every_recorded_case_with_its_production_path() {
    let names: Vec<_> = corpus().into_keys().collect();
    assert_eq!(
        names,
        [
            "demo-wallet-balance-bogus-key",
            "instruments-invalid-category",
            "inverse-orderbook-invalid-symbol",
            "linear-orderbook-invalid-symbol",
            "order-create-bogus-key-fresh",
            "order-create-stale-timestamp",
            "position-list-bogus-key",
            "spot-tickers-invalid-symbol",
            "wallet-balance-bogus-key",
            "wallet-balance-stale-timestamp",
        ]
    );
    for (name, fixture) in corpus() {
        let expected = if name.starts_with("order") {
            Pinned::Write
        } else {
            Pinned::Read
        };
        assert_eq!(fixture.expect.path, expected, "{name}");
    }
}

/// Bybit answers a rejected key on a GET read with HTTP 401 and an empty body:
/// no retCode, so the status decides `authInvalid` and no venue code is invented.
#[test]
fn a_rejected_key_on_the_snapshot_reads_is_classified_auth_invalid() {
    for (name, variant, path, prefix) in [
        (
            "wallet-balance-bogus-key",
            Variant::Linear,
            "/v5/account/wallet-balance",
            "Bybit balance: ",
        ),
        (
            "wallet-balance-stale-timestamp",
            Variant::Linear,
            "/v5/account/wallet-balance",
            "Bybit balance: ",
        ),
        (
            "demo-wallet-balance-bogus-key",
            Variant::Demo,
            "/v5/account/wallet-balance",
            "Bybit balance: ",
        ),
        (
            "position-list-bogus-key",
            Variant::Linear,
            "/v5/position/list?category=linear&settleCoin=USDT",
            "Bybit positions: ",
        ),
    ] {
        let fixture = &corpus()[name];
        assert_eq!(
            (fixture.http_status, fixture.body.as_str()),
            (Some(401), ""),
            "{name}"
        );
        let mut h = awaiting_snapshot(variant);
        reply(&mut h, path, fixture);
        read_notice(&h, prefix, fixture.expect.kind.unwrap());
        let event = h
            .notices
            .iter()
            .find_map(|notice| match notice {
                Notice::Event(event) if event.message.starts_with(prefix) => Some(event),
                _ => None,
            })
            .unwrap();
        assert!(
            !event.fields.iter().any(|(name, _)| *name == "venueCode"),
            "{name}"
        );
    }
}

/// Every discovery-shaped 10001 refusal fails an explicit market refresh as
/// `invalidInput` with its retCode. Bybit reads no REST order book, so the two
/// orderbook bodies replay on the instruments page, which shares the envelope.
#[test]
fn a_10001_refusal_fails_the_market_refresh_as_invalid_input() {
    for (command, name) in [
        (70, "instruments-invalid-category"),
        (71, "linear-orderbook-invalid-symbol"),
        (72, "inverse-orderbook-invalid-symbol"),
        (73, "spot-tickers-invalid-symbol"),
    ] {
        let fixture = &corpus()[name];
        let mut h = Harness::new();
        h.command(Command::Venue {
            command,
            action: VenueAction::RefreshMarkets,
        });
        let _prelaunch = h.take("status=PreLaunch");
        reply(&mut h, "instruments-info", fixture);
        let error = command_failure(&h, command);
        assert_eq!(
            (error.kind, error.venue_code.as_deref()),
            (fixture.expect.kind, Some("10001")),
            "{name}"
        );
    }
}

fn placed(h: &mut Harness, command: u32) {
    h.command(Command::PlaceOrders {
        command,
        orders: vec![support::order()],
    });
    h.respond(
        "/v5/position/list",
        200,
        r#"{"retCode":0,"result":{"list":[{"symbol":"BTCUSDT","positionIdx":0}]}}"#,
    );
}

#[test]
fn a_rejected_key_on_an_order_is_a_rejected_auth_failure() {
    let fixture = &corpus()["order-create-bogus-key-fresh"];
    let mut h = awaiting_snapshot(Variant::Linear);
    h.requests
        .retain(|request| request.url.contains("/v5/order/create"));
    placed(&mut h, 7);
    reply(&mut h, "/v5/order/create", fixture);
    let error = command_failure(&h, 7);
    assert_eq!(
        (error.kind, error.outcome, error.venue_code.as_deref()),
        (
            Some(ErrorKind::AuthInvalid),
            Some(Outcome::Rejected),
            Some("10003")
        )
    );
}

/// 10002 (recv_window) permits one re-signed resend; a second 10002 refuses.
#[test]
fn a_clock_refused_order_resends_once_then_fails_auth_invalid() {
    let fixture = &corpus()["order-create-stale-timestamp"];
    assert!(fixture.body.contains("\"retCode\":10002"));
    let mut h = awaiting_snapshot(Variant::Linear);
    h.requests
        .retain(|request| request.url.contains("/v5/order/create"));
    placed(&mut h, 8);
    let first = h.take("/v5/order/create");
    h.reply(first.id, fixture.http_status.unwrap(), &fixture.body);
    // 10002 is refused at the gateway before any order processing (Bybit: "The
    // request time exceeds the time window range"), so the one re-signed resend
    // repeats an order the venue never accepted: the same body, new signature.
    let resend = h.take("/v5/order/create");
    assert_eq!(resend.body, first.body, "only the signature headers change");
    h.reply(resend.id, fixture.http_status.unwrap(), &fixture.body);
    assert!(!h.has_request("/v5/order/create"), "only one resend");
    let error = command_failure(&h, 8);
    assert_eq!(
        (error.kind, error.outcome, error.venue_code.as_deref()),
        (
            Some(ErrorKind::AuthInvalid),
            Some(Outcome::Rejected),
            Some("10002")
        )
    );
}

/// HTTP 403 is Bybit's IP ban: `Unknown(RateLimited)`, which never earns a
/// resend (only `RefusedResend` does, ordinary.rs Decision::Resend, and
/// assert_table refuses a resendable status row). One dispatch, outcome unknown.
#[test]
fn a_403_on_an_order_is_dispatched_once_and_left_unknown() {
    let mut h = awaiting_snapshot(Variant::Linear);
    h.requests
        .retain(|request| request.url.contains("/v5/order/create"));
    placed(&mut h, 9);
    let create = h.take("/v5/order/create");
    h.reply(create.id, 403, "<html>403 Forbidden</html>");
    let error = command_failure(&h, 9).clone();
    assert_eq!(
        (error.kind, error.outcome, error.venue_code.as_deref()),
        (Some(ErrorKind::RateLimited), Some(Outcome::Unknown), None)
    );
    h.advance(120_000.0);
    assert!(
        !h.has_request("/v5/order/create"),
        "a 403 write is never re-dispatched"
    );
}
