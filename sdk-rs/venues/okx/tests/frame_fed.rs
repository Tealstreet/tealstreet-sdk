#![allow(dead_code)]
//! A frame-fed OKX engine (the browser's main-thread engine for one account):
//! public instrument metadata only, no sockets, no account reads or polls, and
//! each write emits exactly its planned venue requests and never a read.
mod support;
use safe_cex_engine::frame_fed::{DATA_SOURCE_ONLY, MARKETS_NOT_LOADED};
use safe_cex_engine::model::{Order, OrderSide, OrderStatus, OrderType, PlaceOrderOpts};
use safe_cex_engine::{Action, Command, Engine, Event, Method, Notice};
use safe_cex_venue_okx::Okx;
use support::{FixtureOptions, MARKET, envelope, prepared, reply};

fn frame_fed() -> Engine<Okx> {
    let mut engine = support::engine(FixtureOptions {
        key: Some("test-key".into()),
        secret: Some("test-secret".into()),
        passphrase: Some("test-passphrase".into()),
        ..FixtureOptions::default()
    });
    engine.set_frame_fed().unwrap();
    engine.start(0.0);
    engine
}

fn http(actions: &[Action]) -> Vec<&Action> {
    actions
        .iter()
        .filter(|action| matches!(action, Action::Http(_)))
        .collect()
}

fn url(action: &Action) -> &str {
    let Action::Http(request) = action else {
        panic!("not a request: {action:?}")
    };
    &request.url
}

fn method(action: &Action) -> Method {
    let Action::Http(request) = action else {
        panic!("not a request: {action:?}")
    };
    request.method
}

/// Start, answer the one public instrument read, and assert nothing follows.
fn loaded() -> Engine<Okx> {
    let mut engine = frame_fed();
    let start = prepared(&mut engine, 0.0);
    let requests = http(&start);
    assert_eq!(requests.len(), 1, "{requests:?}");
    assert!(
        url(requests[0]).contains("/api/v5/public/instruments"),
        "{requests:?}"
    );
    assert!(
        !start.iter().any(|a| matches!(a, Action::WsConnect { .. })),
        "no socket"
    );
    reply(&mut engine, requests[0], &envelope(MARKET), 1.0);
    let after = prepared(&mut engine, 1.0);
    assert!(http(&after).is_empty(), "nothing after metadata: {after:?}");
    fire_timers(&mut engine, &start, 1.0);
    fire_timers(&mut engine, &after, 1.0);
    assert!(engine.store().loaded().markets);
    engine
}

/// The store as ingested data-source frames leave it, then the frame hook. A
/// delta frame keeps the seeded store (the section decoders arrive separately).
fn ingested(engine: &mut Engine<Okx>, orders: Vec<Order>, hedged: bool) {
    let store = engine.store_mut();
    store.set_hedged(hedged);
    store.set_orders(orders);
    store.set_positions(vec![]);
    let frame = engine.snapshot_frame(2.0).to_vec();
    engine.apply_frame(2.0, &frame, false).unwrap();
    let _ = prepared(engine, 2.0);
}

fn stored(id: &str, kind: OrderType) -> Order {
    Order {
        id: id.into(),
        parent_id: None,
        status: OrderStatus::Open,
        symbol: "BTCUSDT".into(),
        kind,
        side: OrderSide::Sell,
        price: 41000.,
        amount: 1.,
        filled: 0.,
        remaining: 1.,
        reduce_only: false,
        close: None,
        client_order_id: None,
        time_in_force: None,
        last_update: 0.,
        trigger_type: None,
        take_profit: None,
        stop_loss: None,
        extra: None,
    }
}

fn order() -> PlaceOrderOpts {
    serde_json::from_str(
        r#"{"symbol":"BTCUSDT","type":"limit","side":"buy","amount":1,"price":39000}"#,
    )
    .unwrap()
}

fn command(engine: &mut Engine<Okx>, command: Command, now: f64) -> Vec<Action> {
    engine.handle(now, Event::Command(command));
    prepared(engine, now)
}

fn failure(actions: &[Action], id: u32) -> Option<String> {
    actions.iter().find_map(|action| match action {
        Action::Notice(Notice::CommandFailed { command, error }) if *command == id => {
            Some(error.message.clone())
        }
        _ => None,
    })
}

/// One write: exactly one venue POST to `path`, no read, and nothing after
/// its reply (no reconcile, settings or account read).
fn exactly_one_write(
    engine: &mut Engine<Okx>,
    actions: &[Action],
    path: &str,
    body: &str,
    now: f64,
) {
    let requests = http(actions);
    assert_eq!(requests.len(), 1, "one venue request: {requests:?}");
    assert_eq!(method(requests[0]), Method::Post, "{requests:?}");
    assert!(url(requests[0]).ends_with(path), "{requests:?}");
    reply(engine, requests[0], body, now);
    let after = prepared(engine, now);
    assert!(http(&after).is_empty(), "no follow-up read: {after:?}");
    fire_timers(engine, &after, now);
}

