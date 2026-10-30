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

/// A data source holding one open limit order, reconciled from its private stream.
fn source_with_open_order() -> Harness {
    let mut h = Harness::keyed(
        Options {
            paradex_address: Some("0x0123".into()),
            subkey_address: Some("0x456".into()),
            ..Options::default()
        },
        STARK_KEY,
    );
    h.reply("/markets", MARKETS);
    h.reply("summary", TICKERS);
    h.reply(
        "/system/config",
        r#"{"starknet_chain_id":"PRIVATE_SN_PARACLEAR_MAINNET"}"#,
    );
    h.reply("/auth/", r#"{"jwt_token":"jwt"}"#);
    h.reply("/account/margin", r#"{"configs":[]}"#);
    h.reply("/positions", r#"{"results":[]}"#);
    h.reply(
        "/account",
        r#"{"total_collateral":"1000","free_collateral":"1000"}"#,
    );
    h.reply("/orders", r#"{"results":[]}"#);
    h.reply("/account/margin", r#"{"configs":[]}"#);
    let socket = h
        .actions
        .iter()
        .find_map(|a| match a {
            Action::WsConnect {
                socket,
                channel: Some(safe_cex_engine::SocketChannel::Private),
                ..
            } => Some(*socket),
            _ => None,
        })
        .unwrap();
    h.e.handle(h.now, Event::WsOpened { socket });
    h.collect();
    h.e.handle(
        h.now,
        Event::WsMessage {
            socket,
            text: r#"{"jsonrpc":"2.0","id":0,"result":{}}"#,
        },
    );
    h.collect();
    h.e.handle(h.now, Event::WsMessage { socket, text: r#"{"jsonrpc":"2.0","method":"subscription","params":{"channel":"orders.ALL","data":{"id":"123","market":"BTC-USD-PERP","type":"LIMIT","status":"OPEN","side":"BUY","size":"2","remaining_size":"2","price":"100","flags":[],"instruction":"GTC","created_at":1700000000000,"last_updated_at":1700000000000}}}"# });
    h.collect();
    assert!(h.e.store().order("123").is_some());
    h
}

#[test]
fn a_source_snapshot_with_venue_state_lets_an_amend_go_out_in_one_request() {
    let mut source = source_with_open_order();
    let frame = source.e.snapshot_frame(NOW).to_vec();
    let mut h = frame_fed();
    h.command(Command::AmendOrder {
        command: 2,
        id: "123".into(),
        price: Some(101.),
        amount: None,
    });
    assert!(
        h.result(2).is_err(),
        "no native row before the source's state"
    );
    assert!(h.pending.is_empty());

    h.e.apply_frame(NOW, &frame, true).unwrap();
    h.collect();
    assert!(h.e.store().order("123").is_some());
    h.command(Command::AmendOrder {
        command: 3,
        id: "123".into(),
        price: Some(101.),
        amount: None,
    });
    only(&h, "/orders/123");
    let request = h.take("/orders/123");
    assert_eq!(request.method, safe_cex_engine::Method::Put);
    h.respond(request, 200, r#"{"id":"123"}"#);
    assert_eq!(h.result(3), Ok("[\"123\"]"));
    assert!(
        h.pending.is_empty(),
        "no read after the amend: {:?}",
        h.pending
    );
    no_socket(&h);
}

const SYMBOL: &str = "BTC-USD-PERP";

fn leverage(h: &Harness) -> f64 {
    h.e.store().setting(SYMBOL).expect("a setting row").leverage
}

fn set_source_leverage(source: &mut Harness, value: f64) {
    source
        .e
        .store_mut()
        .update_setting(SYMBOL, |s| s.leverage = value);
}

fn apply_source(h: &mut Harness, source: &mut Harness) {
    let frame = source.e.snapshot_frame(NOW).to_vec();
    h.e.apply_frame(NOW, &frame, true).unwrap();
    h.collect();
}

/// A page fed a 10x source that set 20x itself.
fn page_set_to_20x(source: &mut Harness) -> Harness {
    set_source_leverage(source, 10.);
    let mut h = frame_fed();
    apply_source(&mut h, source);
    assert_eq!(leverage(&h), 10.);
    h.command(Command::SetLeverage {
        command: 9,
        symbol: SYMBOL.into(),
        leverage: 20.,
        side: None,
    });
    only(&h, "/account/margin/");
    let write = h.take("/account/margin/");
    assert_eq!(write.method, safe_cex_engine::Method::Post);
    h.respond(write, 200, r#"{"leverage":20,"margin_type":"CROSS"}"#);
    assert_eq!(leverage(&h), 20.);
    h
}

#[test]
fn a_stale_source_frame_keeps_the_new_leverage_and_an_order_is_still_one_request() {
    let mut source = source_with_open_order();
    let mut h = page_set_to_20x(&mut source);
    apply_source(&mut h, &mut source);
    assert_eq!(leverage(&h), 20., "held against the stale 10x frame");
    h.command(Command::PlaceOrders {
        command: 10,
        orders: vec![limit()],
    });
    only(&h, "/orders");
    assert_eq!(leverage(&h), 20.);
}

#[test]
fn a_source_frame_with_the_new_leverage_releases_the_hold() {
    let mut source = source_with_open_order();
    let mut h = page_set_to_20x(&mut source);
    set_source_leverage(&mut source, 20.);
    apply_source(&mut h, &mut source);
    assert_eq!(leverage(&h), 20.);
    // Released: the source's later value is taken again.
    set_source_leverage(&mut source, 5.);
    apply_source(&mut h, &mut source);
    assert_eq!(leverage(&h), 5.);
}

#[test]
fn refresh_settings_re_reads_the_margin_configs_and_writes_nothing() {
    use safe_cex_engine::VenueAction;
    let mut source = source_with_open_order();
    source.pending.clear();
    source.command(Command::Venue {
        command: 11,
        action: VenueAction::RefreshSettings {
            symbol: Some(SYMBOL.into()),
        },
    });
    only(&source, "/account/margin");
    assert_eq!(source.pending[0].method, safe_cex_engine::Method::Get);
    source.reply(
        "/account/margin",
        r#"{"configs":[{"market":"BTC-USD-PERP","leverage":7,"margin_type":"CROSS"}]}"#,
    );
    assert_eq!(leverage(&source), 7.);
    assert!(
        source
            .pending
            .iter()
            .all(|r| r.method == safe_cex_engine::Method::Get),
        "{:?}",
        source.pending
    );
}
