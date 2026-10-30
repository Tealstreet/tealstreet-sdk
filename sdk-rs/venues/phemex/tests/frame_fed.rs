//! A frame-fed Phemex engine: public instrument metadata only, its account
//! state (settings contexts in VENUE_STATE) from a data source's frames, and
//! each write sent in exactly its planned requests, with no read after it.
mod support;
use safe_cex_engine::model::*;
use safe_cex_engine::{Action, Command, Event, HttpRequest, Notice, Response};
use safe_cex_venue_phemex::PhemexPerpetuals;
use serde_json::{Value, json};
use support::{Options, PreparedEngine as Engine};

const PRODUCTS: &str = r#"{"code":0,"data":{"perpProductsV2":[{"symbol":"BTCUSDT","type":"PerpetualV2","settleCurrency":"USDT","baseCurrency":"BTC","quoteCurrency":"USDT","qtyStepSize":"0.001","tickSize":"0.1","maxOrderQtyRq":"1","maxLeverage":"100"}]}}"#;
const DAILY: &str = r#"{"error":null,"result":[{"symbol":"BTCUSDT","bidRp":"69000","askRp":"69001","lastRp":"69000","markRp":"69000","indexRp":"69000","openRp":"69000","openInterestRv":"250","fundingRateRr":"0.0001","volumeRq":"10"}]}"#;

fn requests(actions: &[Action]) -> Vec<HttpRequest> {
    actions
        .iter()
        .filter_map(|a| match a {
            Action::Http(r) => Some(r.clone()),
            _ => None,
        })
        .collect()
}

fn opens_socket(actions: &[Action]) -> bool {
    actions
        .iter()
        .any(|a| matches!(a, Action::WsConnect { .. }))
}

fn respond(e: &mut Engine<PhemexPerpetuals>, r: &HttpRequest, body: &str) -> Vec<Action> {
    e.handle(
        1001.,
        Event::HttpResponse {
            request: r.id,
            response: Response {
                status: 200,
                headers: "",
                body,
            },
        },
    );
    e.drain_actions().collect()
}

fn command(e: &mut Engine<PhemexPerpetuals>, c: Command) -> Vec<Action> {
    e.handle(1000., Event::Command(c));
    e.drain_actions().collect()
}

fn keyed() -> Options {
    Options {
        key: Some("key".into()),
        secret: Some("secret".into()),
        ..Options::default()
    }
}

fn order(id: &str) -> PlaceOrderOpts {
    PlaceOrderOpts {
        symbol: "BTCUSDT".into(),
        kind: OrderType::Limit,
        side: OrderSide::Buy,
        amount: 0.01,
        price: Some(69000.),
        trigger_price: None,
        max_slippage_pct: None,
        stop_loss: None,
        take_profit: None,
        reduce_only: None,
        time_in_force: None,
        trigger_type: None,
        tp_trigger_type: None,
        sl_trigger_type: None,
        client_order_id: Some(id.into()),
        close: None,
        stp_mode: None,
        bybit_bbo: None,
        bybit_slippage_tolerance_ticks: None,
        implied_volatility: None,
    }
}

fn param(r: &HttpRequest, key: &str) -> Option<String> {
    r.url
        .split_once('?')?
        .1
        .split('&')
        .find_map(|pair| pair.strip_prefix(&format!("{key}=")).map(str::to_owned))
}

fn ack(r: &HttpRequest, id: &str) -> Value {
    json!({"code":0,"data":{"orderID":id,"clOrdID":param(r,"clOrdID"),"symbol":"BTCUSDT","side":param(r,"side"),"posSide":param(r,"posSide"),"orderType":param(r,"ordType"),"orderQtyRq":param(r,"orderQtyRq"),"priceRp":param(r,"priceRp"),"ordStatus":"New","execStatus":"New","timeInForce":"GoodTillCancel"}})
}

