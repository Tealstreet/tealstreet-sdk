//! A frame-fed Bybit engine: public instrument metadata only, no sockets, no
//! account reads or polls, and only writes (after markets load) are executed.
mod support;
use safe_cex_engine::Command;
use safe_cex_engine::frame_fed::{DATA_SOURCE_ONLY, MARKETS_NOT_LOADED};
use support::{Harness, Options};

fn frame_fed() -> Harness {
    let mut engine = support::engine(Options {
        key: Some("fixture-public".into()),
        secret: Some("fixture-provider".into()),
        ..Default::default()
    });
    engine.set_frame_fed().unwrap();
    Harness::started_engine(engine)
}

fn load(h: &mut Harness) {
    let paths = h.paths();
    assert!(
        paths
            .iter()
            .all(|path| path.contains("/v5/market/instruments-info")),
        "only public instrument metadata: {paths:?}"
    );
    h.respond("status=PreLaunch", 200, support::PRELAUNCH);
    h.respond("/v5/market/instruments-info", 200, support::INSTRUMENTS);
    assert!(
        h.requests.is_empty(),
        "nothing after metadata: {:?}",
        h.paths()
    );
}

#[test]
fn a_frame_fed_engine_loads_public_metadata_once_and_opens_no_socket() {
    let mut h = frame_fed();
    load(&mut h);
    assert!(h.engine.store().loaded().markets);
    h.advance(120_000.0);
    assert!(h.requests.is_empty(), "no polls: {:?}", h.paths());
    assert!(h.sockets.is_empty(), "no sockets: {:?}", h.sockets);
}

#[test]
fn writes_before_markets_and_non_writes_are_refused_without_a_request() {
    let mut h = frame_fed();
    h.command(Command::PlaceOrders {
        command: 1,
        orders: vec![support::order()],
    });
    assert!(
        matches!(h.result(1), Some(Err(message)) if message.contains(MARKETS_NOT_LOADED)),
        "{:?}",
        h.result(1)
    );
    load(&mut h);
    h.command(Command::FetchAccount { command: 2 });
    assert!(
        matches!(h.result(2), Some(Err(message)) if message.contains(DATA_SOURCE_ONLY)),
        "{:?}",
        h.result(2)
    );
    assert!(h.requests.is_empty());
}

#[test]
fn a_frame_fed_engine_ingests_frames_and_a_normal_engine_refuses_them() {
    let mut h = frame_fed();
    load(&mut h);
    let frame = h.engine.snapshot_frame(h.now).to_vec();
    assert!(h.engine.apply_frame(h.now, &frame, true).is_ok());
    assert!(h.engine.apply_frame(h.now, b"not a frame", false).is_err());
    let mut normal = Harness::started(Options::default());
    assert!(normal.engine.apply_frame(normal.now, &frame, true).is_err());
    normal.collect();
}

/// A data source's snapshot: one resting order and Bybit's VENUE_STATE
/// (version 1: BTCUSDT linear hedged, no position-bound stops).
fn source_frame() -> Vec<u8> {
    source_frame_at(0.0)
}

/// `source_frame`, written by the source at `written` (the frame header stamp).
fn source_frame_at(written: f64) -> Vec<u8> {
    use safe_cex_engine::model::{Order, OrderSide, OrderStatus, OrderType};
    let mut source = safe_cex_engine::Store::default();
    source.set_orders(vec![Order {
        id: "server-order".into(),
        parent_id: None,
        status: OrderStatus::Open,
        symbol: "BTCUSDT".into(),
        kind: OrderType::Limit,
        side: OrderSide::Sell,
        price: 120.0,
        amount: 0.5,
        filled: 0.0,
        remaining: 0.5,
        reduce_only: false,
        close: None,
        client_order_id: None,
        time_in_force: None,
        last_update: 0.0,
        trigger_type: None,
        take_profit: None,
        stop_loss: None,
        extra: None,
    }]);
    let mut state = vec![1u8];
    state.extend_from_slice(&1u32.to_le_bytes());
    state.extend_from_slice(&[0, 1]);
    state.extend_from_slice(&7u16.to_le_bytes());
    state.extend_from_slice(b"BTCUSDT");
    state.extend_from_slice(&0u32.to_le_bytes());
    let mut writer = safe_cex_engine::wire::FrameWriter::new(Vec::new(), written);
    source.write(&source.account_snapshot(), &mut writer);
    writer.venue_state(&state);
    writer.finish()
}

