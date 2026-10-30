#![allow(dead_code)]
//! End to end: a real OKX data-source engine streams frames to a frame-fed OKX
//! engine (the browser's main-thread engine), which signs and sends the writes.
mod support;
use safe_cex_engine::model::{
    Order, OrderSide, OrderStatus, OrderType, PlaceOrderOpts, Position, PositionSide,
};
use safe_cex_engine::{Action, Command, Engine, Event, Method, VenueAction};
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
    // The source runs in this process: its frame stamps are on this clock.
    engine.set_source_clock_offset(Some(0.0));
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
    //    the reconcile once written past the fence grace (a frame written
    //    sooner can predate the venue showing the lost order); after it the
    //    symbol takes a new write.
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
    let settled = source.take_frame(3_107.0).expect("orders changed").to_vec();
    page.apply_frame(3_107.0, &settled, false).unwrap();
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
        3_108.0,
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
        3_109.0,
    );
    assert!(
        requests(&mode).is_empty(),
        "refused: the ingested long is open"
    );
    source.store_mut().set_positions(vec![]);
    let flat = source
        .take_frame(3_110.0)
        .expect("positions changed")
        .to_vec();
    page.apply_frame(3_110.0, &flat, false).unwrap();
    let mode = send(
        &mut page,
        Command::SetPositionMode {
            command: 6,
            hedged: true,
            symbol: None,
        },
        3_111.0,
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

/// A flat source, so the page may change position mode.
fn flat_source() -> Engine<Okx> {
    let mut source = source();
    source.store_mut().set_positions(vec![]);
    source
}

fn set_mode(page: &mut Engine<Okx>, command: u32, hedged: bool, now: f64) {
    let sent = send(
        page,
        Command::SetPositionMode {
            command,
            hedged,
            symbol: None,
        },
        now,
    );
    let sent = requests(&sent);
    assert_eq!(sent.len(), 1, "{sent:?}");
    reply(
        page,
        sent[0],
        &envelope(r#"{"posMode":"long_short_mode"}"#),
        now + 0.1,
    );
    let _ = prepared(page, now + 0.1);
}

fn placed_pos_side(page: &mut Engine<Okx>, command: u32, now: f64) -> bool {
    let placed = send(
        page,
        Command::PlaceOrders {
            command,
            orders: vec![place(39500.)],
        },
        now,
    );
    let sent = requests(&placed);
    assert_eq!(sent.len(), 1, "one request: {sent:?}");
    let Action::Http(request) = sent[0] else {
        unreachable!()
    };
    request
        .body
        .as_deref()
        .unwrap_or_default()
        .contains(r#""posSide":"long""#)
}

#[test]
fn a_confirmed_position_mode_outlasts_a_stale_source_frame() {
    let mut source = flat_source();
    let mut page = frame_fed();
    page.apply_frame(2.0, source.snapshot_frame(2.0), true)
        .unwrap();
    assert!(!page.store().hedged());
    set_mode(&mut page, 1, true, 3.0);
    assert!(page.store().hedged());

    // The source has not re-read its config: its snapshot still says net mode.
    let stale = source.snapshot_frame(4.0).to_vec();
    page.apply_frame(4.0, &stale, true).unwrap();
    assert!(
        page.store().hedged(),
        "the held mode survives the stale frame"
    );
    assert!(
        placed_pos_side(&mut page, 2, 5.0),
        "the next order uses the new mode"
    );
}

#[test]
fn a_source_frame_with_the_new_mode_releases_the_hold() {
    let mut source = flat_source();
    let mut page = frame_fed();
    page.apply_frame(2.0, source.snapshot_frame(2.0), true)
        .unwrap();
    set_mode(&mut page, 1, true, 3.0);
    source.store_mut().set_hedged(true);
    page.apply_frame(4.0, source.snapshot_frame(4.0), true)
        .unwrap();
    assert!(page.store().hedged());
    // Released: a later change on the source now applies at once.
    source.store_mut().set_hedged(false);
    page.apply_frame(5.0, source.snapshot_frame(5.0), true)
        .unwrap();
    assert!(!page.store().hedged());
}

#[test]
fn a_confirmed_leverage_outlasts_a_stale_source_frame_until_the_hold_expires() {
    let mut source = flat_source();
    source
        .store_mut()
        .update_setting("BTCUSDT", |s| s.leverage = 5.);
    let mut page = frame_fed();
    page.apply_frame(2.0, source.snapshot_frame(2.0), true)
        .unwrap();
    let sent = send(
        &mut page,
        Command::SetLeverage {
            command: 1,
            symbol: "BTCUSDT".into(),
            leverage: 10.,
            side: None,
        },
        3.0,
    );
    reply(
        &mut page,
        requests(&sent)[0],
        &envelope(r#"{"instId":"BTC-USDT-SWAP","lever":"10","mgnMode":"cross","posSide":""}"#),
        3.1,
    );
    let _ = prepared(&mut page, 3.1);
    page.apply_frame(4.0, source.snapshot_frame(4.0), true)
        .unwrap();
    assert_eq!(page.store().setting("BTCUSDT").unwrap().leverage, 10.);
    // The source never confirms it: after the hold the source's value applies.
    let late = 3.1 + safe_cex_engine::setting_holds::HOLD_MS;
    page.apply_frame(late, source.snapshot_frame(late), true)
        .unwrap();
    assert_eq!(page.store().setting("BTCUSDT").unwrap().leverage, 5.);
}

#[test]
fn refresh_settings_on_a_source_re_reads_config_and_positions_and_writes_nothing() {
    let mut source = trading_engine();
    let _ = prepared(&mut source, 10.0);
    let actions = send(
        &mut source,
        Command::Venue {
            command: 9,
            action: VenueAction::RefreshSettings { symbol: None },
        },
        10.0,
    );
    let sent: Vec<_> = requests(&actions).into_iter().map(path).collect();
    assert!(
        sent.iter().all(|(method, _)| *method == Method::Get),
        "{sent:?}"
    );
    assert!(
        sent.iter()
            .any(|(_, url)| url.contains("/api/v5/account/config")),
        "{sent:?}"
    );
    assert!(
        sent.iter()
            .any(|(_, url)| url.contains("/api/v5/account/positions")),
        "{sent:?}"
    );
}

#[test]
fn a_flat_symbols_leverage_is_confirmed_by_the_sources_refresh_read() {
    // The page sets leverage on a symbol with no position.
    let mut source = flat_source();
    let _ = prepared(&mut source, 2.0);
    source
        .store_mut()
        .update_setting("BTCUSDT", |s| s.leverage = 5.);
    let mut page = frame_fed();
    page.apply_frame(2.0, source.snapshot_frame(2.0), true)
        .unwrap();
    let sent = send(
        &mut page,
        Command::SetLeverage {
            command: 1,
            symbol: "BTCUSDT".into(),
            leverage: 10.,
            side: None,
        },
        3.0,
    );
    reply(
        &mut page,
        requests(&sent)[0],
        &envelope(r#"{"instId":"BTC-USDT-SWAP","lever":"10","mgnMode":"cross","posSide":""}"#),
        3.1,
    );
    let _ = prepared(&mut page, 3.1);

    // The source's refresh reads that symbol's leverage-info in the background.
    let actions = send(
        &mut source,
        Command::Venue {
            command: 9,
            action: VenueAction::RefreshSettings {
                symbol: Some("BTCUSDT".into()),
            },
        },
        4.0,
    );
    let sent = requests(&actions);
    assert!(sent.iter().all(|a| path(a).0 == Method::Get), "{sent:?}");
    let leverage = sent
        .iter()
        .find(|a| {
            path(a)
                .1
                .contains("/api/v5/account/leverage-info?instId=BTC-USDT-SWAP")
        })
        .expect("a leverage-info read for the symbol");
    reply(
        &mut source,
        leverage,
        &envelope(r#"{"instId":"BTC-USDT-SWAP","lever":"10","mgnMode":"cross","posSide":""}"#),
        4.1,
    );
    let _ = prepared(&mut source, 4.1);
    assert_eq!(source.store().setting("BTCUSDT").unwrap().leverage, 10.);

    // Its next frame agrees, so the page's hold releases at once.
    page.apply_frame(5.0, source.snapshot_frame(5.0), true)
        .unwrap();
    assert_eq!(page.store().setting("BTCUSDT").unwrap().leverage, 10.);
    source
        .store_mut()
        .update_setting("BTCUSDT", |s| s.leverage = 7.);
    page.apply_frame(6.0, source.snapshot_frame(6.0), true)
        .unwrap();
    assert_eq!(
        page.store().setting("BTCUSDT").unwrap().leverage,
        7.,
        "released: a later source change applies before the hold would expire"
    );
}