/// A frame-fed engine with its public catalogue loaded.
fn frame_fed() -> Engine<PhemexPerpetuals> {
    let mut e = support::prepared_engine(keyed());
    e.set_frame_fed().unwrap();
    e.start(0.);
    let actions: Vec<_> = e.drain_actions().collect();
    let first = requests(&actions);
    assert_eq!(first.len(), 1, "{first:?}");
    assert!(
        first[0].url.contains("/public/products"),
        "{}",
        first[0].url
    );
    assert!(!opens_socket(&actions));
    let after = respond(&mut e, &first[0], PRODUCTS);
    assert!(
        requests(&after).is_empty(),
        "nothing after metadata: {:?}",
        requests(&after)
    );
    assert!(!opens_socket(&after));
    assert!(e.store().loaded().markets);
    e
}

/// A normal (data-source) engine's snapshot once its account read says BTCUSDT is hedged.
fn source_snapshot() -> Vec<u8> {
    source("Hedged").snapshot_frame(1001.).to_vec()
}

/// A normal (data-source) engine whose account read gives BTCUSDT `mode`.
fn source(mode: &str) -> Engine<PhemexPerpetuals> {
    source_with(vec![
        json!({"symbol":"BTCUSDT","currency":"USDT","posMode":mode,"posSide":if mode == "Hedged" { "Long" } else { "Merged" },"size":"0","leverageRr":"-10","crossMargin":true}),
    ])
}

/// A normal (data-source) engine whose USDT account read returns `rows`.
fn source_with(rows: Vec<Value>) -> Engine<PhemexPerpetuals> {
    let mut e = support::prepared_engine(keyed());
    e.start(0.);
    let a: Vec<_> = e.drain_actions().collect();
    let mut a = respond(&mut e, &requests(&a)[0], PRODUCTS);
    let ticker = requests(&a)
        .into_iter()
        .find(|r| r.url.contains("24hr/all"))
        .unwrap();
    a.extend(respond(&mut e, &ticker, DAILY));
    for r in requests(&a)
        .into_iter()
        .filter(|r| !r.url.contains("24hr/all"))
    {
        let currency = if r.url.contains("USDC") {
            "USDC"
        } else {
            "USDT"
        };
        let rows = if currency == "USDT" {
            rows.clone()
        } else {
            vec![]
        };
        respond(
            &mut e,
            &r,
            &json!({"code":0,"data":{"account":{"currency":currency,"accountBalanceRv":"1000","totalUsedBalanceRv":"0"},"positions":rows}}).to_string(),
        );
    }
    e.drain_actions().for_each(drop);
    e
}

#[test]
fn a_frame_fed_engine_loads_public_metadata_once_and_refuses_reads() {
    let mut e = frame_fed();
    let a = command(&mut e, Command::FetchAccount { command: 1 });
    assert!(requests(&a).is_empty());
    assert!(
        a.iter()
            .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { .. })))
    );
}

#[test]
fn an_ingested_hedge_mode_places_in_exactly_one_request_and_reads_nothing_after() {
    let mut e = frame_fed();
    e.apply_frame(1001., &source_snapshot(), true).unwrap();
    e.drain_actions().for_each(drop);
    let a = command(
        &mut e,
        Command::PlaceOrders {
            command: 1,
            orders: vec![order("caller-1")],
        },
    );
    let sent = requests(&a);
    assert_eq!(sent.len(), 1, "{sent:?}");
    assert!(sent[0].url.contains("/g-orders/create"), "{}", sent[0].url);
    assert_eq!(
        param(&sent[0], "posSide").as_deref(),
        Some("Long"),
        "hedged buy opens long"
    );
    let a = respond(&mut e, &sent[0], &ack(&sent[0], "real-1").to_string());
    assert!(
        a.iter()
            .any(|a| matches!(a, Action::Notice(Notice::CommandDone { .. }))),
        "{a:?}"
    );
    assert!(
        requests(&a).is_empty(),
        "no read after the write: {:?}",
        requests(&a)
    );
}

#[test]
fn a_write_before_the_source_snapshot_has_no_settings_and_sends_nothing() {
    let mut e = frame_fed();
    let a = command(
        &mut e,
        Command::PlaceOrders {
            command: 1,
            orders: vec![order("caller-1")],
        },
    );
    assert!(requests(&a).is_empty());
    assert!(
        a.iter()
            .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { .. })))
    );
}

