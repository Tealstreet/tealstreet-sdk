//! Variant contracts from documented Bybit V5 payloads and Hyro's readiness contract.
mod support;
use safe_cex_engine::{Command, Event};
use safe_cex_venue_bybit::{Endpoints, Options, Variant};
use support::Harness;

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