#[test]
fn an_ingested_hedge_mode_places_in_exactly_one_request() {
    let mut h = frame_fed();
    load(&mut h);
    h.engine.apply_frame(h.now, &source_frame(), true).unwrap();
    h.collect();
    h.command(Command::PlaceOrders {
        command: 1,
        orders: vec![support::order()],
    });
    assert_eq!(h.requests.len(), 1, "no mode read: {:?}", h.paths());
    let request = h.take("/v5/order/create");
    let body = request.body.clone().unwrap();
    assert!(
        body.contains(r#""positionIdx":1"#),
        "hedged buy opens long: {body}"
    );
    h.reply(
        request.id,
        200,
        r#"{"retCode":0,"result":{"list":[{"orderId":"o-1"}]},"retExtInfo":{"list":[{"code":0,"msg":"OK"}]}}"#,
    );
    assert!(matches!(h.result(1), Some(Ok(_))), "{:?}", h.result(1));
    h.advance(120_000.0);
    assert!(h.requests.is_empty(), "no follow-up read: {:?}", h.paths());
    assert!(h.sockets.is_empty());
}

#[test]
fn an_ingested_order_cancels_in_exactly_one_request() {
    let mut h = frame_fed();
    load(&mut h);
    h.engine.apply_frame(h.now, &source_frame(), true).unwrap();
    h.collect();
    h.command(Command::CancelOrders {
        command: 1,
        ids: vec!["server-order".into()],
    });
    assert_eq!(h.requests.len(), 1, "{:?}", h.paths());
    let body = h.take("/v5/order/cancel").body.clone().unwrap();
    assert!(
        body.contains("server-order") && body.contains("BTCUSDT"),
        "{body}"
    );
}

#[test]
fn an_unknown_placement_is_never_re_sent() {
    let mut h = frame_fed();
    load(&mut h);
    h.engine.apply_frame(h.now, &source_frame(), true).unwrap();
    h.collect();
    h.command(Command::PlaceOrders {
        command: 1,
        orders: vec![support::order()],
    });
    let request = h.take("/v5/order/create");
    h.event(safe_cex_engine::Event::HttpFailed {
        request: request.id,
        error: "connection reset",
    });
    assert_eq!(h.unknown, vec![1]);
    h.advance(300_000.0);
    assert!(
        h.requests.is_empty(),
        "no reconcile read, no re-send: {:?}",
        h.paths()
    );
}

/// A frame-fed engine with one unknown placement on BTCUSDT, and the source's
/// clock `offset` ms from this host's (as the host measured it).
fn fenced(offset: Option<f64>) -> Harness {
    let mut h = frame_fed();
    load(&mut h);
    h.engine.set_source_clock_offset(offset);
    h.engine.apply_frame(h.now, &source_frame(), true).unwrap();
    h.collect();
    h.command(Command::PlaceOrders {
        command: 1,
        orders: vec![support::order()],
    });
    let request = h.take("/v5/order/create");
    h.event(safe_cex_engine::Event::HttpFailed {
        request: request.id,
        error: "connection reset",
    });
    assert_eq!(h.unknown, vec![1]);
    h
}

/// Ingest a source frame stamped `ahead` ms past this host's now (the
/// source's clock), then try a placement: true when it went out.
fn retry_after(h: &mut Harness, ahead: f64, command: u32) -> bool {
    let stamp = h.now + ahead;
    h.engine
        .apply_frame(h.now, &source_frame_at(stamp), false)
        .unwrap();
    h.collect();
    h.command(Command::PlaceOrders {
        command,
        orders: vec![support::order()],
    });
    let sent = !h.requests.is_empty();
    h.requests.clear();
    sent
}

#[test]
fn a_source_clock_ahead_does_not_lift_a_fence_early() {
    // The server's clock runs 10s ahead; the host measured it.
    let mut h = fenced(Some(10_000.0));
    h.advance(250.0);
    assert!(
        !retry_after(&mut h, 10_000.0, 2),
        "250ms later on one clock"
    );
    h.advance(3_000.0);
    assert!(
        retry_after(&mut h, 10_000.0, 3),
        "past the grace on one clock"
    );
}

#[test]
fn an_unmeasured_source_clock_waits_the_long_grace() {
    let mut h = fenced(None);
    h.advance(3_500.0);
    assert!(
        !retry_after(&mut h, 0.0, 2),
        "3.5s is not enough unmeasured"
    );
    h.advance(27_000.0);
    assert!(retry_after(&mut h, 0.0, 3), "past the unsynced grace");
}

#[test]
fn a_frame_written_right_after_an_unknown_placement_keeps_its_fence() {
    let mut h = frame_fed();
    load(&mut h);
    h.engine.set_source_clock_offset(Some(0.0));
    h.engine.apply_frame(h.now, &source_frame(), true).unwrap();
    h.collect();
    h.command(Command::PlaceOrders {
        command: 1,
        orders: vec![support::order()],
    });
    let request = h.take("/v5/order/create");
    h.event(safe_cex_engine::Event::HttpFailed {
        request: request.id,
        error: "connection reset",
    });
    assert_eq!(h.unknown, vec![1]);
    // The source wrote this before the venue showed the lost order: a retry
    // must not go out, or it could place twice.
    h.advance(250.0);
    h.engine
        .apply_frame(h.now, &source_frame_at(h.now), false)
        .unwrap();
    h.collect();
    h.command(Command::PlaceOrders {
        command: 2,
        orders: vec![support::order()],
    });
    assert!(h.requests.is_empty(), "fenced: {:?}", h.paths());
    assert!(
        matches!(h.result(2), Some(Err(message)) if message.contains("uncertain")),
        "{:?}",
        h.result(2)
    );
    // A frame the source wrote well after the loss is the reconcile.
    h.advance(3_000.0);
    h.engine
        .apply_frame(h.now, &source_frame_at(h.now), false)
        .unwrap();
    h.collect();
    h.command(Command::PlaceOrders {
        command: 3,
        orders: vec![support::order()],
    });
    assert_eq!(h.requests.len(), 1, "{:?}", h.paths());
}

/// A data source's snapshot whose STATUS and VENUE_STATE say BTCUSDT is `hedged`.
fn mode_frame(hedged: bool) -> Vec<u8> {
    let mut source = safe_cex_engine::Store::default();
    source.set_hedged(hedged);
    let mut state = vec![1u8];
    state.extend_from_slice(&1u32.to_le_bytes());
    state.extend_from_slice(&[0, u8::from(hedged)]);
    state.extend_from_slice(&7u16.to_le_bytes());
    state.extend_from_slice(b"BTCUSDT");
    state.extend_from_slice(&0u32.to_le_bytes());
    let mut writer = safe_cex_engine::wire::FrameWriter::new(Vec::new(), 0.0);
    source.write(&source.account_snapshot(), &mut writer);
    writer.venue_state(&state);
    writer.finish()
}

fn placed_index(h: &mut Harness, command: u32) -> String {
    h.command(Command::PlaceOrders {
        command,
        orders: vec![support::order()],
    });
    assert_eq!(h.requests.len(), 1, "{:?}", h.paths());
    let body = h.take("/v5/order/create").body.clone().unwrap();
    let at = body.find(r#""positionIdx":"#).expect("positionIdx") + 14;
    body[at..at + 1].to_owned()
}

/// A one-way account switched to hedge mode on this page engine.
fn switched_to_hedge() -> Harness {
    let mut h = frame_fed();
    load(&mut h);
    h.engine
        .apply_frame(h.now, &mode_frame(false), true)
        .unwrap();
    h.collect();
    h.command(Command::SetPositionMode {
        command: 1,
        hedged: true,
        symbol: None,
    });
    let request = h.take("/v5/position/switch-mode");
    h.reply(
        request.id,
        200,
        r#"{"retCode":0,"retMsg":"OK","result":{},"retExtInfo":{},"time":1672281607343}"#,
    );
    assert!(matches!(h.result(1), Some(Ok(_))), "{:?}", h.result(1));
    assert!(h.requests.is_empty(), "no positions read: {:?}", h.paths());
    h
}

#[test]
fn a_switched_mode_outlasts_a_source_that_has_not_re_read_it() {
    let mut h = switched_to_hedge();
    assert!(h.engine.store().hedged());
    // The source still reports one-way: this engine's confirmed switch stands.
    h.engine
        .apply_frame(h.now, &mode_frame(false), false)
        .unwrap();
    h.collect();
    assert!(h.engine.store().hedged());
    assert_eq!(placed_index(&mut h, 2), "1", "hedged buy opens long");
}

#[test]
fn the_hold_clears_once_the_source_agrees() {
    let mut h = switched_to_hedge();
    h.engine
        .apply_frame(h.now, &mode_frame(true), false)
        .unwrap();
    h.collect();
    // Released: a later source change applies again.
    h.engine
        .apply_frame(h.now, &mode_frame(false), false)
        .unwrap();
    h.collect();
    assert!(!h.engine.store().hedged());
    assert_eq!(placed_index(&mut h, 2), "0");
}

#[test]
fn an_unconfirmed_hold_expires_and_the_source_wins() {
    let mut h = switched_to_hedge();
    h.advance(safe_cex_engine::setting_holds::HOLD_MS);
    h.engine
        .apply_frame(h.now, &mode_frame(false), false)
        .unwrap();
    h.collect();
    assert!(!h.engine.store().hedged());
    assert_eq!(placed_index(&mut h, 2), "0");
}

#[test]
fn a_frame_without_status_does_not_release_the_hedge_hold() {
    let mut h = switched_to_hedge();
    // A ticker- or orders-only delta carries no STATUS: no evidence either way.
    let quiet = safe_cex_engine::wire::FrameWriter::new(Vec::new(), 0.0).finish();
    h.engine.apply_frame(h.now, &quiet, false).unwrap();
    h.collect();
    // The source's next, stale STATUS still cannot put one-way back.
    h.engine
        .apply_frame(h.now, &mode_frame(false), false)
        .unwrap();
    h.collect();
    assert!(h.engine.store().hedged());
    assert_eq!(placed_index(&mut h, 2), "1");
}

/// A real keyed source: public startup answered, account info loaded.
fn source() -> Harness {
    let mut h = Harness::started(Options {
        key: Some("fixture-public".into()),
        secret: Some("fixture-provider".into()),
        ..Default::default()
    });
    if h.has_request("/v5/account/info") {
        h.respond("/v5/account/info", 200, support::ACCOUNT_INFO);
    }
    if h.has_request("/v5/account/fee-rate") {
        h.respond("/v5/account/fee-rate", 200, support::FEE_RATES);
    }
    h.respond("status=PreLaunch", 200, support::PRELAUNCH);
    h.respond("/v5/market/instruments-info", 200, support::INSTRUMENTS);
    if h.has_request("/v5/market/tickers") {
        h.respond("/v5/market/tickers", 200, support::TICKERS);
    }
    h
}

#[test]
fn a_real_sources_snapshot_carries_its_hedge_mode_to_a_one_request_place() {
    // The source learns BTCUSDT is hedged from its own placement's mode read;
    // its orders reach its store only from its private stream, not modelled here.
    let mut source = source();
    source.requests.clear();
    source.command(Command::PlaceOrders {
        command: 1,
        orders: vec![support::order()],
    });
    source.respond(
        "/v5/position/list",
        200,
        r#"{"retCode":0,"result":{"list":[{"symbol":"BTCUSDT","positionIdx":1},{"symbol":"BTCUSDT","positionIdx":2}]}}"#,
    );
    let create = source.take("/v5/order/create");
    assert!(
        create
            .body
            .as_deref()
            .unwrap()
            .contains(r#""positionIdx":1"#),
        "the source itself places hedged"
    );
    source.reply(
        create.id,
        200,
        r#"{"retCode":0,"result":{"list":[{"orderId":"source-order"}]},"retExtInfo":{"list":[{"code":0,"msg":"OK"}]}}"#,
    );
    let snapshot = source.engine.snapshot_frame(source.now).to_vec();

    let mut h = frame_fed();
    load(&mut h);
    h.engine.apply_frame(h.now, &snapshot, true).unwrap();
    h.collect();
    h.command(Command::PlaceOrders {
        command: 2,
        orders: vec![support::order()],
    });
    assert_eq!(h.requests.len(), 1, "no mode read: {:?}", h.paths());
    let body = h.take("/v5/order/create").body.clone().unwrap();
    assert!(body.contains(r#""positionIdx":1"#), "{body}");
}

/// A data source's frame carrying BTCUSDT's settings row (SETTINGS section).
fn settings_frame(leverage: f64, margin: safe_cex_engine::model::MarginType) -> Vec<u8> {
    let mut source = safe_cex_engine::Store::default();
    source.set_markets(h_markets());
    source.update_setting("BTCUSDT", |s| {
        s.leverage = leverage;
        s.margin_type = Some(margin);
    });
    let mut writer = safe_cex_engine::wire::FrameWriter::new(Vec::new(), 0.0);
    source.write(&source.everything(), &mut writer);
    writer.finish()
}

/// The page engine's own catalogue, so the source's settings rows index the same markets.
fn h_markets() -> Vec<safe_cex_engine::model::Market> {
    let mut h = frame_fed();
    load(&mut h);
    h.engine.store().markets().to_vec()
}

fn loaded_with_settings() -> Harness {
    let mut h = frame_fed();
    load(&mut h);
    let cross = safe_cex_engine::model::MarginType::Cross;
    h.engine
        .apply_frame(h.now, &settings_frame(10.0, cross), true)
        .unwrap();
    h.collect();
    assert_eq!(h.engine.store().setting("BTCUSDT").unwrap().leverage, 10.0);
    h
}

#[test]
fn a_confirmed_leverage_outlasts_a_stale_source_until_it_agrees() {
    use safe_cex_engine::model::MarginType::Cross;
    let mut h = loaded_with_settings();
    h.command(Command::SetLeverage {
        command: 1,
        symbol: "BTCUSDT".into(),
        leverage: 20.0,
        side: None,
    });
    let request = h.take("/v5/position/set-leverage");
    h.reply(
        request.id,
        200,
        r#"{"retCode":0,"retMsg":"OK","result":{},"retExtInfo":{},"time":1672281607343}"#,
    );
    assert!(matches!(h.result(1), Some(Ok(_))), "{:?}", h.result(1));
    assert!(
        h.requests.is_empty(),
        "no read after the write: {:?}",
        h.paths()
    );
    // The source has not re-read: its stale row cannot put 10x back.
    h.engine
        .apply_frame(h.now, &settings_frame(10.0, Cross), false)
        .unwrap();
    h.collect();
    assert_eq!(h.engine.store().setting("BTCUSDT").unwrap().leverage, 20.0);
    // It agrees: released, so its later rows apply again.
    h.engine
        .apply_frame(h.now, &settings_frame(20.0, Cross), false)
        .unwrap();
    h.engine
        .apply_frame(h.now, &settings_frame(10.0, Cross), false)
        .unwrap();
    h.collect();
    assert_eq!(h.engine.store().setting("BTCUSDT").unwrap().leverage, 10.0);
}

#[test]
fn a_confirmed_margin_switch_outlasts_a_stale_source_and_reads_nothing() {
    use safe_cex_engine::model::MarginType::{Cross, Isolated};
    let mut h = loaded_with_settings();
    h.command(Command::SetMarginType {
        command: 1,
        symbol: "BTCUSDT".into(),
        isolated: true,
    });
    let request = h.take("/v5/account/set-margin-mode");
    h.reply(request.id, 200, r#"{"retCode":0,"result":{"reasons":[]}}"#);
    assert!(matches!(h.result(1), Some(Ok(_))), "{:?}", h.result(1));
    assert!(h.requests.is_empty(), "no positions read: {:?}", h.paths());
    let margin = |h: &Harness| h.engine.store().setting("BTCUSDT").unwrap().margin_type;
    assert_eq!(margin(&h), Some(Isolated));
    h.engine
        .apply_frame(h.now, &settings_frame(10.0, Cross), false)
        .unwrap();
    h.collect();
    assert_eq!(margin(&h), Some(Isolated));
    h.engine
        .apply_frame(h.now, &settings_frame(10.0, Isolated), false)
        .unwrap();
    h.engine
        .apply_frame(h.now, &settings_frame(10.0, Cross), false)
        .unwrap();
    h.collect();
    assert_eq!(margin(&h), Some(Cross));
}

/// The source never sees a page write, so nothing would send the ORDERS frame
/// that lifts its fence: the engine says it is held, and when a frame would qualify.
fn orders_held(h: &Harness) -> Vec<f64> {
    h.notices
        .iter()
        .filter_map(|notice| match notice {
            safe_cex_engine::Notice::Event(event)
                if event.code == safe_cex_engine::log::code::ORDERS_HELD =>
            {
                event
                    .fields
                    .iter()
                    .find_map(|(name, value)| match (name, value) {
                        (&"liftAfterMs", safe_cex_engine::Value::Num(ms)) => Some(*ms),
                        _ => None,
                    })
            }
            _ => None,
        })
        .collect()
}

#[test]
fn a_fence_asks_once_for_the_source_orders_after_its_grace() {
    let measured = fenced(Some(0.0));
    assert_eq!(orders_held(&measured), vec![3_000.0]);
    let unmeasured = fenced(None);
    assert_eq!(orders_held(&unmeasured), vec![30_000.0]);
}

#[test]
fn a_fence_already_waiting_does_not_ask_again() {
    let mut h = fenced(Some(0.0));
    h.advance(250.0);
    // A second lost write before any frame: the same wait covers it.
    assert!(!retry_after(&mut h, 0.0, 2));
    h.advance(60_000.0);
    assert_eq!(orders_held(&h), vec![3_000.0]);
}

#[test]
fn a_new_fence_after_a_lift_asks_again() {
    let mut h = fenced(Some(0.0));
    h.advance(3_500.0);
    h.engine
        .apply_frame(h.now, &source_frame_at(h.now), false)
        .unwrap();
    h.collect();
    h.command(Command::PlaceOrders {
        command: 2,
        orders: vec![support::order()],
    });
    // The qualifying frame lifted it; this placement is lost too.
    let lost = h.take("/v5/order/create");
    h.event(safe_cex_engine::Event::HttpFailed {
        request: lost.id,
        error: "connection reset",
    });
    assert_eq!(orders_held(&h), vec![3_000.0, 3_000.0]);
}

/// A source frame written now, as the data source answers an `orders.held` ask.
fn ingest_now(h: &mut Harness) {
    h.engine
        .apply_frame(h.now, &source_frame_at(h.now), false)
        .unwrap();
    h.collect();
}

const ETH_FLAT: &str = r#"{"retCode":0,"retMsg":"OK","result":{"list":[{"symbol":"ETHUSDT","positionIdx":0,"side":"","size":"0","avgPrice":"0","markPrice":"3000","positionValue":"0","leverage":"10","unrealisedPnl":"0","curRealisedPnl":"0","positionIM":"0"}],"nextPageCursor":""}}"#;

/// Send an ETHUSDT placement, answering the mode read the source frame left unknown.
fn place_eth(h: &mut Harness, command: u32) -> safe_cex_engine::HttpRequest {
    let mut order = support::order();
    order.symbol = "ETHUSDT".into();
    h.command(Command::PlaceOrders {
        command,
        orders: vec![order],
    });
    if h.paths()
        .iter()
        .any(|path| path.contains("/v5/position/list"))
    {
        h.respond("/v5/position/list", 200, ETH_FLAT);
    }
    h.take("/v5/order/create")
}

fn lose_eth_placement(h: &mut Harness, command: u32) {
    let lost = place_eth(h, command);
    h.event(safe_cex_engine::Event::HttpFailed {
        request: lost.id,
        error: "connection reset",
    });
}

#[test]
fn a_fence_the_lifting_frame_could_not_lift_is_asked_for_again() {
    let mut h = fenced(Some(0.0));
    h.advance(2_000.0);
    // Another symbol's write is lost while the first ask still waits.
    lose_eth_placement(&mut h, 2);
    assert_eq!(orders_held(&h), vec![3_000.0]);
    h.advance(1_500.0);
    // The frame that answers the first ask lifts only the first fence.
    ingest_now(&mut h);
    assert_eq!(orders_held(&h), vec![3_000.0, 1_500.0]);
    h.advance(1_500.0);
    ingest_now(&mut h);
    place_eth(&mut h, 3);
    assert_eq!(orders_held(&h).len(), 2, "both fences lifted");
}

#[test]
fn a_fence_sealed_right_after_a_full_lift_is_announced() {
    let mut h = fenced(Some(0.0));
    h.advance(100.0);
    h.command(Command::CancelOrders {
        command: 2,
        ids: vec!["server-order".into()],
    });
    let cancel = h.take("/v5/order/cancel");
    h.advance(3_400.0);
    ingest_now(&mut h);
    // The cancel sent before the lift finds no order in the very next event,
    // which fences the symbol again.
    h.reply(
        cancel.id,
        200,
        r#"{"retCode":110001,"retMsg":"order not exists or too late to cancel","result":{},"retExtInfo":{},"time":1}"#,
    );
    assert_eq!(orders_held(&h), vec![3_000.0, 3_000.0]);
}

const NO_ROWS: &str = r#"{"retCode":0,"retMsg":"OK","result":{"list":[],"nextPageCursor":""}}"#;
const WALLET: &str = r#"{"retCode":0,"retMsg":"OK","result":{"list":[{"accountType":"UNIFIED","totalEquity":"1100","totalWalletBalance":"1000","totalInitialMargin":"10","totalMaintenanceMargin":"1","totalAvailableBalance":"900","coin":[]}]}}"#;
/// The page's lost placement, as the venue lists it once it rests.
const LOST_ORDER_RESTING: &str = r#"{"retCode":0,"retMsg":"OK","result":{"list":[{"symbol":"BTCUSDT","orderId":"lost-order","side":"Buy","orderType":"Limit","orderStatus":"New","price":"100","qty":"0.01","leavesQty":"0.01","cumExecQty":"0"}],"nextPageCursor":""}}"#;

fn answer_all(h: &mut Harness, path: &str, body: &str) {
    while h.has_request(path) {
        h.respond(path, 200, body);
    }
}

/// A loaded keyed source, and a frame-fed page fed by it that has just lost a
/// BTCUSDT placement. Nothing reaches the source's private stream: it is stalled.
fn source_and_fenced_page() -> (Harness, Harness) {
    let mut source = source();
    answer_all(&mut source, "/v5/order/realtime", NO_ROWS);
    // One-way BTCUSDT: the page learns the mode from the source's venue state.
    answer_all(
        &mut source,
        "/v5/position/list",
        r#"{"retCode":0,"retMsg":"OK","result":{"list":[{"symbol":"BTCUSDT","positionIdx":0,"side":"","size":"0","avgPrice":"0","markPrice":"100","positionValue":"0","leverage":"10","unrealisedPnl":"0","curRealisedPnl":"0","positionIM":"0"}],"nextPageCursor":""}}"#,
    );
    answer_all(&mut source, "/v5/account/wallet-balance", WALLET);
    source.engine.take_frame(source.now);
    let mut page = frame_fed();
    load(&mut page);
    page.engine.set_source_clock_offset(Some(0.0));
    page.now = source.now;
    let snapshot = source.engine.snapshot_frame(source.now).to_vec();
    page.engine.apply_frame(page.now, &snapshot, true).unwrap();
    page.collect();
    page.command(Command::PlaceOrders {
        command: 1,
        orders: vec![support::order()],
    });
    let lost = page.take("/v5/order/create");
    page.event(safe_cex_engine::Event::HttpFailed {
        request: lost.id,
        error: "connection reset",
    });
    assert_eq!(page.unknown, vec![1]);
    (source, page)
}

fn advance_both(source: &mut Harness, page: &mut Harness, ms: f64) {
    source.advance(ms);
    page.advance(ms);
}

/// Forward the source's next frame to the page; false when it had none.
fn forward(source: &mut Harness, page: &mut Harness) -> bool {
    let Some(frame) = source.engine.take_frame(source.now).map(<[u8]>::to_vec) else {
        return false;
    };
    page.engine.apply_frame(page.now, &frame, false).unwrap();
    page.collect();
    true
}

/// The page's ask, as its host sends it once `orders.held` says a frame could lift.
fn refresh_orders(source: &mut Harness, command: u32) {
    source.requests.clear();
    source.command(Command::Venue {
        command,
        action: safe_cex_engine::VenueAction::RefreshOrders,
    });
    assert!(
        source.has_request("/v5/order/realtime"),
        "a fresh orders read: {:?}",
        source.paths()
    );
}

fn btc_place_goes_out(page: &mut Harness, command: u32) -> bool {
    page.requests.clear();
    page.command(Command::PlaceOrders {
        command,
        orders: vec![support::order()],
    });
    page.has_request("/v5/order/create")
}

#[test]
fn a_stalled_source_lifts_nothing_until_its_fresh_orders_read_lands() {
    let (mut source, mut page) = source_and_fenced_page();
    advance_both(&mut source, &mut page, 3_500.0);
    assert_eq!(orders_held(&page), vec![3_000.0]);
    refresh_orders(&mut source, 90);
    // The ask alone sends nothing: a stalled source has nothing new to say.
    assert!(!forward(&mut source, &mut page));
    assert!(!btc_place_goes_out(&mut page, 2));
    // The fresh read finds no such order: its rows go out, and they lift the fence.
    answer_all(&mut source, "/v5/order/realtime", NO_ROWS);
    assert!(
        forward(&mut source, &mut page),
        "the read's orders are sent unchanged"
    );
    assert!(btc_place_goes_out(&mut page, 3));
}

#[test]
fn a_fresh_read_that_finds_the_lost_order_brings_its_row() {
    let (mut source, mut page) = source_and_fenced_page();
    advance_both(&mut source, &mut page, 3_500.0);
    refresh_orders(&mut source, 90);
    source.respond("settleCoin=USDT", 200, LOST_ORDER_RESTING);
    answer_all(&mut source, "/v5/order/realtime", NO_ROWS);
    assert!(forward(&mut source, &mut page));
    assert!(
        page.engine
            .store()
            .orders()
            .iter()
            .any(|order| order.id == "lost-order" && order.symbol == "BTCUSDT"),
        "the placement did land; the page now shows it"
    );
}

#[test]
fn a_refresh_in_flight_is_not_asked_again() {
    let (mut source, mut page) = source_and_fenced_page();
    advance_both(&mut source, &mut page, 3_500.0);
    refresh_orders(&mut source, 90);
    source.requests.clear();
    source.command(Command::Venue {
        command: 91,
        action: safe_cex_engine::VenueAction::RefreshOrders,
    });
    assert!(!source.has_request("/v5/order/realtime"));
    assert!(matches!(source.result(91), Some(Ok(_))));
}
