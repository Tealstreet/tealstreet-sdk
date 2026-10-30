#![allow(dead_code)]
//! End to end: a real OKX data-source engine streams frames to a frame-fed OKX
//! engine (the browser's main-thread engine), which signs and sends the writes.
mod support;
use safe_cex_engine::model::{
    Order, OrderSide, OrderStatus, OrderType, PlaceOrderOpts, Position, PositionSide,
};
use safe_cex_engine::{Action, Command, Engine, Event, Method};
use safe_cex_venue_okx::Okx;
use support::{FixtureOptions, MARKET, envelope, prepared, reply, trading_engine};

fn frame_fed() -> Engine<Okx> {
    let mut engine = support::engine(FixtureOptions {
        key: Some("test-key".into()),
        secret: Some("test-secret".into()),
        passphrase: Some("test-passphrase".into()),
        ..FixtureOptions::default()
    });
    engine.set_frame_fed().unwrap();
    engine.start(0.0);
    let start = prepared(&mut engine, 0.0);
    let instruments = start
        .iter()
        .find(|a| matches!(a, Action::Http(_)))
        .expect("public instruments read");
    reply(&mut engine, instruments, &envelope(MARKET), 1.0);
    let _ = prepared(&mut engine, 1.0);
    engine
}

fn resting(id: &str, price: f64) -> Order {
    Order {
        id: id.into(),
        parent_id: None,
        status: OrderStatus::Open,
        symbol: "BTCUSDT".into(),
        kind: OrderType::Limit,
        side: OrderSide::Buy,
        price,
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

fn long(contracts: f64) -> Position {
    Position {
        id: "BTCUSDT-long".into(),
        symbol: "BTCUSDT".into(),
        side: PositionSide::Long,
        entry_price: 40000.,
        notional: 400.,
        leverage: 10.,
        margin_type: None,
        position_mode: None,
        unrealized_pnl: 0.,
        realized_pnl: None,
        contracts,
        liquidation_price: 0.,
        margin_size: None,
        last_update: None,
    }
}

fn place(price: f64) -> PlaceOrderOpts {
    serde_json::from_value(serde_json::json!({
        "symbol": "BTCUSDT", "type": "limit", "side": "buy", "amount": 1, "price": price
    }))
    .unwrap()
}

fn requests(actions: &[Action]) -> Vec<&Action> {
    actions
        .iter()
        .filter(|a| matches!(a, Action::Http(_)))
        .collect()
}

fn path(action: &Action) -> (Method, String) {
    let Action::Http(request) = action else {
        panic!("{action:?}")
    };
    (request.method, request.url.clone())
}

fn send(engine: &mut Engine<Okx>, command: Command, now: f64) -> Vec<Action> {
    engine.handle(now, Event::Command(command));
    prepared(engine, now)
}

/// The data source after its account loaded, holding a resting order and a long.
fn source() -> Engine<Okx> {
    let mut source = trading_engine();
    let store = source.store_mut();
    store.set_orders(vec![resting("701", 39000.)]);
    store.set_positions(vec![long(2.)]);
    source
}

#[test]
fn the_page_engine_writes_from_the_sources_frames_and_its_next_frame_settles_them() {
    let mut source = source();
    let mut page = frame_fed();

    // 1. Snapshot: the page store mirrors the source's account.
    let snapshot = source.snapshot_frame(2.0).to_vec();
    page.apply_frame(2.0, &snapshot, true).unwrap();
    assert_eq!(page.store().orders(), source.store().orders());
    assert_eq!(page.store().positions().len(), 1);
    assert!(page.store().loaded().orders && page.store().loaded().positions);
    assert_eq!(page.store().hedged(), source.store().hedged());

    // 2. A placement from the page: exactly one POST, no read.
    let placed = send(
        &mut page,
        Command::PlaceOrders {
            command: 1,
            orders: vec![place(39500.)],
        },
        3.0,
    );
    let sent = requests(&placed);
    assert_eq!(sent.len(), 1, "{sent:?}");
    assert_eq!(path(sent[0]).0, Method::Post);
    assert!(path(sent[0]).1.ends_with("/api/v5/trade/batch-orders"));
    reply(
        &mut page,
        sent[0],
        &envelope(r#"{"ordId":"702","clOrdId":"","sCode":"0","sMsg":""}"#),
        3.1,
    );
    assert!(
        requests(&prepared(&mut page, 3.1)).is_empty(),
        "no read after the ack"
    );

    // 3. The source streams the acknowledged order; the page cancels it via its store.
    source
        .store_mut()
        .set_orders(vec![resting("701", 39000.), resting("702", 39500.)]);
    let delta = source.take_frame(4.0).expect("orders changed").to_vec();
    page.apply_frame(4.0, &delta, false).unwrap();
    assert!(page.store().order("702").is_some());
    let cancelled = send(
        &mut page,
        Command::CancelOrders {
            command: 2,
            ids: vec!["702".into()],
        },
        5.0,
    );
    let sent = requests(&cancelled);
    assert_eq!(sent.len(), 1, "{sent:?}");
    assert!(
        path(sent[0])
            .1
            .ends_with("/api/v5/trade/cancel-batch-orders")
    );
    reply(
        &mut page,
        sent[0],
        &envelope(r#"{"ordId":"702","clOrdId":"","sCode":"0","sMsg":""}"#),
        5.1,
    );

    // 4. An unknown outcome is never re-sent; the source's next ORDERS frame is
    //    the reconcile, after which the symbol takes a new write.
    let lost = send(
        &mut page,
        Command::PlaceOrders {
            command: 3,
            orders: vec![place(39600.)],
        },
        6.0,
    );
    let sent = requests(&lost);
    assert_eq!(sent.len(), 1, "{sent:?}");
    reply(&mut page, sent[0], "<html>gateway timeout</html>", 6.1);
    assert!(
        requests(&prepared(&mut page, 6.1)).is_empty(),
        "never re-sent, no reconcile read"
    );
    source
        .store_mut()
        .set_orders(vec![resting("701", 39000.), resting("703", 39600.)]);
    let settled = source.take_frame(7.0).expect("orders changed").to_vec();
    page.apply_frame(7.0, &settled, false).unwrap();
    assert!(
        page.store().order("703").is_some(),
        "the lost write landed and is now known"
    );
    let next = send(
        &mut page,
        Command::PlaceOrders {
            command: 4,
            orders: vec![place(39700.)],
        },
        8.0,
    );
    assert_eq!(
        requests(&next).len(),
        1,
        "the symbol is not fenced after the source's frame"
    );

    // 5. Positions from the source gate a position-mode change, with no read.
    let mode = send(
        &mut page,
        Command::SetPositionMode {
            command: 5,
            hedged: true,
            symbol: None,
        },
        9.0,
    );
    assert!(
        requests(&mode).is_empty(),
        "refused: the ingested long is open"
    );
    source.store_mut().set_positions(vec![]);
    let flat = source.take_frame(10.0).expect("positions changed").to_vec();
    page.apply_frame(10.0, &flat, false).unwrap();
    let mode = send(
        &mut page,
        Command::SetPositionMode {
            command: 6,
            hedged: true,
            symbol: None,
        },
        11.0,
    );
    let sent = requests(&mode);
    assert_eq!(sent.len(), 1, "{sent:?}");
    assert!(
        path(sent[0])
            .1
            .ends_with("/api/v5/account/set-position-mode")
    );
}

#[test]
fn a_hedged_source_makes_the_page_send_pos_side() {
    let mut source = source();
    source.store_mut().set_hedged(true);
    let mut page = frame_fed();
    let snapshot = source.snapshot_frame(2.0).to_vec();
    page.apply_frame(2.0, &snapshot, true).unwrap();
    assert!(page.store().hedged());
    let placed = send(
        &mut page,
        Command::PlaceOrders {
            command: 1,
            orders: vec![place(39500.)],
        },
        3.0,
    );
    let Action::Http(request) = requests(&placed)[0] else {
        unreachable!()
    };
    assert!(
        request
            .body
            .as_deref()
            .unwrap_or_default()
            .contains(r#""posSide":"long""#)
    );
}

#[test]
fn the_source_frames_are_well_formed_for_the_page_today() {
    // Runs now: the source's snapshot and deltas parse and apply on a frame-fed
    // engine, even while the account decoders are stubs.
    let mut source = source();
    let mut page = frame_fed();
    let snapshot = source.snapshot_frame(2.0).to_vec();
    page.apply_frame(2.0, &snapshot, true).unwrap();
    source.store_mut().set_orders(vec![]);
    let delta = source.take_frame(3.0).expect("orders changed").to_vec();
    page.apply_frame(3.0, &delta, false).unwrap();
}
