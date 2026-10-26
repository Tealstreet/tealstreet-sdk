// Null protection values are documented in the BloFin REST algo/TPSL examples.
// The captured public tape currently contains no nulls; required-field nulls
// below are defensive corruption cases, not claims about captured live traffic.
mod support;
use safe_cex_engine::{Action, Event, Notice};
use support::*;

fn two_instruments() -> String {
    let mut instruments: serde_json::Value = serde_json::from_str(INSTRUMENT).unwrap();
    let mut eth = instruments["data"][0].clone();
    eth["instId"] = "ETH-USDT".into();
    eth["baseCurrency"] = "ETH".into();
    instruments["data"].as_array_mut().unwrap().push(eth);
    instruments.to_string()
}

#[test]
fn documented_null_protection_fields_do_not_reject_rest_order_snapshot() {
    let mut h = Harness::new();
    h.modes();
    h.reply("orders-pending", r#"{"code":"0","data":[{"instId":"BTC-USDT","orderCategory":"normal","orderId":"123","side":"buy","size":"1","price":"60000","state":"live","clientOrderId":null,"attachAlgoOrders":null,"tpTriggerPrice":null,"slTriggerPrice":null,"tpOrderPrice":null,"slOrderPrice":null,"tpTriggerPriceType":null,"slTriggerPriceType":null}]}"#);
    for path in [
        "orders-pending",
        "orders-tpsl-pending",
        "orders-algo-pending",
    ] {
        h.reply(path, r#"{"code":"0","data":[]}"#);
    }
    assert!(h.e.store().loaded().orders);
    assert_eq!(h.e.store().orders().len(), 1);
    assert_eq!(h.e.store().orders()[0].id, "123");
    assert_eq!(h.e.store().orders()[0].client_order_id, None);
}

#[test]
fn documented_null_unused_tpsl_fields_preserve_active_protection() {
    let mut h = Harness::new();
    h.seed();
    h.ws(r#"{"arg":{"channel":"orders-algo"},"data":[{"instId":"BTC-USDT","tpslId":"700","orderType":null,"triggerPrice":null,"orderPrice":null,"side":"sell","size":"-1","tpTriggerPrice":"65000","tpOrderPrice":null,"slTriggerPrice":null,"slOrderPrice":null,"slTriggerPriceType":null,"state":"live"}]}"#);
    assert_eq!(h.e.store().orders().len(), 1);
    let order = &h.e.store().orders()[0];
    assert_eq!(order.id, "700__take_profit");
    assert_eq!(order.price, 65000.0);
}

#[test]
fn null_position_metadata_reuses_known_leverage_and_liquidation_price() {
    let mut h = Harness::new();
    h.seed();
    let row = POSITION
        .replace("\"leverage\":\"20\"", "\"leverage\":null")
        .replace(
            "\"liquidationPrice\":\"26914.9\"",
            "\"liquidationPrice\":null",
        )
        .replace("\"positions\":\"4\"", "\"positions\":\"5\"");
    h.ws(&format!(
        r#"{{"arg":{{"channel":"positions"}},"data":[{row}]}}"#
    ));
    let position = &h.e.store().positions()[0];
    assert_eq!(position.contracts, 0.005);
    assert_eq!(position.leverage, 20.0);
    assert_eq!(position.liquidation_price, 26914.9);
}

#[test]
fn malformed_ws_order_row_retains_known_order_and_processes_valid_sibling() {
    let mut h = Harness::new();
    h.seed();
    let good = r#"{"instId":"BTC-USDT","orderCategory":"normal","orderId":"123","side":"buy","size":"1","price":"60000","state":"live"}"#;
    h.ws(&format!(
        r#"{{"arg":{{"channel":"orders"}},"data":[{good}]}}"#
    ));
    let bad = good.replace("\"size\":\"1\"", "\"size\":null");
    let sibling = good.replace("\"123\"", "\"456\"");
    h.ws(&format!(
        r#"{{"arg":{{"channel":"orders"}},"data":[{bad},{sibling}]}}"#
    ));
    assert_eq!(h.e.store().orders().len(), 2);
    assert_eq!(h.e.store().order("123").unwrap().amount, 0.001);
    assert!(h.e.store().order("456").is_some());
    assert!(h.actions.iter().any(|a| matches!(a, Action::Notice(Notice::Event(event)) if event.message.contains("Malformed BloFin order row"))));
}

#[test]
fn malformed_ws_position_row_preserves_its_symbol_and_totals_while_processing_valid_sibling() {
    let mut h = Harness::with_instruments(&two_instruments());
    h.seed();
    let old_balance = h.e.store().balance().clone();
    let bad = POSITION.replace("\"positions\":\"4\"", "\"positions\":null");
    let good = POSITION.replace("BTC-USDT", "ETH-USDT");
    h.ws(&format!(
        r#"{{"arg":{{"channel":"positions"}},"data":[{bad},{good}]}}"#
    ));
    assert_eq!(h.e.store().positions().len(), 2);
    assert_eq!(h.e.store().position("BTCUSDT").unwrap().contracts, 0.004);
    assert_eq!(h.e.store().position("ETHUSDT").unwrap().contracts, 0.004);
    assert_eq!(*h.e.store().balance(), old_balance);
}

#[test]
fn malformed_rest_required_quantity_preserves_last_good_position_snapshot() {
    let mut h = Harness::new();
    h.seed();
    h.handle(10_000.0, Event::TimerFired { timer: h.tick });
    h.collect();
    let bad = POSITION.replace("\"positions\":\"4\"", "\"positions\":null");
    h.reply(
        "account/positions",
        &format!(r#"{{"code":"0","data":[{bad}]}}"#),
    );
    assert_eq!(h.e.store().positions()[0].contracts, 0.004);
    assert_eq!(h.e.store().balance().used, 0.72);
    assert!(
        !h.pending
            .iter()
            .any(|request| request.url.contains("balances"))
    );
}

#[test]
fn malformed_ws_ticker_row_retains_known_ticker_and_processes_valid_sibling() {
    let mut h = Harness::with_instruments(&two_instruments());
    let socket = h
        .actions
        .iter()
        .find_map(|action| match action {
            Action::WsConnect { socket, url, .. } if url.ends_with("/public") => Some(*socket),
            _ => None,
        })
        .unwrap();
    h.handle(1.0, Event::WsMessage {
        socket,
        text: r#"{"arg":{"channel":"tickers"},"data":[{"instId":"BTC-USDT","last":null},{"instId":"ETH-USDT","last":"3000","open24h":"2500","bidPrice":"2999","askPrice":"3001"}]}"#,
    });
    h.collect();
    assert_eq!(h.e.store().ticker("BTCUSDT").unwrap().last, 60000.0);
    assert_eq!(h.e.store().ticker("ETHUSDT").unwrap().last, 3000.0);
    assert!(h.actions.iter().any(|action| matches!(action,
        Action::Notice(Notice::Event(event)) if event.message.contains("Malformed BloFin ticker row"))));
}

#[test]
fn valid_unloaded_position_rows_still_contribute_to_account_totals() {
    let mut h = Harness::new();
    h.seed();
    let unloaded = POSITION.replace("BTC-USDT", "UNLOADED-USDT");
    h.ws(&format!(
        r#"{{"arg":{{"channel":"positions"}},"data":[{POSITION},{unloaded}]}}"#
    ));
    assert_eq!(h.e.store().positions().len(), 1);
    assert_eq!(h.e.store().balance().used, 1.44);
    assert_eq!(h.e.store().balance().upnl, 1.0);
}

#[test]
fn websocket_balance_pnl_rounds_raw_sum_to_cents_not_market_price_precision() {
    let instruments = INSTRUMENT.replace("\"tickSize\":\"0.1\"", "\"tickSize\":\"1\"");
    let mut h = Harness::with_instruments(&instruments);
    h.seed();
    let row = POSITION.replace("\"unrealizedPnl\":\"0.5\"", "\"unrealizedPnl\":\"1.23\"");
    h.ws(&format!(
        r#"{{"arg":{{"channel":"positions"}},"data":[{row}]}}"#
    ));
    assert_eq!(h.e.store().positions()[0].unrealized_pnl, 1.0);
    assert_eq!(h.e.store().balance().upnl, 1.23);
}