#[test]
fn an_unknown_placement_is_never_re_sent_or_read() {
    let mut e = frame_fed();
    e.apply_frame(1001., &source_snapshot(), true).unwrap();
    e.drain_actions().for_each(drop);
    let a = command(
        &mut e,
        Command::PlaceOrders {
            command: 1,
            orders: vec![order("caller-1")],
        },
    );
    let sent = requests(&a);
    e.handle(
        1002.,
        Event::HttpFailed {
            request: sent[0].id,
            error: "connection reset",
        },
    );
    let a: Vec<_> = e.drain_actions().collect();
    assert!(
        requests(&a).is_empty(),
        "no reconcile read: {:?}",
        requests(&a)
    );
    // The same caller id is refused while its first placement's outcome is unknown.
    let a = command(
        &mut e,
        Command::PlaceOrders {
            command: 2,
            orders: vec![order("caller-1")],
        },
    );
    assert!(requests(&a).is_empty(), "never re-sent: {:?}", requests(&a));
}

/// The page switches BTCUSDT to hedge mode; the venue accepts.
fn switch_to_hedged(e: &mut Engine<PhemexPerpetuals>) {
    let a = command(
        e,
        Command::SetPositionMode {
            command: 7,
            hedged: true,
            symbol: Some("BTCUSDT".into()),
        },
    );
    let sent = requests(&a);
    assert_eq!(sent.len(), 1, "{sent:?}");
    assert!(sent[0].url.contains("/g-positions/switch-pos-mode-sync"));
    let a = respond(e, &sent[0], r#"{"code":0,"data":null}"#);
    assert!(requests(&a).is_empty(), "no read after the switch");
}

fn placed_pos_side(e: &mut Engine<PhemexPerpetuals>, id: &str) -> Option<String> {
    let a = command(
        e,
        Command::PlaceOrders {
            command: 8,
            orders: vec![order(id)],
        },
    );
    let sent = requests(&a);
    assert_eq!(sent.len(), 1, "one request: {sent:?}");
    param(&sent[0], "posSide")
}

#[test]
fn a_confirmed_position_mode_outlasts_a_stale_source_frame() {
    let mut e = frame_fed();
    let mut oneway = source("OneWay");
    e.apply_frame(1001., &oneway.snapshot_frame(1001.).to_vec(), true)
        .unwrap();
    assert!(!e.store().hedged());
    switch_to_hedged(&mut e);
    // The source has not re-read: its snapshot still says one-way.
    e.apply_frame(1002., &oneway.snapshot_frame(1002.).to_vec(), true)
        .unwrap();
    e.drain_actions().for_each(drop);
    assert!(e.store().hedged(), "the held mode keeps the hedge flag");
    assert_eq!(
        placed_pos_side(&mut e, "caller-1").as_deref(),
        Some("Long"),
        "the next order uses the new mode"
    );
}

#[test]
fn a_source_frame_with_the_new_mode_releases_the_hold() {
    let mut e = frame_fed();
    let mut oneway = source("OneWay");
    e.apply_frame(1001., &oneway.snapshot_frame(1001.).to_vec(), true)
        .unwrap();
    switch_to_hedged(&mut e);
    e.apply_frame(1002., &source_snapshot(), true).unwrap();
    // Released: the source's later one-way read applies at once.
    e.apply_frame(1003., &oneway.snapshot_frame(1003.).to_vec(), true)
        .unwrap();
    e.drain_actions().for_each(drop);
    assert_eq!(
        placed_pos_side(&mut e, "caller-1").as_deref(),
        Some("Merged")
    );
}

#[test]
fn refresh_settings_on_a_source_re_reads_the_account_and_writes_nothing() {
    let mut e = source("OneWay");
    let a = command(
        &mut e,
        Command::Venue {
            command: 9,
            action: safe_cex_engine::VenueAction::RefreshSettings {
                symbol: Some("BTCUSDT".into()),
            },
        },
    );
    let sent = requests(&a);
    assert!(!sent.is_empty(), "the account cycle re-reads");
    assert!(
        sent.iter()
            .all(|r| r.method == safe_cex_engine::Method::Get),
        "{sent:?}"
    );
}

#[test]
fn a_source_without_the_symbols_context_does_not_release_a_held_margin_type() {
    let mut e = frame_fed();
    let mut cross = source("OneWay");
    e.apply_frame(1001., &cross.snapshot_frame(1001.).to_vec(), true)
        .unwrap();
    let a = command(
        &mut e,
        Command::SetMarginType {
            command: 7,
            symbol: "BTCUSDT".into(),
            isolated: true,
        },
    );
    let sent = requests(&a);
    assert_eq!(sent.len(), 1, "{sent:?}");
    respond(&mut e, &sent[0], r#"{"code":0,"data":null}"#);

    // The source's VENUE_STATE has no BTCUSDT context: that confirms nothing.
    let mut empty = source_with(vec![]);
    e.apply_frame(1002., &empty.snapshot_frame(1002.).to_vec(), true)
        .unwrap();
    // A stale one, still cross, must not overwrite the confirmed isolated margin.
    e.apply_frame(1003., &cross.snapshot_frame(1003.).to_vec(), true)
        .unwrap();
    e.drain_actions().for_each(drop);
    let a = command(
        &mut e,
        Command::SetLeverage {
            command: 8,
            symbol: "BTCUSDT".into(),
            leverage: 20.,
            side: None,
        },
    );
    let sent = requests(&a);
    assert_eq!(sent.len(), 1, "{sent:?}");
    assert_eq!(
        param(&sent[0], "leverageRr").as_deref(),
        Some("20"),
        "isolated leverage, not a cross-signed one: {}",
        sent[0].url
    );
}

fn landed(filled: f64) -> Order {
    Order {
        id: "landed".into(),
        parent_id: None,
        status: OrderStatus::Open,
        symbol: "BTCUSDT".into(),
        kind: OrderType::Limit,
        side: OrderSide::Buy,
        price: 69000.,
        amount: 0.01,
        filled,
        remaining: 0.01 - filled,
        reduce_only: false,
        close: None,
        client_order_id: Some("caller-1".into()),
        time_in_force: None,
        last_update: 0.,
        trigger_type: None,
        take_profit: None,
        stop_loss: None,
        extra: None,
    }
}

/// An unknown placement of `caller-1`, then a cancel-all on BTCUSDT, then a
/// source frame showing that placement landed.
fn late_landing(filled: f64) -> Vec<Action> {
    let mut e = frame_fed();
    let mut source = source("Hedged");
    source.store_mut().set_orders(vec![]);
    e.apply_frame(1001., &source.snapshot_frame(1001.).to_vec(), true)
        .unwrap();
    e.drain_actions().for_each(drop);
    let a = command(
        &mut e,
        Command::PlaceOrders {
            command: 1,
            orders: vec![order("caller-1")],
        },
    );
    let sent = requests(&a);
    e.handle(
        1002.,
        Event::HttpFailed {
            request: sent[0].id,
            error: "connection reset",
        },
    );
    e.drain_actions().for_each(drop);
    let a = command(
        &mut e,
        Command::CancelSymbolOrders {
            command: 2,
            symbol: "BTCUSDT".into(),
        },
    );
    assert!(
        a.iter().any(
            |a| matches!(a, Action::Notice(Notice::Event(e)) if e.message.contains("caller-1"))
        ),
        "cancel-all warns about the unknown placement: {a:?}"
    );
    source.store_mut().set_orders(vec![landed(filled)]);
    e.apply_frame(1003., &source.snapshot_frame(1003.).to_vec(), true)
        .unwrap();
    e.drain_actions().collect()
}

#[test]
fn a_placement_that_lands_after_a_frame_fed_cancel_all_is_canceled() {
    let a = late_landing(0.);
    let cancels: Vec<_> = requests(&a)
        .into_iter()
        .filter(|r| r.method == safe_cex_engine::Method::Delete)
        .collect();
    assert_eq!(cancels.len(), 1, "{:?}", requests(&a));
    assert_eq!(param(&cancels[0], "orderID").as_deref(), Some("landed"));
}

#[test]
fn a_late_placement_that_filled_is_canceled_and_reported() {
    let a = late_landing(0.004);
    assert!(
        requests(&a)
            .iter()
            .any(|r| r.method == safe_cex_engine::Method::Delete),
        "{:?}",
        requests(&a)
    );
    assert!(
        a.iter().any(|a| matches!(a, Action::Notice(Notice::Event(e)) if e.message.contains("landed after cancel-all"))),
        "{a:?}"
    );
}

/// A page whose source has loaded empty orders, with `caller-1` lost and a
/// cancel-all on BTCUSDT covering it.
fn lost_and_canceled() -> (Engine<PhemexPerpetuals>, Engine<PhemexPerpetuals>) {
    let mut e = frame_fed();
    let mut source = source("Hedged");
    source.store_mut().set_orders(vec![]);
    e.apply_frame(1001., &source.snapshot_frame(1001.).to_vec(), true)
        .unwrap();
    e.drain_actions().for_each(drop);
    let sent = requests(&command(
        &mut e,
        Command::PlaceOrders {
            command: 1,
            orders: vec![order("caller-1")],
        },
    ));
    e.handle(
        1002.,
        Event::HttpFailed {
            request: sent[0].id,
            error: "connection reset",
        },
    );
    e.drain_actions().for_each(drop);
    command(
        &mut e,
        Command::CancelSymbolOrders {
            command: 2,
            symbol: "BTCUSDT".into(),
        },
    );
    (e, source)
}

fn warned_about(actions: &[Action], text: &str) -> bool {
    actions
        .iter()
        .any(|a| matches!(a, Action::Notice(Notice::Event(e)) if e.message.contains(text)))
}

#[test]
fn a_lost_placement_ends_once_a_source_frame_covers_its_signed_expiry() {
    let (mut e, mut source) = lost_and_canceled();
    // A frame written before the signed expiry proves nothing: still refused.
    e.apply_frame(1003., &source.snapshot_frame(1003.).to_vec(), true)
        .unwrap();
    e.drain_actions().for_each(drop);
    let a = command(
        &mut e,
        Command::PlaceOrders {
            command: 3,
            orders: vec![order("caller-1")],
        },
    );
    assert!(requests(&a).is_empty(), "{:?}", requests(&a));

    // One written well past it, without the order: it never landed.
    let late = 10_000_000.;
    e.apply_frame(late, &source.snapshot_frame(late).to_vec(), true)
        .unwrap();
    e.drain_actions().for_each(drop);
    let a = command(
        &mut e,
        Command::CancelSymbolOrders {
            command: 4,
            symbol: "BTCUSDT".into(),
        },
    );
    assert!(
        !warned_about(&a, "caller-1"),
        "no repeated unknown-outcome warning: {a:?}"
    );
    let a = command(
        &mut e,
        Command::PlaceOrders {
            command: 5,
            orders: vec![order("caller-1")],
        },
    );
    assert_eq!(requests(&a).len(), 1, "the clOrdID is free again: {a:?}");
}

#[test]
fn a_create_ack_does_not_reopen_an_order_an_orders_frame_showed_closed() {
    let mut e = frame_fed();
    let mut source = source("Hedged");
    source.store_mut().set_orders(vec![]);
    e.apply_frame(1001., &source.snapshot_frame(1001.).to_vec(), true)
        .unwrap();
    e.drain_actions().for_each(drop);
    let sent = requests(&command(
        &mut e,
        Command::PlaceOrders {
            command: 1,
            orders: vec![order("caller-1")],
        },
    ));
    // The source sees it fill before the page's ack arrives.
    let mut filled = landed(0.01);
    filled.id = "real-1".into();
    filled.status = OrderStatus::Closed;
    source.store_mut().set_orders(vec![filled]);
    e.apply_frame(1001., &source.snapshot_frame(1001.).to_vec(), true)
        .unwrap();
    respond(&mut e, &sent[0], &ack(&sent[0], "real-1").to_string());
    assert_eq!(
        e.store().order("real-1").map(|o| o.status),
        Some(OrderStatus::Closed)
    );
}
