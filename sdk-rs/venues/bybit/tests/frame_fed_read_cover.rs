//! A source frame's ORDERS lift a frame-fed fence only as far as the rows are
//! fresh: rows from a venue read are as old as that read's send, not the frame.
mod support;
use safe_cex_engine::Command;
use support::{Harness, Options};

const NO_ROWS: &str = r#"{"retCode":0,"retMsg":"OK","result":{"list":[],"nextPageCursor":""}}"#;
const ONE_WAY_BTC: &str = r#"{"retCode":0,"retMsg":"OK","result":{"list":[{"symbol":"BTCUSDT","positionIdx":0,"side":"","size":"0","avgPrice":"0","markPrice":"100","positionValue":"0","leverage":"10","unrealisedPnl":"0","curRealisedPnl":"0","positionIM":"0"}],"nextPageCursor":""}}"#;
const WALLET: &str = r#"{"retCode":0,"retMsg":"OK","result":{"list":[{"accountType":"UNIFIED","totalEquity":"1100","totalWalletBalance":"1000","totalInitialMargin":"10","totalMaintenanceMargin":"1","totalAvailableBalance":"900","coin":[]}]}}"#;

fn keys() -> Options {
    Options {
        key: Some("fixture-public".into()),
        secret: Some("fixture-provider".into()),
        ..Default::default()
    }
}

fn answer_all(h: &mut Harness, path: &str, body: &str) {
    while h.has_request(path) {
        h.respond(path, 200, body);
    }
}

/// A keyed source whose startup orders read is still out; everything else answered.
fn source_with_orders_read_out() -> Harness {
    let mut source = Harness::started(keys());
    for (path, body) in [
        ("/v5/account/info", support::ACCOUNT_INFO),
        ("/v5/account/fee-rate", support::FEE_RATES),
        ("status=PreLaunch", support::PRELAUNCH),
        ("/v5/market/instruments-info", support::INSTRUMENTS),
        ("/v5/market/tickers", support::TICKERS),
    ] {
        if source.has_request(path) {
            source.respond(path, 200, body);
        }
    }
    answer_all(&mut source, "/v5/position/list", ONE_WAY_BTC);
    answer_all(&mut source, "/v5/account/wallet-balance", WALLET);
    assert!(source.has_request("/v5/order/realtime"));
    source
}

fn frame_fed_page(source: &mut Harness) -> Harness {
    let mut engine = support::engine(keys());
    engine.set_frame_fed().unwrap();
    let mut page = Harness::started_engine(engine);
    page.respond("status=PreLaunch", 200, support::PRELAUNCH);
    page.respond("/v5/market/instruments-info", 200, support::INSTRUMENTS);
    page.engine.set_source_clock_offset(Some(0.0));
    page.now = source.now;
    let snapshot = source.engine.snapshot_frame(source.now).to_vec();
    source.engine.take_frame(source.now);
    page.engine.apply_frame(page.now, &snapshot, true).unwrap();
    page.collect();
    page
}

fn lose_btc_placement(page: &mut Harness, command: u32) {
    page.command(Command::PlaceOrders {
        command,
        orders: vec![support::order()],
    });
    let lost = page.take("/v5/order/create");
    page.event(safe_cex_engine::Event::HttpFailed {
        request: lost.id,
        error: "connection reset",
    });
}

fn forward(source: &mut Harness, page: &mut Harness) -> bool {
    let Some(frame) = source.engine.take_frame(source.now).map(<[u8]>::to_vec) else {
        return false;
    };
    page.engine.apply_frame(page.now, &frame, false).unwrap();
    page.collect();
    true
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
fn a_read_sent_before_the_lost_write_cannot_lift_its_fence_however_late_it_lands() {
    let mut source = source_with_orders_read_out();
    let mut page = frame_fed_page(&mut source);
    source.advance(1_000.0);
    page.advance(1_000.0);
    lose_btc_placement(&mut page, 1);
    assert_eq!(page.unknown, vec![1]);
    // The read sent a second before the write lands well past the grace.
    source.advance(4_000.0);
    page.advance(4_000.0);
    answer_all(&mut source, "/v5/order/realtime", NO_ROWS);
    assert!(forward(&mut source, &mut page), "the read's rows go out");
    assert!(
        !btc_place_goes_out(&mut page, 2),
        "rows read before the write say nothing about it"
    );
}

#[test]
fn a_frame_without_read_rows_still_lifts_on_its_stamp() {
    let mut source = source_with_orders_read_out();
    answer_all(&mut source, "/v5/order/realtime", NO_ROWS);
    let mut page = frame_fed_page(&mut source);
    lose_btc_placement(&mut page, 1);
    source.advance(3_500.0);
    page.advance(3_500.0);
    // A stream event: the source's own order list changes with no read behind it.
    source
        .engine
        .store_mut()
        .upsert_order(safe_cex_engine::model::Order {
            id: "streamed".into(),
            parent_id: None,
            status: safe_cex_engine::model::OrderStatus::Open,
            symbol: "ETHUSDT".into(),
            kind: safe_cex_engine::model::OrderType::Limit,
            side: safe_cex_engine::model::OrderSide::Buy,
            price: 10.0,
            amount: 1.0,
            filled: 0.0,
            remaining: 1.0,
            reduce_only: false,
            close: None,
            client_order_id: None,
            time_in_force: None,
            last_update: 0.0,
            trigger_type: None,
            take_profit: None,
            stop_loss: None,
            extra: None,
        });
    assert!(forward(&mut source, &mut page));
    assert!(btc_place_goes_out(&mut page, 2));
}

/// codex-hcmq3f: the same stale read rows, re-sent in a reconnect snapshot,
/// must not lift the fence the delta that carried them kept.
#[test]
fn a_snapshot_cannot_restamp_the_same_stale_read_rows() {
    let mut source = source_with_orders_read_out();
    let mut page = frame_fed_page(&mut source);
    source.advance(1_000.0);
    page.advance(1_000.0);
    lose_btc_placement(&mut page, 1);
    source.advance(4_000.0);
    page.advance(4_000.0);
    answer_all(&mut source, "/v5/order/realtime", NO_ROWS);
    assert!(forward(&mut source, &mut page));
    assert!(
        !btc_place_goes_out(&mut page, 2),
        "working control: ordinary frame keeps old read fence"
    );
    for snapshot in [
        source.engine.snapshot_frame(source.now).to_vec(),
        source.engine.account_snapshot_frame(source.now).to_vec(),
    ] {
        page.engine.apply_frame(page.now, &snapshot, true).unwrap();
        page.collect();
        assert!(
            !btc_place_goes_out(&mut page, 3),
            "a reconnect snapshot of identical old rows must retain the fence"
        );
    }
}

/// A snapshot from a source that knows no read time for its rows lifts nothing.
#[test]
fn a_snapshot_without_a_read_time_lifts_nothing() {
    let mut source = source_with_orders_read_out();
    let mut page = frame_fed_page(&mut source);
    lose_btc_placement(&mut page, 1);
    source.advance(10_000.0);
    page.advance(10_000.0);
    let snapshot = source.engine.snapshot_frame(source.now).to_vec();
    page.engine.apply_frame(page.now, &snapshot, true).unwrap();
    page.collect();
    assert!(!btc_place_goes_out(&mut page, 2));
}
