//! Variant contracts from documented Bybit V5 payloads and Hyro's readiness contract.
mod support;
use safe_cex_engine::{Command, Event};
use safe_cex_venue_bybit::{Endpoints, Variant};
use support::Harness;
use support::Options;

const EMPTY: &str = r#"{"retCode":0,"retMsg":"OK","result":{"list":[],"nextPageCursor":""}}"#;
const WALLET: &str = r#"{"retCode":0,"retMsg":"OK","result":{"list":[{"accountType":"UNIFIED","totalEquity":"1100","totalWalletBalance":"1000","totalInitialMargin":"10","totalMaintenanceMargin":"1","totalAvailableBalance":"900","coin":[]}]}}"#;
const POSITIONS: &str = r#"{"retCode":0,"retMsg":"OK","result":{"list":[{"symbol":"BTCUSDT","positionIdx":0,"side":"Buy","size":"1","avgPrice":"50000","markPrice":"51000","positionValue":"51000","leverage":"10","unrealisedPnl":"100","curRealisedPnl":"0","positionIM":"10"}],"nextPageCursor":""}}"#;
const INVERSE: &str = r#"{"retCode":0,"retMsg":"OK","result":{"list":[{"symbol":"BTCUSD","positionIdx":0,"side":"Buy","size":"1000","avgPrice":"49000","markPrice":"50000","positionValue":"0.02","leverage":"10","unrealisedPnl":"0.002","curRealisedPnl":"0.001","positionIM":"0.002"}],"nextPageCursor":""}}"#;
const ORDER: &str = r#"{"retCode":0,"retMsg":"OK","result":{"list":[{"symbol":"BTCUSD","orderId":"inverse-order","side":"Buy","orderType":"Limit","orderStatus":"New","price":"50000","qty":"1000","leavesQty":"750","cumExecQty":"250","stopLoss":"40000","takeProfit":"60000"}],"nextPageCursor":""}}"#;

fn options(variant: Variant) -> Options {
    Options {
        variant,
        key: Some("key".into()),
        secret: Some("secret".into()),
        endpoints: variant.is_hyro().then(|| Endpoints {
            rest: "https://hyro.example".into(),
            public_ws: "wss://hyro.example/v5/public/linear".into(),
            private_ws: "wss://hyro.example/v5/private".into(),
        }),
        ..Options::default()
    }
}

fn startup(variant: Variant) -> Harness {
    let mut h = Harness::started(options(variant));
    if h.has_request("/v5/account/fee-rate") {
        h.respond("/v5/account/fee-rate", 200, support::FEE_RATES);
    }
    if h.has_request("/v5/account/info") {
        h.respond("/v5/account/info", 200, support::ACCOUNT_INFO);
    }
    if h.has_request("status=PreLaunch") {
        h.respond("status=PreLaunch", 200, EMPTY);
    }
    let instruments = if variant.is_inverse() {
        support::INSTRUMENTS
            .replace("USDT", "USD")
            .replace("LinearPerpetual", "InversePerpetual")
    } else {
        support::INSTRUMENTS.to_owned()
    };
    h.respond("/v5/market/instruments-info", 200, &instruments);
    let tickers = if variant.is_inverse() {
        support::TICKERS
            .replace("USDT", "USD")
            .replace("66666.60", "50000")
    } else {
        support::TICKERS.to_owned()
    };
    h.respond("/v5/market/tickers", 200, &tickers);
    h
}

fn ack(h: &mut Harness, op: &str) {
    let message = h
        .sent_on("/v5/private")
        .into_iter()
        .rev()
        .find(|text| text.contains(&format!("\"op\":\"{op}\"")))
        .unwrap();
    #[derive(serde::Deserialize)]
    struct Sent {
        req_id: String,
    }
    let message: Sent = serde_json::from_str(&message).unwrap();
    h.message(
        "/v5/private",
        &format!(
            r#"{{"op":"{op}","req_id":"{}","success":true}}"#,
            message.req_id
        ),
    );
}

fn handshake(h: &mut Harness) {
    let socket = h.socket("/v5/private").unwrap();
    h.event(Event::WsOpened { socket });
    ack(h, "auth");
    ack(h, "subscribe");
}

fn hyro_snapshot(h: &mut Harness) {
    h.respond("/v5/order/realtime", 200, EMPTY);
    h.respond("/v5/position/list", 200, POSITIONS);
    h.respond("/v5/account/wallet-balance", 200, WALLET);
}

#[test]
fn inverse_native_quantity_and_coin_position_units() {
    let mut h = startup(Variant::Inverse);
    assert!(h.paths().iter().all(|path| !path.contains("settleCoin")));
    h.respond("/v5/order/realtime", 200, ORDER);
    h.respond("/v5/position/list", 200, INVERSE);
    h.respond("/v5/account/wallet-balance", 200, WALLET);
    let orders = h.engine.store().orders();
    let parent = orders.iter().find(|o| o.id == "inverse-order").unwrap();
    assert_eq!(parent.amount, 1000.0);
    assert_eq!(parent.filled, 250.0);
    assert_eq!(parent.remaining, 0.015);
    assert_eq!(parent.extra.as_ref().unwrap().remaining_native, Some(750.0));
    assert_eq!(
        orders
            .iter()
            .find(|o| o.id.ends_with("__stop_loss"))
            .unwrap()
            .remaining,
        0.01875
    );
    let position = &h.engine.store().positions()[0];
    assert_eq!(position.contracts, 0.02);
    assert_eq!(position.notional, 1000.0);
    assert_eq!(position.unrealized_pnl, 0.002);
    assert_eq!(h.engine.store().balance().upnl, 100.0);
    assert_eq!(h.engine.store().balance().total, 1000.0);
}

