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
    let mut writer = safe_cex_engine::wire::FrameWriter::new(Vec::new(), 0.0);
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