/// Fires every timer `actions` armed; none may lead to a venue request.
fn fire_timers(engine: &mut Engine<Okx>, actions: &[Action], now: f64) {
    for action in actions {
        if let Action::SetTimer { timer, at_ms } = action {
            engine.handle(at_ms.max(now), Event::TimerFired { timer: *timer });
            let fired = prepared(engine, at_ms.max(now));
            assert!(
                http(&fired).is_empty(),
                "a timer led to a request: {fired:?}"
            );
        }
    }
}

#[test]
fn a_frame_fed_engine_loads_public_metadata_once_and_opens_no_socket() {
    let mut engine = loaded();
    let later = prepared(&mut engine, 120_000.0);
    assert!(http(&later).is_empty(), "no polls: {later:?}");
}

#[test]
fn writes_wait_for_markets_then_for_the_account_frames_and_non_writes_are_refused() {
    let mut engine = frame_fed();
    let early = command(
        &mut engine,
        Command::PlaceOrders {
            command: 1,
            orders: vec![order()],
        },
        0.5,
    );
    assert!(
        failure(&early, 1).is_some_and(|m| m.contains(MARKETS_NOT_LOADED)),
        "{early:?}"
    );
    let mut engine = loaded();
    let before = command(
        &mut engine,
        Command::PlaceOrders {
            command: 2,
            orders: vec![order()],
        },
        1.5,
    );
    assert_eq!(
        failure(&before, 2).as_deref(),
        Some("OKX account is not ready"),
        "{before:?}"
    );
    assert!(http(&before).is_empty());
    let read = command(
        &mut engine,
        Command::FetchLeverage {
            command: 3,
            symbol: "BTCUSDT".into(),
        },
        1.6,
    );
    assert!(
        failure(&read, 3).is_some_and(|m| m.contains(DATA_SOURCE_ONLY)),
        "{read:?}"
    );
    assert!(http(&read).is_empty());
}

#[test]
fn a_placement_is_one_request_and_reads_the_ingested_position_mode() {
    let mut engine = loaded();
    ingested(&mut engine, vec![], true);
    let actions = command(
        &mut engine,
        Command::PlaceOrders {
            command: 4,
            orders: vec![order()],
        },
        3.0,
    );
    let Action::Http(request) = http(&actions)[0] else {
        unreachable!()
    };
    let body = request.body.clone().unwrap_or_default();
    assert!(body.contains(r#""instId":"BTC-USDT-SWAP""#), "{body}");
    assert!(
        body.contains(r#""posSide":"long""#),
        "hedged comes from the ingested STATUS: {body}"
    );
    exactly_one_write(
        &mut engine,
        &actions,
        "/api/v5/trade/batch-orders",
        &envelope(r#"{"ordId":"9001","clOrdId":"","sCode":"0","sMsg":""}"#),
        3.5,
    );
}

#[test]
fn cancel_amend_and_leverage_are_each_one_request_from_the_ingested_store() {
    let mut engine = loaded();
    ingested(
        &mut engine,
        vec![
            stored("101", OrderType::Limit),
            stored("102", OrderType::Limit),
        ],
        false,
    );
    let cancel = command(
        &mut engine,
        Command::CancelOrders {
            command: 5,
            ids: vec!["101".into()],
        },
        3.0,
    );
    exactly_one_write(
        &mut engine,
        &cancel,
        "/api/v5/trade/cancel-batch-orders",
        &envelope(r#"{"ordId":"101","clOrdId":"","sCode":"0","sMsg":""}"#),
        3.1,
    );
    let amend = command(
        &mut engine,
        Command::AmendOrder {
            command: 6,
            id: "102".into(),
            price: Some(41500.),
            amount: None,
        },
        4.0,
    );
    exactly_one_write(
        &mut engine,
        &amend,
        "/api/v5/trade/amend-order",
        &envelope(r#"{"ordId":"102","clOrdId":"","reqId":"","sCode":"0","sMsg":""}"#),
        4.1,
    );
    let leverage = command(
        &mut engine,
        Command::SetLeverage {
            command: 7,
            symbol: "BTCUSDT".into(),
            leverage: 10.,
            side: None,
        },
        5.0,
    );
    exactly_one_write(
        &mut engine,
        &leverage,
        "/api/v5/account/set-leverage",
        &envelope(r#"{"instId":"BTC-USDT-SWAP","lever":"10","mgnMode":"cross","posSide":""}"#),
        5.1,
    );
}

#[test]
fn an_unknown_placement_outcome_is_never_re_sent_or_followed_by_a_read() {
    let mut engine = loaded();
    ingested(&mut engine, vec![], false);
    let actions = command(
        &mut engine,
        Command::PlaceOrders {
            command: 8,
            orders: vec![order()],
        },
        3.0,
    );
    let requests = http(&actions);
    assert_eq!(requests.len(), 1, "{requests:?}");
    reply(
        &mut engine,
        requests[0],
        "<html>gateway timeout</html>",
        3.5,
    );
    let after = prepared(&mut engine, 3.5);
    assert!(
        http(&after).is_empty(),
        "no re-send and no reconcile read: {after:?}"
    );
    fire_timers(&mut engine, &after, 3.5);
}
