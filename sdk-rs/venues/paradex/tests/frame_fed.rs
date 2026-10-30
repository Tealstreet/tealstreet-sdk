//! A frame-fed Paradex engine: the public catalogue, the public system config
//! and a held JWT, and nothing else. No socket, no account read, no poll; each
//! write goes out in exactly its planned requests and is never re-sent.
mod support;
use safe_cex_engine::model::{OrderType, PlaceOrderOpts};
use safe_cex_engine::{Action, Command, Event, SocketId};
use safe_cex_venue_paradex::Options;
use support::*;

const NOW: f64 = 1_700_000_000_000.;

fn frame_fed_unloaded() -> Harness {
    let mut e = engine(
        Options {
            paradex_address: Some("0x0123".into()),
            subkey_address: Some("0x456".into()),
            ..Options::default()
        },
        Some(STARK_KEY),
    );
    e.set_frame_fed().unwrap();
    e.start(NOW);
    let mut h = Harness {
        e,
        pending: vec![],
        actions: vec![],
        failures: vec![],
        receipt_trace: vec![],
        public: SocketId(0),
        now: NOW,
    };
    h.collect();
    h
}

fn only(h: &Harness, path: &str) {
    assert_eq!(h.pending.len(), 1, "{:?}", h.pending);
    assert!(h.pending[0].url.contains(path), "{}", h.pending[0].url);
}

fn no_socket(h: &Harness) {
    assert!(
        !h.actions
            .iter()
            .any(|a| matches!(a, Action::WsConnect { .. })),
        "no socket"
    );
}

/// Loaded and holding a JWT: markets, config, auth, and then nothing.
fn frame_fed() -> Harness {
    let mut h = frame_fed_unloaded();
    only(&h, "/markets");
    h.reply("/markets", MARKETS);
    only(&h, "/system/config");
    h.reply(
        "/system/config",
        r#"{"starknet_chain_id":"PRIVATE_SN_PARACLEAR_MAINNET"}"#,
    );
    only(&h, "/auth/");
    h.reply("/auth/", r#"{"jwt_token":"jwt"}"#);
    assert!(h.pending.is_empty(), "no account read: {:?}", h.pending);
    no_socket(&h);
    assert!(h.e.store().loaded().markets);
    h
}

fn limit() -> PlaceOrderOpts {
    serde_json::from_str(
        r#"{"symbol":"BTC-USD-PERP","type":"limit","side":"buy","amount":1,"price":100,"clientOrderId":"caller"}"#,
    )
    .unwrap()
}

#[test]
fn a_frame_fed_engine_loads_the_catalogue_config_and_a_jwt_only() {
    let mut h = frame_fed();
    h.command(Command::FetchAccount { command: 1 });
    assert!(h.result(1).is_err());
    assert!(h.pending.is_empty());
}

#[test]
fn a_limit_order_is_one_request_and_reads_nothing_after() {
    let mut h = frame_fed();
    h.command(Command::PlaceOrders {
        command: 1,
        orders: vec![limit()],
    });
    only(&h, "/orders/batch");
    let request = h.take("/orders/batch");
    assert!(
        request
            .headers
            .iter()
            .any(|(name, value)| *name == "Authorization" && value == "Bearer jwt")
    );
    h.respond(
        request,
        200,
        r#"{"orders":[{"id":"venue-1","client_id":"caller"}]}"#,
    );
    assert_eq!(h.result(1), Ok("[\"venue-1\"]"));
    assert!(
        h.pending.is_empty(),
        "no read after the write: {:?}",
        h.pending
    );
    no_socket(&h);
}

#[test]
fn the_held_jwt_is_renewed_before_it_expires() {
    let mut h = frame_fed();
    // Issued at NOW, valid for 180 s, renewed 15 s early.
    h.timer(NOW + 165_000.);
    only(&h, "/auth/");
    h.reply("/auth/", r#"{"jwt_token":"fresh"}"#);
    assert!(h.pending.is_empty());
    h.command(Command::PlaceOrders {
        command: 1,
        orders: vec![limit()],
    });
    let request = h.take("/orders");
    assert!(
        request
            .headers
            .iter()
            .any(|(name, value)| *name == "Authorization" && value == "Bearer fresh")
    );
}

#[test]
fn an_unknown_placement_is_never_read_or_re_sent() {
    let mut h = frame_fed();
    h.command(Command::PlaceOrders {
        command: 1,
        orders: vec![limit()],
    });
    let request = h.take("/orders");
    h.e.handle(
        h.now,
        Event::HttpFailed {
            request: request.id,
            error: "connection reset",
        },
    );
    h.collect();
    assert!(h.result(1).is_err());
    h.now += 300_000.;
    h.collect();
    assert!(
        h.pending.iter().all(|r| r.url.contains("/auth/")),
        "{:?}",
        h.pending
    );
}

#[test]
fn a_write_before_the_catalogue_sends_nothing() {
    let mut h = frame_fed_unloaded();
    h.command(Command::PlaceOrders {
        command: 1,
        orders: vec![PlaceOrderOpts {
            kind: OrderType::Limit,
            ..limit()
        }],
    });
    assert!(h.result(1).is_err());
    only(&h, "/markets");
}