#[test]
fn spot_has_no_position_reads_or_derivative_private_rows() {
    let mut h = startup(Variant::Spot);
    assert!(!h.has_request("/v5/position/list"));
    assert!(h.paths().iter().all(|path| !path.contains("settleCoin")));
    h.respond("/v5/order/realtime", 200, EMPTY);
    h.respond("/v5/account/wallet-balance", 200, WALLET);
    assert!(h.engine.store().loaded().positions);
    assert_eq!(h.engine.store().balance().upnl, 0.0);
    assert_eq!(h.engine.store().balance().total, 1100.0);
    let socket = h.socket("/v5/private").unwrap();
    h.event(Event::WsOpened { socket });
    h.message("/v5/private", r#"{"op":"auth","success":true}"#);
    assert!(
        h.sent_on("/v5/private")
            .iter()
            .any(|text| text.contains("order.spot") && !text.contains("position"))
    );
    h.message("/v5/private",r#"{"topic":"order","data":[{"category":"linear","symbol":"BTCUSDT","orderId":"foreign","side":"Buy","orderType":"Limit","orderStatus":"New","price":"50000","qty":"1"}]}"#);
    assert!(h.engine.store().orders().is_empty());
}

#[test]
fn hyro_requires_correlated_acks_then_atomic_reconciliation() {
    let mut h = startup(Variant::Hyrotrader);
    assert!(!h.has_request("/v5/account/info"));
    assert!(!h.has_request("/v5/account/fee-rate"));
    assert!(!h.has_request("/v5/order/realtime"));
    let socket = h.socket("/v5/private").unwrap();
    h.event(Event::WsOpened { socket });
    h.message(
        "/v5/private",
        r#"{"op":"auth","success":true,"req_id":"old-session"}"#,
    );
    assert!(!h.has_request("/v5/order/realtime"));
    ack(&mut h, "auth");
    assert!(!h.has_request("/v5/order/realtime"));
    ack(&mut h, "subscribe");
    h.respond("/v5/order/realtime", 200, EMPTY);
    h.respond("/v5/position/list", 200, POSITIONS);
    assert!(!h.engine.store().loaded().positions);
    assert!(!h.engine.store().loaded().orders);
    h.respond("/v5/account/wallet-balance", 200, WALLET);
    assert!(h.engine.store().loaded().positions);
    assert!(h.engine.store().loaded().orders);
    assert_eq!(h.engine.store().balance().total, 1000.0);
}

#[test]
fn hyro_stream_during_snapshot_requires_fresh_snapshot() {
    let mut h = startup(Variant::Hyrotrader);
    handshake(&mut h);
    h.message("/v5/private", r#"{"topic":"position","data":[]}"#);
    hyro_snapshot(&mut h);
    assert!(!h.engine.store().loaded().positions);
    assert!(h.has_request("/v5/position/list"));
    hyro_snapshot(&mut h);
    assert!(h.engine.store().loaded().positions);
}

#[test]
fn hyro_malformed_snapshot_never_becomes_empty_authority() {
    let mut h = startup(Variant::Hyrotrader);
    handshake(&mut h);
    hyro_snapshot(&mut h);
    h.advance(5000.0);
    h.respond(
        "/v5/order/realtime",
        200,
        r#"{"retCode":0,"retMsg":"OK","result":{}}"#,
    );
    assert_eq!(h.engine.store().positions().len(), 1);
    assert_eq!(h.engine.store().balance().total, 1000.0);
    assert!(!h.closed.is_empty());
}

#[test]
fn hyro_handshake_timeout_reconnects_and_refuses_hedging() {
    let mut h = startup(Variant::Hyrotrader);
    let socket = h.socket("/v5/private").unwrap();
    h.event(Event::WsOpened { socket });
    h.advance(10000.0);
    assert!(h.closed.contains(&socket));
    h.command(Command::SetPositionMode {
        command: 99,
        hedged: true,
        symbol: None,
    });
    assert!(h.result(99).unwrap().is_err());
}

#[test]
fn hyro_stale_session_replies_cannot_commit_after_reconnect() {
    let mut h = startup(Variant::Hyrotrader);
    handshake(&mut h);
    let old_orders = h.take("/v5/order/realtime");
    let old_positions = h.take("/v5/position/list");
    let old_wallet = h.take("/v5/account/wallet-balance");
    let socket = h.socket("/v5/private").unwrap();
    h.event(Event::WsClosed { socket, code: 1006 });
    h.advance(501.0);
    handshake(&mut h);
    h.reply(old_orders.id, 200, EMPTY);
    h.reply(old_positions.id, 200, POSITIONS);
    h.reply(old_wallet.id, 200, WALLET);
    assert!(!h.engine.store().loaded().positions);
    assert!(!h.engine.store().loaded().balance);
    hyro_snapshot(&mut h);
    assert!(h.engine.store().loaded().balance);
}

#[test]
fn hyro_missing_index_is_oneway_and_bad_numbers_preserve_last_good_snapshot() {
    let mut h = startup(Variant::Hyrotrader);
    handshake(&mut h);
    h.respond("/v5/order/realtime", 200, EMPTY);
    h.respond(
        "/v5/position/list",
        200,
        &POSITIONS.replace("\"positionIdx\":0,", ""),
    );
    h.respond("/v5/account/wallet-balance", 200, WALLET);
    assert_eq!(h.engine.store().positions()[0].id, "BTCUSDT");
    h.advance(5000.0);
    h.respond("/v5/order/realtime", 200, EMPTY);
    h.respond(
        "/v5/position/list",
        200,
        &POSITIONS.replace("\"leverage\":\"10\"", "\"leverage\":\"NaN\""),
    );
    assert_eq!(h.engine.store().positions()[0].leverage, 10.0);
    assert!(!h.closed.is_empty());
}

#[test]
fn hyro_bad_order_quantity_does_not_replace_last_good_orders() {
    let mut h = startup(Variant::Hyrotrader);
    handshake(&mut h);
    let good = ORDER.replace("BTCUSD", "BTCUSDT");
    h.respond("/v5/order/realtime", 200, &good);
    h.respond("/v5/position/list", 200, POSITIONS);
    h.respond("/v5/account/wallet-balance", 200, WALLET);
    assert!(h.engine.store().order("inverse-order").is_some());
    h.advance(5000.0);
    h.respond(
        "/v5/order/realtime",
        200,
        &good.replace("\"qty\":\"1000\"", "\"qty\":\"oops\""),
    );
    assert_eq!(
        h.engine.store().order("inverse-order").unwrap().amount,
        1000.0
    );
    assert!(!h.closed.is_empty());
}

#[test]
fn hyro_bad_order_event_preserves_orders_and_reconnects() {
    let mut h = startup(Variant::Hyrotrader);
    handshake(&mut h);
    hyro_snapshot(&mut h);
    h.message("/v5/private",r#"{"topic":"order","data":[{"symbol":"BTCUSDT","orderId":"bad","side":"Buy","orderType":"Limit","orderStatus":"New","price":"50000","qty":"NaN"}]}"#);
    assert!(h.engine.store().orders().is_empty());
    assert!(!h.closed.is_empty());
}

#[test]
fn inverse_remaining_uses_eight_decimals_and_retains_native_without_price() {
    let mut h = startup(Variant::Inverse);
    let orders = ORDER
        .replace("\"qty\":\"1000\"", "\"qty\":\"200\"")
        .replace("\"leavesQty\":\"750\"", "\"leavesQty\":\"200\"")
        .replace("\"price\":\"50000\"", "\"price\":\"90500\"");
    h.respond("/v5/order/realtime", 200, &orders);
    assert_eq!(
        h.engine.store().order("inverse-order").unwrap().remaining,
        0.00220994
    );
    let socket = h.socket("/v5/private").unwrap();
    h.event(Event::WsOpened { socket });
    h.message("/v5/private", r#"{"topic":"order.inverse","data":[{"category":"inverse","symbol":"BTCUSD","orderId":"no-price","side":"Buy","orderType":"Market","orderStatus":"New","price":"0","qty":"200","leavesQty":"200"}]}"#);
    assert_eq!(h.engine.store().order("no-price").unwrap().remaining, 200.0);
}

#[test]
fn spot_documented_tpsl_order_uses_direction_for_protective_kind() {
    let mut h = startup(Variant::Spot);
    let socket = h.socket("/v5/private").unwrap();
    h.event(Event::WsOpened { socket });
    h.message("/v5/private",r#"{"topic":"order.spot","data":[{"category":"spot","symbol":"BTCUSDT","orderId":"spot-sl","side":"Sell","orderType":"Market","stopOrderType":"tpslOrder","orderStatus":"Untriggered","price":"0","triggerPrice":"70000","triggerDirection":2,"qty":"1","leavesQty":"1"},{"category":"spot","symbol":"BTCUSDT","orderId":"spot-terminal","side":"Buy","orderType":"Limit","orderStatus":"PartiallyFilledCanceled","price":"50000","qty":"1","cumExecQty":"0.5","leavesQty":"0.5"}]}"#);
    assert_eq!(
        h.engine.store().order("spot-sl").unwrap().kind,
        safe_cex_engine::model::OrderType::StopLoss
    );
    assert_eq!(
        h.engine.store().order("spot-terminal").unwrap().status,
        safe_cex_engine::model::OrderStatus::Canceled
    );
}

fn emitted_fills(h: &mut Harness) -> Vec<(f64, f64)> {
    let Some(frame) = h.engine.take_frame(h.now) else {
        return Vec::new();
    };
    let Some((_, payload)) = safe_cex_engine::wire::sections(frame).find(|(kind, _)| *kind == 11)
    else {
        return Vec::new();
    };
    let count = u32::from_le_bytes(payload[..4].try_into().unwrap()) as usize;
    let offset = (4 + count * 12 + 7) & !7;
    (0..count)
        .map(|index| {
            let at = offset + index * 16;
            (
                f64::from_le_bytes(payload[at..at + 8].try_into().unwrap()),
                f64::from_le_bytes(payload[at + 8..at + 16].try_into().unwrap()),
            )
        })
        .collect()
}

fn hyro_fill_frame(status: &str, quantity: &str, value: &str) -> String {
    format!(
        r#"{{"topic":"order","data":[{{"category":"linear","symbol":"BTCUSDT","orderId":"delta-fill","side":"Buy","orderType":"Limit","orderStatus":"{status}","price":"120","qty":"1","cumExecQty":"{quantity}","cumExecValue":"{value}","avgPrice":"106","leavesQty":"0"}}]}}"#
    )
}

#[test]
fn hyro_order_fill_deltas_use_executed_value_and_ignore_duplicates() {
    let mut h = startup(Variant::Hyrotrader);
    handshake(&mut h);
    hyro_snapshot(&mut h);
    emitted_fills(&mut h);
    let partial = hyro_fill_frame("PartiallyFilled", "0.4", "40");
    h.message("/v5/private", &partial);
    assert_eq!(emitted_fills(&mut h), vec![(100.0, 0.4)]);
    h.message("/v5/private", &partial);
    assert!(emitted_fills(&mut h).is_empty());
    h.message("/v5/private", &hyro_fill_frame("Filled", "1", "106"));
    assert_eq!(emitted_fills(&mut h), vec![(110.0, 0.6)]);
    h.message("/v5/private", &partial);
    assert!(emitted_fills(&mut h).is_empty());
    assert_eq!(
        h.engine.store().order("delta-fill").unwrap().status,
        safe_cex_engine::model::OrderStatus::Closed
    );
    h.message("/v5/private",r#"{"topic":"execution","data":[{"category":"linear","symbol":"BTCUSDT","orderId":"delta-fill","side":"Buy","execQty":"1","execPrice":"106","execType":"Trade"}]}"#);
    assert!(emitted_fills(&mut h).is_empty());
}

#[test]
fn cumulative_preparation_hyro_unpriced_report_keeps_corrected_fill_eligible() {
    let mut h = startup(Variant::Hyrotrader);
    handshake(&mut h);
    hyro_snapshot(&mut h);
    emitted_fills(&mut h);
    let unpriced = hyro_fill_frame("PartiallyFilled", "0.4", "0")
        .replace("\"price\":\"120\"", "\"price\":\"0\"")
        .replace("\"avgPrice\":\"106\"", "\"avgPrice\":\"0\"");
    h.message("/v5/private", &unpriced);
    assert!(emitted_fills(&mut h).is_empty());
    let corrected = hyro_fill_frame("PartiallyFilled", "0.4", "40");
    h.message("/v5/private", &corrected);
    assert_eq!(emitted_fills(&mut h), vec![(100.0, 0.4)]);
    h.message("/v5/private", &corrected);
    assert!(emitted_fills(&mut h).is_empty());
    h.message("/v5/private", &hyro_fill_frame("Filled", "1", "106"));
    assert_eq!(emitted_fills(&mut h), vec![(110.0, 0.6)]);
}

#[test]
fn hyro_reconnect_snapshot_seeds_prior_fills_without_reemitting() {
    let mut h = startup(Variant::Hyrotrader);
    handshake(&mut h);
    hyro_snapshot(&mut h);
    emitted_fills(&mut h);
    let socket = h.socket("/v5/private").unwrap();
    h.event(Event::WsClosed { socket, code: 1006 });
    h.advance(501.0);
    handshake(&mut h);
    h.respond("/v5/order/realtime",200,r#"{"retCode":0,"result":{"list":[{"symbol":"BTCUSDT","orderId":"delta-fill","side":"Buy","orderType":"Limit","orderStatus":"PartiallyFilled","price":"120","qty":"1","cumExecQty":"0.4","cumExecValue":"40","avgPrice":"100","leavesQty":"0.6"}],"nextPageCursor":""}}"#);
    h.respond("/v5/position/list", 200, POSITIONS);
    h.respond("/v5/account/wallet-balance", 200, WALLET);
    assert!(emitted_fills(&mut h).is_empty());
    h.message(
        "/v5/private",
        &hyro_fill_frame("PartiallyFilled", "0.4", "40"),
    );
    assert!(emitted_fills(&mut h).is_empty());
    h.message("/v5/private", &hyro_fill_frame("Filled", "1", "106"));
    assert_eq!(emitted_fills(&mut h), vec![(110.0, 0.6)]);
}

#[test]
fn hyro_regressing_fill_value_does_not_invent_a_fill() {
    let mut h = startup(Variant::Hyrotrader);
    handshake(&mut h);
    hyro_snapshot(&mut h);
    emitted_fills(&mut h);
    h.message(
        "/v5/private",
        &hyro_fill_frame("PartiallyFilled", "0.4", "40"),
    );
    emitted_fills(&mut h);
    h.message(
        "/v5/private",
        &hyro_fill_frame("PartiallyFilled", "0.5", "20"),
    );
    assert!(emitted_fills(&mut h).is_empty());
    assert!(!h.closed.is_empty());
    assert_eq!(h.engine.store().order("delta-fill").unwrap().filled, 0.4);
}

#[test]
fn cumulative_seed_hyro_negative_quote_retains_existing_regression_recovery() {
    let mut h = startup(Variant::Hyrotrader);
    handshake(&mut h);
    hyro_snapshot(&mut h);
    emitted_fills(&mut h);
    h.message(
        "/v5/private",
        &hyro_fill_frame("PartiallyFilled", "0.4", "40"),
    );
    assert_eq!(emitted_fills(&mut h), vec![(100.0, 0.4)]);
    h.message(
        "/v5/private",
        &hyro_fill_frame("PartiallyFilled", "0.5", "-1"),
    );
    assert!(emitted_fills(&mut h).is_empty());
    assert!(!h.closed.is_empty());
    assert_eq!(h.engine.store().order("delta-fill").unwrap().filled, 0.4);
}

#[test]
fn cumulative_seed_hyro_unavailable_average_does_not_invent_zero_quote() {
    let mut h = startup(Variant::Hyrotrader);
    handshake(&mut h);
    h.respond("/v5/order/realtime", 200, r#"{"retCode":0,"result":{"list":[{"symbol":"BTCUSDT","orderId":"delta-fill","side":"Buy","orderType":"Limit","orderStatus":"PartiallyFilled","price":"120","qty":"1","cumExecQty":"0.4","avgPrice":"0","leavesQty":"0.6"}],"nextPageCursor":""}}"#);
    h.respond("/v5/position/list", 200, POSITIONS);
    h.respond("/v5/account/wallet-balance", 200, WALLET);
    assert!(emitted_fills(&mut h).is_empty());
    h.message("/v5/private", &hyro_fill_frame("Filled", "1", "106"));
    assert_eq!(emitted_fills(&mut h), vec![(106.0, 0.6)]);
}

#[test]
fn hyro_periodic_snapshot_does_not_consume_delayed_fill() {
    let mut h = startup(Variant::Hyrotrader);
    handshake(&mut h);
    hyro_snapshot(&mut h);
    emitted_fills(&mut h);
    h.message("/v5/private", &hyro_fill_frame("New", "0", "0"));
    emitted_fills(&mut h);
    h.advance(5000.0);
    h.respond("/v5/order/realtime",200,r#"{"retCode":0,"result":{"list":[{"symbol":"BTCUSDT","orderId":"delta-fill","side":"Buy","orderType":"Limit","orderStatus":"PartiallyFilled","price":"120","qty":"1","cumExecQty":"0.4","cumExecValue":"40","avgPrice":"100","leavesQty":"0.6"}],"nextPageCursor":""}}"#);
    h.respond("/v5/position/list", 200, POSITIONS);
    h.respond("/v5/account/wallet-balance", 200, WALLET);
    h.message(
        "/v5/private",
        &hyro_fill_frame("PartiallyFilled", "0.4", "40"),
    );
    assert_eq!(emitted_fills(&mut h), vec![(100.0, 0.4)]);
}

#[test]
fn hyro_recovery_terminal_snapshot_seeds_fill_cursor() {
    let mut h = startup(Variant::Hyrotrader);
    handshake(&mut h);
    h.respond("/v5/order/realtime",200,r#"{"retCode":0,"result":{"list":[{"symbol":"BTCUSDT","orderId":"delta-fill","side":"Buy","orderType":"Limit","orderStatus":"Filled","price":"120","qty":"1","cumExecQty":"1","cumExecValue":"106","avgPrice":"106","leavesQty":"0"}],"nextPageCursor":""}}"#);
    h.respond("/v5/position/list", 200, POSITIONS);
    h.respond("/v5/account/wallet-balance", 200, WALLET);
    emitted_fills(&mut h);
    h.message("/v5/private", &hyro_fill_frame("Filled", "1", "106"));
    assert!(emitted_fills(&mut h).is_empty());
}

#[test]
fn hyro_periodic_absent_order_keeps_partial_fill_cursor() {
    let mut h = startup(Variant::Hyrotrader);
    handshake(&mut h);
    hyro_snapshot(&mut h);
    emitted_fills(&mut h);
    h.message(
        "/v5/private",
        &hyro_fill_frame("PartiallyFilled", "0.4", "40"),
    );
    assert_eq!(emitted_fills(&mut h), vec![(100.0, 0.4)]);
    h.advance(5000.0);
    hyro_snapshot(&mut h);
    emitted_fills(&mut h);
    h.message("/v5/private", &hyro_fill_frame("Filled", "1", "106"));
    assert_eq!(emitted_fills(&mut h), vec![(110.0, 0.6)]);
}

#[test]
fn request_parity_spot_reads_account_metadata_before_private_snapshots() {
    let mut h = Harness::started(options(Variant::Spot));
    let metadata = h.take("/v5/account/info");
    assert!(!metadata.url.contains("category="));
    assert!(!h.has_request("/v5/position/list"));
    assert!(!h.has_request("/v5/order/realtime"));
    h.reply(metadata.id, 200, support::ACCOUNT_INFO);
    h.respond("/v5/market/instruments-info", 200, support::INSTRUMENTS);
    h.respond("/v5/market/tickers", 200, support::TICKERS);
    assert!(h.has_request("/v5/order/realtime?category=spot"));
    assert!(h.has_request("/v5/account/wallet-balance?accountType=UNIFIED"));
    assert!(!h.has_request("/v5/position/list"));
}

#[test]
fn request_parity_hyro_position_pages_use_its_fifty_row_limit() {
    let mut h = startup(Variant::Hyrotrader);
    handshake(&mut h);
    let first = h.take("/v5/position/list");
    assert!(
        first
            .url
            .contains("category=linear&settleCoin=USDT&limit=50&cursor=")
    );
    h.reply(
        first.id,
        200,
        &POSITIONS.replace("\"nextPageCursor\":\"\"", "\"nextPageCursor\":\"next\""),
    );
    let next = h.take("/v5/position/list");
    assert!(next.url.contains("limit=50&cursor=next"));
    assert!(!h.has_request("/v5/account/info"));
}

// https://bybit-exchange.github.io/docs/v5/websocket/private/execution
#[test]
fn execution_id_dedupes_batches_repeated_frames_and_reconnects() {
    for variant in [
        Variant::Linear,
        Variant::Demo,
        Variant::Inverse,
        Variant::Spot,
    ] {
        let mut h = startup(variant);
        emitted_fills(&mut h);
        let category = if variant.is_inverse() {
            "inverse"
        } else if variant.is_spot() {
            "spot"
        } else {
            "linear"
        };
        let symbol = if variant.is_inverse() {
            "BTCUSD"
        } else {
            "BTCUSDT"
        };
        let row = |id: &str| {
            format!(
                r#"{{"category":"{category}","symbol":"{symbol}","orderId":"execution-order","execId":"{id}","side":"Buy","execQty":"2","execPrice":"50000","execType":"Trade"}}"#
            )
        };
        let packet = format!(
            r#"{{"topic":"execution","data":[{},{},{}]}}"#,
            row("exec-a"),
            row("exec-a"),
            row("exec-b")
        );
        h.message("/v5/private", &packet);
        assert_eq!(
            emitted_fills(&mut h),
            vec![(50000.0, 2.0), (50000.0, 2.0)],
            "{variant:?}: exec ID, not same price/qty/order, identifies fills"
        );
        h.message("/v5/private", &packet);
        assert!(
            emitted_fills(&mut h).is_empty(),
            "{variant:?}: replayed packet"
        );
        let old = h.socket("/v5/private").unwrap();
        h.event(Event::WsClosed {
            socket: old,
            code: 1006,
        });
        h.advance(2000.0);
        let new = h.socket("/v5/private").unwrap();
        assert_ne!(old, new);
        h.event(Event::WsOpened { socket: new });
        h.message("/v5/private", &packet);
        assert!(
            emitted_fills(&mut h).is_empty(),
            "{variant:?}: reconnect replay"
        );
        h.message(
            "/v5/private",
            &format!(r#"{{"topic":"execution","data":[{}]}}"#, row("exec-c")),
        );
        assert_eq!(emitted_fills(&mut h), vec![(50000.0, 2.0)]);
    }
}

#[test]
fn malformed_execution_does_not_consume_identity_and_missing_ids_stay_distinct() {
    let mut h = startup(Variant::Linear);
    emitted_fills(&mut h);
    let row = r#"{"topic":"execution","data":[{"category":"linear","symbol":"BTCUSDT","orderId":"execution-order","execId":"corrected","side":"Buy","execQty":"2","execPrice":"50000","execType":"Trade"}]}"#;
    for (from, to) in [
        ("50000", "0"),
        ("50000", "NaN"),
        ("50000", "inf"),
        (r#""execQty":"2""#, r#""execQty":"NaN""#),
        (r#""side":"Buy""#, r#""side":"unknown""#),
        (r#""execType":"Trade""#, r#""execType":"Funding""#),
    ] {
        h.message("/v5/private", &row.replace(from, to));
        assert!(emitted_fills(&mut h).is_empty());
    }
    h.message("/v5/private", row);
    assert_eq!(emitted_fills(&mut h), vec![(50000.0, 2.0)]);
    h.message("/v5/private", row);
    assert!(emitted_fills(&mut h).is_empty());
    let idless = row.replace(r#""execId":"corrected","#, "");
    h.message("/v5/private", &idless);
    h.message("/v5/private", &idless);
    assert_eq!(
        emitted_fills(&mut h),
        vec![(50000.0, 2.0), (50000.0, 2.0)],
        "missing execution IDs cannot safely use economic fields as identity"
    );
}

#[test]
fn deduplicated_execution_retains_native_order_identity_in_fill_frame() {
    let mut h = startup(Variant::Inverse);
    emitted_fills(&mut h);
    let row = r#"{"topic":"execution.inverse","data":[{"category":"inverse","symbol":"BTCUSD","orderId":"native-order-id","orderLinkId":"client-link-id","execId":"execution-id","side":"Sell","execQty":"750","execPrice":"50000","execType":"Trade"}]}"#;
    h.message("/v5/private", row);
    h.message("/v5/private", row);
    let frame = h.engine.take_frame(h.now).unwrap();
    let fills = safe_cex_engine::wire::sections(frame)
        .find(|(kind, _)| *kind == 11)
        .unwrap()
        .1;
    let word = |payload: &[u8], at: usize| {
        u32::from_le_bytes(payload[at..at + 4].try_into().unwrap()) as usize
    };
    assert_eq!(word(fills, 0), 1);
    let id = word(fills, 4);
    let strings = safe_cex_engine::wire::sections(frame)
        .find(|(kind, _)| *kind == 1)
        .unwrap()
        .1;
    let base = 4 + (word(strings, 0) + 1) * 4;
    let start = base + word(strings, 4 + id * 4);
    let end = base + word(strings, 8 + id * 4);
    assert_eq!(&strings[start..end], b"native-order-id");
    let number = |at| f64::from_le_bytes(fills[at..at + 8].try_into().unwrap());
    assert_eq!(
        (number(16), number(24)),
        (50000.0, 750.0),
        "inverse fills stay in native contract units"
    );
}

#[test]
fn hyro_transient_periodic_read_failure_keeps_the_ready_stream_and_last_good_state() {
    let mut h = startup(Variant::Hyrotrader);
    handshake(&mut h);
    hyro_snapshot(&mut h);
    let socket = h.socket("/v5/private").unwrap();
    for failure in [
        "upstream timeout",
        r#"{"retCode":10016,"retMsg":"Server error"}"#,
    ] {
        h.advance(5000.0);
        h.respond("/v5/order/realtime", 502, failure);
        h.respond("/v5/position/list", 200, POSITIONS);
        h.respond("/v5/account/wallet-balance", 200, WALLET);
        assert!(
            !h.closed.contains(&socket),
            "a transient poll failure tore down the stream"
        );
        assert_eq!(h.socket("/v5/private"), Some(socket));
        assert_eq!(h.engine.store().positions().len(), 1);
        assert_eq!(h.engine.store().balance().total, 1000.0);
    }
    // The next tick repairs.
    h.advance(5000.0);
    hyro_snapshot(&mut h);
    assert!(!h.closed.contains(&socket));
    assert!(h.engine.store().loaded().orders);
}

#[test]
fn hyro_initial_snapshot_transport_failure_still_recovers_the_session() {
    let mut h = startup(Variant::Hyrotrader);
    handshake(&mut h);
    h.respond("/v5/order/realtime", 502, "upstream timeout");
    assert!(!h.closed.is_empty());
}

#[test]
fn hyro_reads_orphaned_by_a_transient_failure_cannot_fill_the_next_snapshot() {
    let mut h = startup(Variant::Hyrotrader);
    handshake(&mut h);
    let private = h.socket("/v5/private").unwrap();
    hyro_snapshot(&mut h);
    h.advance(5000.0);
    let old_orders = h.take("/v5/order/realtime");
    let old_positions = h.take("/v5/position/list");
    h.respond("/v5/account/wallet-balance", 502, "upstream timeout");
    h.message(
        "/v5/private",
        r#"{"topic":"position","data":[{"symbol":"BTCUSDT","positionIdx":0,"side":"Buy","size":"3","avgPrice":"50000","markPrice":"51000","positionValue":"153000","leverage":"10","unrealisedPnl":"300","curRealisedPnl":"0","positionIM":"30"}]}"#,
    );
    assert_eq!(h.engine.store().positions()[0].contracts, 3.0);
    h.advance(5000.0);
    h.reply(old_orders.id, 200, EMPTY);
    h.reply(old_positions.id, 200, POSITIONS);
    h.respond("/v5/account/wallet-balance", 200, WALLET);
    assert_eq!(
        h.engine.store().positions()[0].contracts,
        3.0,
        "a read issued before the stream event overwrote it"
    );
    assert!(!h.closed.contains(&private));
}

#[test]
fn hyro_streamed_venue_refusal_is_transient_but_repeated_failures_reconnect() {
    let mut h = startup(Variant::Hyrotrader);
    handshake(&mut h);
    let private = h.socket("/v5/private").unwrap();
    hyro_snapshot(&mut h);
    let refusal =
        r#"{"retCode":10006,"retMsg":"Too many visits!","result":{},"retExtInfo":{},"time":1}"#;
    for _ in 0..3 {
        h.advance(5000.0);
        // "Connect": the private pong keeps the healthy stream alive.
        h.message(
            "/v5/private",
            r#"{"req_id":"","op":"pong","args":["1675418560633"],"conn_id":"cfcb4ocsvfriu23r3er0-1b"}"#,
        );
        h.respond("/v5/order/realtime", 200, refusal);
        assert!(
            !h.closed.contains(&private),
            "a venue refusal tore down the stream"
        );
        h.requests.clear();
    }
    h.advance(5000.0);
    h.respond("/v5/order/realtime", 200, refusal);
    assert!(
        h.closed.contains(&private),
        "persistent failures must recover the session"
    );
}

/// Every closed socket is a public one the supervisor dropped for idling (code 4000):
/// the harness feeds no public traffic, so those closes are expected.
fn only_public_idle_closes(h: &Harness) {
    for closed in &h.closed {
        let url = &h.sockets.iter().find(|(s, _)| s == closed).unwrap().1;
        assert!(url.contains("/v5/public"), "unexpected close of {url}");
    }
    let closes = h
        .logs
        .iter()
        .filter(|m| m.contains("Bybit socket closed"))
        .count();
    let idle = h
        .logs
        .iter()
        .filter(|m| m.contains("Bybit socket closed (4000)"))
        .count();
    assert_eq!(
        closes, idle,
        "a close other than the idle timeout: {:?}",
        h.logs
    );
}

#[test]
fn hyro_steady_stream_frames_never_turn_reconciliation_into_a_reconnect_loop() {
    let mut h = startup(Variant::Hyrotrader);
    handshake(&mut h);
    hyro_snapshot(&mut h);
    let socket = h.socket("/v5/private").unwrap();
    let richer = WALLET.replace("\"totalEquity\":\"1100\"", "\"totalEquity\":\"1600\"");
    // Every periodic read is overtaken by a live position frame.
    for _ in 0..6 {
        h.advance(5000.0);
        while h.has_request("/v5/order/realtime") {
            h.message("/v5/private", r#"{"topic":"position","data":[]}"#);
            h.respond("/v5/order/realtime", 200, EMPTY);
            h.respond("/v5/position/list", 200, POSITIONS);
            h.respond("/v5/account/wallet-balance", 200, &richer);
        }
    }
    // The harness feeds no public traffic, so only the private stream is in question.
    assert!(
        !h.closed.contains(&socket),
        "steady frames forced a reconnect"
    );
    only_public_idle_closes(&h);
    assert_eq!(h.socket("/v5/private"), Some(socket));
    assert!(
        h.engine.store().balance().total > 1000.0,
        "the unfenced wallet still refreshed"
    );
}

#[test]
fn hyro_steady_frames_during_initial_recovery_still_become_ready_without_reconnecting() {
    let mut h = startup(Variant::Hyrotrader);
    handshake(&mut h);
    // Three reads in a row are overtaken; the third is admitted.
    for _ in 0..3 {
        h.message("/v5/private", r#"{"topic":"position","data":[]}"#);
        hyro_snapshot(&mut h);
    }
    assert!(
        h.closed.is_empty(),
        "steady frames during recovery forced a reconnect"
    );
    assert!(h.engine.store().loaded().positions && h.engine.store().loaded().orders);
    assert!(
        h.logs
            .iter()
            .any(|m| m.contains("kept moving during reconciliation"))
    );
}

#[test]
fn hyro_overtaken_but_successful_reads_reset_the_transient_streak() {
    let mut h = startup(Variant::Hyrotrader);
    handshake(&mut h);
    hyro_snapshot(&mut h);
    let private = h.socket("/v5/private").unwrap();
    let refusal =
        r#"{"retCode":10006,"retMsg":"Too many visits!","result":{},"retExtInfo":{},"time":1}"#;
    for _ in 0..5 {
        h.advance(5000.0);
        h.respond("/v5/order/realtime", 200, refusal);
        h.requests.clear();
        // Successful reads overtaken by frames are admitted and clear the streak.
        h.advance(5000.0);
        for _ in 0..3 {
            h.message("/v5/private", r#"{"topic":"position","data":[]}"#);
            hyro_snapshot(&mut h);
        }
        h.requests.clear();
    }
    assert!(
        !h.closed.contains(&private),
        "spaced-out transient blips accumulated into a reconnect"
    );
    only_public_idle_closes(&h);
}

#[test]
fn capability_profile_spot_does_not_advertise_income() {
    assert_no_income_capability(Variant::Spot);
}

#[test]
fn capability_profile_hyro_does_not_advertise_income() {
    assert_no_income_capability(Variant::Hyrotrader);
}

fn assert_no_income_capability(variant: Variant) {
    let mut h = Harness::started(options(variant));
    assert!(!h.engine.capabilities().income_history, "{variant:?}");
    h.requests.clear();
    h.command(Command::Query {
        command: 971,
        query: safe_cex_engine::Query::Income {
            symbol: None,
            kind: None,
            from_ms: None,
            to_ms: None,
            limit: Some(1),
        },
    });
    assert!(h.requests.is_empty(), "refusing variant issued no HTTP");
    assert_eq!(
        h.result(971).unwrap().as_ref().unwrap_err(),
        "This Bybit variant does not support income history"
    );
}

#[test]
fn capability_profile_linear_inverse_demo_keep_income_advertised() {
    for variant in [Variant::Linear, Variant::Inverse, Variant::Demo] {
        let h = Harness::started(options(variant));
        assert!(h.engine.capabilities().income_history, "{variant:?}");
    }
}

/// fe7lgm (#1971): a read the engine refused to admit never reached Hyro, so
/// local owner-cap pressure cannot spend the transient streak and tear down
/// a healthy private session; only venue and transport transients count.
#[test]
fn hyro_local_admission_refusals_never_spend_the_transient_streak() {
    let mut h = startup(Variant::Hyrotrader);
    handshake(&mut h);
    let private = h.socket("/v5/private").unwrap();
    hyro_snapshot(&mut h);
    h.advance(1.0);
    // Park caller candle reads (20 s windows) until admission refuses.
    for command in 2_000..2_300 {
        h.command(Command::FetchCandles {
            command,
            symbol: "BTCUSDT".into(),
            interval: safe_cex_engine::model::Interval::M1,
            limit: Some(2),
            from_ms: None,
            to_ms: None,
        });
    }
    h.requests.clear();
    for tick in 0..4 {
        h.advance(5000.0);
        h.message(
            "/v5/private",
            r#"{"req_id":"","op":"pong","args":["1675418560633"],"conn_id":"cfcb4ocsvfriu23r3er0-1b"}"#,
        );
        assert!(
            !h.has_request("/v5/order/realtime"),
            "tick {tick}: the snapshot read was refused locally"
        );
        assert!(
            !h.closed.contains(&private),
            "tick {tick}: a local refusal tore down the stream"
        );
    }
}
