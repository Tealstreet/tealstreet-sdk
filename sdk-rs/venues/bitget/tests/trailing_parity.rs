use crate::support::FixtureOptions as Options;
mod support;
use safe_cex_engine::model::*;
use safe_cex_engine::{Command, Event, Method, RequestId};
use serde_json::{Value, json};
use support::*;

// Documented Classic place/modify/cancel payloads:
// https://www.bitget.com/docs/catalog/classic-contract-plan/classic-contract-plan
// UTA trailingStopParams placement is an array; unfilled orders return an object:
// https://www.bitget.com/docs/catalog/trading/strategy-trading
// Added August 25, 2026: https://www.bitget.com/docs/uta/changelog/2026-08
fn opts(side: &str, price: f64) -> PlaceOrderOpts {
    serde_json::from_value(json!({"symbol":"BTCUSDT","type":"trailing_stop_market", "side":side,"amount":0.01,"price":price})).unwrap()
}
fn place(h: &mut Harness, options: Vec<PlaceOrderOpts>) {
    h.command(Command::PlaceOrders {
        command: 73,
        orders: options,
    });
}
fn body(h: &mut Harness, path: &str) -> (RequestId, Value) {
    let r = h.take(path);
    assert_eq!(r.method, Method::Post);
    (
        r.id,
        serde_json::from_str(r.body.as_deref().unwrap()).unwrap(),
    )
}
fn success(h: &mut Harness, id: RequestId, order: &str) {
    h.reply(
        id,
        200,
        &json!({"code":"00000","data":{"orderId":order}}).to_string(),
    );
}
fn uta(hedged: bool) -> Harness {
    let mut h = Harness::private_started();
    h.respond(
        "/account/settings",
        200,
        &json!({"code":"00000","data":{"holdMode":if hedged {"hedge_mode"}else{"one_way_mode"}}})
            .to_string(),
    );
    h.respond(
        "/current-position",
        200,
        r#"{"code":"00000","data":{"list":[]}}"#,
    );
    h.respond("/account/assets", 200, r#"{"code":"00000","data":{"accountEquity":"1000","effEquity":"1000","unrealisedPnl":"0"}}"#);
    h.respond(
        "/unfilled-orders",
        200,
        r#"{"code":"00000","data":{"list":[]}}"#,
    );
    while h
        .requests
        .iter()
        .any(|r| r.url.contains("/unfilled-strategy-orders"))
    {
        h.respond(
            "/unfilled-strategy-orders",
            200,
            r#"{"code":"00000","data":[]}"#,
        );
    }
    h.login();
    h.engine.store_mut().set_hedged(hedged);
    h.engine.store_mut().update_setting("BTCUSDT", |s| {
        s.leverage = 10.0;
        s.margin_type = Some(MarginType::Cross);
    });
    // Metadata is a recent live position only in other tests; flat symbols need
    // the explicit per-command settings lookup, serviced by ready().
    h
}
fn ready(h: &mut Harness) {
    while h
        .requests
        .iter()
        .any(|r| r.url.contains("/account/settings"))
    {
        let hedged = h.engine.store().hedged();
        h.respond("/account/settings",200,&json!({"code":"00000","data":{"holdMode":if hedged {"hedge_mode"}else{"one_way_mode"}}}).to_string());
    }
}
fn native_order(kind: OrderType) -> Order {
    Order {
        id: "trail-1".into(),
        parent_id: None,
        status: OrderStatus::Open,
        symbol: "BTCUSDT".into(),
        kind,
        side: OrderSide::Sell,
        price: 95.0,
        amount: 0.01,
        filled: 0.0,
        remaining: 0.01,
        reduce_only: true,
        close: None,
        client_order_id: None,
        time_in_force: None,
        last_update: 0.0,
        trigger_type: Some(StopTriggerType::LastPrice),
        take_profit: None,
        stop_loss: None,
        extra: None,
    }
}

// Defensive compatibility case: REST trailing fields adapted to the strategy topic.
// The published WS schema lists TPSL/trigger only; REST lifecycle is tested separately.
fn uta_native_trailing(h: &mut Harness, kind: &str, variance: &str) {
    h.private_message(&json!({"arg":{"instType":"UTA","topic":"strategy-order"},"data":[{
        "category":"USDT-FUTURES","type":"trailing_stop","orderId":"trail-1","symbol":"BTCUSDT",
        "qty":"0.01","side":"sell","status":"pending","reduceOnly":"yes",
        "trailingStopParams":{"activationPrice":"95","activationType":"market","trailType":kind,"trailVariance":variance,"preOrderType":"market"}
    }]}).to_string());
    assert_eq!(
        h.engine.store().order("trail-1").unwrap().kind,
        OrderType::TrailingStopLoss
    );
    assert_eq!(
        h.engine.store().order("trail-1").unwrap().trigger_type,
        Some(StopTriggerType::LastPrice)
    );
}

#[test]
fn classic_trailing_uses_documented_percentage_and_market_execution() {
    let mut h = Harness::classic();
    let mut o = opts("sell", 95.0);
    o.sl_trigger_type = Some(StopTriggerType::LastPrice);
    place(&mut h, vec![o]);
    let (id, p) = body(&mut h, "/place-plan-order");
    assert_eq!(p["planType"], "track_plan");
    assert_eq!(p["callbackRatio"], "5");
    assert_eq!(p["triggerPrice"], "95");
    assert_eq!(p["triggerType"], "fill_price");
    assert_eq!(p["size"], "0.01");
    assert_eq!(p["orderType"], "market");
    assert_eq!(p["reduceOnly"], "YES");
    assert_eq!(p["side"], "sell");
    assert!(p.get("price").is_none());
    success(&mut h, id, "new-trailing");
    assert_eq!(
        h.done.last().unwrap().1.as_ref().unwrap(),
        "[\"new-trailing\"]"
    );
}

#[test]
fn classic_hedged_trailing_protects_the_requested_position() {
    for (side, expected) in [("sell", "buy"), ("buy", "sell")] {
        let mut h = Harness::classic();
        h.engine.store_mut().set_hedged(true);
        place(&mut h, vec![opts(side, 95.0)]);
        let (_, p) = body(&mut h, "/place-plan-order");
        assert_eq!(p["side"], expected);
        assert_eq!(p["tradeSide"], "close");
        assert!(p.get("reduceOnly").is_none());
    }
}

#[test]
fn trailing_batch_preflights_all_distance_and_size_errors_before_any_write() {
    for (size, price) in [
        (0.0, 95.0),
        (0.0001, 95.0),
        (0.01, 100.0),
        (0.01, 99.99),
        (0.01, 80.0),
    ] {
        let mut h = Harness::classic();
        let mut bad = opts("sell", price);
        bad.amount = size;
        place(&mut h, vec![opts("sell", 95.0), bad]);
        assert!(h.requests.iter().all(|r| r.method == Method::Get));
        assert!(h.done.last().unwrap().1.is_err());
    }
}

#[test]
fn classic_trailing_amend_uses_native_plan_fields_without_cancellation() {
    let mut h = Harness::classic();
    h.engine
        .store_mut()
        .upsert_order(native_order(OrderType::TrailingStopLoss));
    h.command(Command::AmendOrder {
        command: 74,
        id: "trail-1".into(),
        price: Some(96.0),
        amount: Some(0.02),
    });
    let (id, p) = body(&mut h, "/modify-plan-order");
    assert_eq!(
        p,
        json!({"productType":"usdt-futures","orderId":"trail-1","newSize":"0.02","newTriggerPrice":"96","newCallbackRatio":"4"})
    );
    assert!(
        !h.requests
            .iter()
            .any(|r| r.url.contains("cancel") || r.url.contains("modify-tpsl"))
    );
    success(&mut h, id, "trail-1");
    assert!(h.done.last().unwrap().1.is_ok());
}

#[test]
fn classic_invalid_trailing_amend_keeps_live_protection() {
    let mut h = Harness::classic();
    h.engine
        .store_mut()
        .upsert_order(native_order(OrderType::TrailingStopLoss));
    h.command(Command::AmendOrder {
        command: 74,
        id: "trail-1".into(),
        price: Some(70.0),
        amount: None,
    });
    assert!(h.done.last().unwrap().1.is_err());
    assert!(h.requests.iter().all(|r| r.method == Method::Get));
    assert_eq!(
        h.engine.store().order("trail-1").unwrap().status,
        OrderStatus::Open
    );
}

#[test]
fn uta_trailing_uses_documented_array_distance_and_no_unsupported_client_id() {
    for hedged in [false, true] {
        for side in ["buy", "sell"] {
            let mut h = uta(hedged);
            let mut o = opts(side, 95.0);
            o.sl_trigger_type = Some(StopTriggerType::LastPrice);
            place(&mut h, vec![o]);
            ready(&mut h);
            let (id, p) = body(&mut h, "/place-strategy-order");
            assert_eq!(p["category"], "USDT-FUTURES");
            assert_eq!(p["type"], "trailing_stop");
            assert_eq!(p["side"], side);
            assert_eq!(p["qty"], "0.01");
            assert_eq!(p["marginMode"], "crossed");
            assert_eq!(
                p["trailingStopParams"],
                json!([{"activationPrice":"95","activationType":"market","trailType":"distance","trailVariance":"5","preOrderType":"market"}])
            );
            if hedged {
                assert_eq!(p["posSide"], if side == "sell" { "long" } else { "short" });
                assert!(p.get("reduceOnly").is_none());
            } else {
                assert_eq!(p["reduceOnly"], "yes");
                assert!(p.get("posSide").is_none());
            }
            for field in [
                "clientOid",
                "tpslMode",
                "stopLoss",
                "takeProfit",
                "size",
                "planType",
            ] {
                assert!(p.get(field).is_none(), "{field}: {p}");
            }
            success(&mut h, id, "uta-trailing");
            assert!(h.done.last().unwrap().1.is_ok());
        }
    }
}

#[test]
fn uta_trailing_cancel_uses_strategy_id_and_no_category() {
    let mut h = uta(false);
    h.engine
        .store_mut()
        .upsert_order(native_order(OrderType::TrailingStopLoss));
    h.command(Command::CancelOrders {
        command: 74,
        ids: vec!["trail-1".into()],
    });
    let (id, p) = body(&mut h, "/cancel-strategy-order");
    assert_eq!(p, json!({"orderId":"trail-1"}));
    h.reply(id, 200, r#"{"code":"00000","data":null}"#);
    assert!(h.done.last().unwrap().1.is_ok());
}

#[test]
fn uta_trailing_known_rejection_rolls_back_and_reports_the_requested_edit_failed() {
    let mut h = uta(false);
    uta_native_trailing(&mut h, "distance", "5");
    h.command(Command::AmendOrder {
        command: 74,
        id: "trail-1".into(),
        price: Some(96.0),
        amount: None,
    });
    ready(&mut h);
    let (cancel, _) = body(&mut h, "/cancel-strategy-order");
    h.reply(cancel, 200, r#"{"code":"00000","data":null}"#);
    let (replace, p) = body(&mut h, "/place-strategy-order");
    assert_eq!(p["trailingStopParams"][0]["activationPrice"], "96");
    h.reply(
        replace,
        200,
        r#"{"code":"43023","msg":"invalid qty","data":null}"#,
    );
    let (rollback, p) = body(&mut h, "/place-strategy-order");
    assert_eq!(p["trailingStopParams"][0]["activationPrice"], "95");
    success(&mut h, rollback, "restored-trailing");
    assert!(
        h.done
            .last()
            .unwrap()
            .1
            .as_ref()
            .unwrap_err()
            .contains("invalid qty")
    );
}

#[test]
fn uta_ambiguous_trailing_replace_does_not_duplicate_protection() {
    let mut h = uta(false);
    uta_native_trailing(&mut h, "distance", "5");
    h.command(Command::AmendOrder {
        command: 74,
        id: "trail-1".into(),
        price: Some(96.0),
        amount: None,
    });
    ready(&mut h);
    let (cancel, _) = body(&mut h, "/cancel-strategy-order");
    h.reply(cancel, 200, r#"{"code":"00000","data":null}"#);
    let (replace, _) = body(&mut h, "/place-strategy-order");
    h.event(Event::HttpFailed {
        request: replace,
        error: "connection reset",
    });
    assert!(!h.requests.iter().any(|r| r.method == Method::Post));
    assert!(h.done.last().unwrap().1.is_err());
}

fn inverse_eth() -> Harness {
    use safe_cex_venue_bitget::Product;
    let mut h = Harness::started(Options {
        product: Product::Inverse,
        key: Some("key".into()),
        secret: Some("secret".into()),
        passphrase: Some("pass".into()),
        ..Options::default()
    });
    let contracts = CONTRACTS
        .replace("BTCUSDT", "ETHUSD")
        .replace("BTC", "ETH")
        .replace("USDT", "USD")
        .replace("0.001", "1");
    h.respond("/contracts", 200, &contracts);
    h.respond("/tickers", 200, &TICKERS.replace("BTCUSDT", "ETHUSD"));
    for (socket, _) in h.sockets.clone() {
        h.event(Event::WsOpened { socket });
    }
    h.respond(
        "/account/settings",
        200,
        r#"{"code":"40084","msg":"Classic Account mode","data":null}"#,
    );
    h.respond(
        "/all-position",
        200,
        &POSITION
            .replace("BTCUSDT", "ETHUSD")
            .replace("\"total\":\"1\"", "\"total\":\"100\""),
    );
    h.respond("/account/accounts", 200, &ACCOUNT.replace("USDT", "ETH"));
    h.orders(&OPEN_ORDER.replace("BTCUSDT", "ETHUSD"));
    h.login();
    h
}
fn eth_opts(kind: &str, amount: f64) -> PlaceOrderOpts {
    serde_json::from_value(
        json!({"symbol":"ETHUSD","type":kind,"side":"sell","amount":amount,"price":95}),
    )
    .unwrap()
}

#[test]
fn inverse_place_keeps_native_quantity_and_uses_symbol_collateral() {
    let mut h = inverse_eth();
    place(&mut h, vec![eth_opts("limit", 70.0)]);
    let (_, p) = body(&mut h, "/batch-place-order");
    assert_eq!(p["symbol"], "ETHUSD");
    assert_eq!(p["productType"], "coin-futures");
    assert_eq!(p["marginCoin"], "ETH");
    assert_eq!(p["orderList"][0]["size"], "70");
    assert_eq!(p["orderList"][0]["price"], "95");
}

#[test]
fn inverse_sized_stop_clamps_in_native_contracts_not_display_coin() {
    let mut h = inverse_eth();
    assert_eq!(h.engine.store().positions()[0].contracts, 1.0);
    assert_eq!(h.engine.store().positions()[0].notional, 100.0);
    place(&mut h, vec![eth_opts("stop_market", 120.0)]);
    let (_, p) = body(&mut h, "/place-tpsl-order");
    assert_eq!(p["marginCoin"], "ETH");
    assert_eq!(p["size"], "100");
    assert_eq!(p["planType"], "loss_plan");
}

#[test]
fn inverse_metadata_leverage_margin_and_cancellation_use_symbol_collateral() {
    let mut h = inverse_eth();
    h.engine.store_mut().set_positions(vec![]);
    h.command(Command::SetLeverage {
        command: 80,
        symbol: "ETHUSD".into(),
        leverage: 25.0,
        side: None,
    });
    let metadata = h.take("/account/account?");
    assert!(
        metadata
            .url
            .ends_with("symbol=ETHUSD&productType=coin-futures&marginCoin=ETH")
    );
    h.reply(metadata.id,200,r#"{"code":"00000","data":{"posMode":"one_way_mode","marginMode":"crossed","crossedMarginLeverage":"20"}}"#);
    let (id, p) = body(&mut h, "/set-leverage");
    assert_eq!(p["marginCoin"], "ETH");
    success(&mut h, id, "unused");
    h.command(Command::SetMarginType {
        command: 81,
        symbol: "ETHUSD".into(),
        isolated: true,
    });
    let (id, p) = body(&mut h, "/set-margin-mode");
    assert_eq!(p["marginCoin"], "ETH");
    success(&mut h, id, "unused");
    h.command(Command::CancelOrders {
        command: 82,
        ids: vec!["1".into()],
    });
    let (_, p) = body(&mut h, "/batch-cancel-orders");
    assert_eq!(p["marginCoin"], "ETH");
    assert_eq!(p["orderIdList"], json!([{"orderId":"1"}]));
}

#[test]
fn inverse_native_limit_amend_keeps_contract_size_and_fresh_client_id() {
    let mut h = inverse_eth();
    h.command(Command::AmendOrder {
        command: 83,
        id: "1".into(),
        price: Some(96.0),
        amount: Some(20.0),
    });
    let (_, p) = body(&mut h, "/modify-order");
    assert_eq!(p["newSize"], "20");
    assert_eq!(p["newPrice"], "96");
    assert_eq!(p["productType"], "coin-futures");
    assert_ne!(p["newClientOid"], "caller");
    assert!(!p["newClientOid"].as_str().unwrap().is_empty());
}

#[test]
fn classic_trailing_cancel_uses_a_bounded_native_id_list() {
    let mut h = Harness::classic();
    h.engine
        .store_mut()
        .upsert_order(native_order(OrderType::TrailingStopLoss));
    h.command(Command::CancelOrders {
        command: 84,
        ids: vec!["trail-1".into()],
    });
    let (id, p) = body(&mut h, "/cancel-plan-order");
    assert_eq!(p["orderIdList"], json!([{"orderId":"trail-1"}]));
    assert!(p.get("orderId").is_none());
    assert_eq!(p["planType"], "track_plan");
    h.reply(
        id,
        200,
        r#"{"code":"00000","data":{"successList":[{"orderId":"trail-1"}],"failureList":[]}}"#,
    );
    assert!(h.done.last().unwrap().1.is_ok());
}

#[test]
fn classic_size_only_edit_preserves_native_callback_after_ticker_movement() {
    let mut h = Harness::classic();
    h.private_message(r#"{"arg":{"channel":"orders-algo"},"data":[{"orderId":"trail-1","symbol":"BTCUSDT","planType":"track_plan","size":"0.01","triggerPrice":"95","callbackRatio":"5","side":"sell","reduceOnly":"yes","posMode":"one_way_mode","planStatus":"not_trigger"}]}"#);
    h.engine
        .store_mut()
        .update_ticker("BTCUSDT", |t| t.last = 110.0);
    h.command(Command::AmendOrder {
        command: 90,
        id: "trail-1".into(),
        price: None,
        amount: Some(0.02),
    });
    let (id, p) = body(&mut h, "/modify-plan-order");
    assert_eq!(p["newCallbackRatio"], "5");
    assert_eq!(p["newSize"], "0.02");
    assert!(p.get("newTriggerPrice").is_none());
    success(&mut h, id, "trail-1");
    // A price edit has its own callback and becomes the source for later size edits.
    h.engine
        .store_mut()
        .update_ticker("BTCUSDT", |t| t.last = 100.0);
    h.command(Command::AmendOrder {
        command: 91,
        id: "trail-1".into(),
        price: Some(96.0),
        amount: None,
    });
    let (id, p) = body(&mut h, "/modify-plan-order");
    assert_eq!(p["newCallbackRatio"], "4");
    success(&mut h, id, "trail-1");
    h.engine
        .store_mut()
        .update_ticker("BTCUSDT", |t| t.last = 150.0);
    h.command(Command::AmendOrder {
        command: 92,
        id: "trail-1".into(),
        price: None,
        amount: Some(0.03),
    });
    let (_, p) = body(&mut h, "/modify-plan-order");
    assert_eq!(p["newCallbackRatio"], "4");
}

#[test]
fn uta_size_only_edit_and_rejection_rollback_preserve_native_distance_or_ratio() {
    for (kind, variance) in [("distance", "5"), ("ratio", "0.05")] {
        let mut h = uta(false);
        uta_native_trailing(&mut h, kind, variance);
        // Even an activation price equal to the latest ticker must not create a
        // zero callback, or block an edit whose native trailing state is known.
        h.engine
            .store_mut()
            .update_ticker("BTCUSDT", |t| t.last = 95.0);
        h.command(Command::AmendOrder {
            command: 93,
            id: "trail-1".into(),
            price: None,
            amount: Some(0.02),
        });
        ready(&mut h);
        let (cancel, _) = body(&mut h, "/cancel-strategy-order");
        h.reply(cancel, 200, r#"{"code":"00000","data":null}"#);
        let (replace, p) = body(&mut h, "/place-strategy-order");
        assert_eq!(p["qty"], "0.02");
        assert_eq!(p["trailingStopParams"][0]["trailType"], kind);
        assert_eq!(p["trailingStopParams"][0]["trailVariance"], variance);
        assert_eq!(p["trailingStopParams"][0]["activationType"], "market");
        h.reply(
            replace,
            200,
            r#"{"code":"43023","msg":"invalid qty","data":null}"#,
        );
        let (rollback, p) = body(&mut h, "/place-strategy-order");
        assert_eq!(p["qty"], "0.01");
        assert_eq!(p["trailingStopParams"][0]["trailType"], kind);
        assert_eq!(p["trailingStopParams"][0]["trailVariance"], variance);
        assert_eq!(p["trailingStopParams"][0]["activationType"], "market");
        success(&mut h, rollback, "restored-trailing");
        assert!(h.done.last().unwrap().1.is_err());
    }
}

#[test]
fn uta_price_edit_rollback_keeps_original_native_distance_when_ticker_moves() {
    let mut h = uta(false);
    uta_native_trailing(&mut h, "distance", "5");
    h.engine
        .store_mut()
        .update_ticker("BTCUSDT", |t| t.last = 110.0);
    h.command(Command::AmendOrder {
        command: 94,
        id: "trail-1".into(),
        price: Some(96.0),
        amount: None,
    });
    ready(&mut h);
    let (cancel, _) = body(&mut h, "/cancel-strategy-order");
    h.reply(cancel, 200, r#"{"code":"00000","data":null}"#);
    let (replace, p) = body(&mut h, "/place-strategy-order");
    assert_eq!(p["trailingStopParams"][0]["trailVariance"], "14");
    h.reply(
        replace,
        200,
        r#"{"code":"43023","msg":"invalid qty","data":null}"#,
    );
    let (_, p) = body(&mut h, "/place-strategy-order");
    assert_eq!(p["trailingStopParams"][0]["trailVariance"], "5");
}

#[test]
fn size_edit_without_native_trailing_parameters_fails_before_any_write() {
    for unified in [false, true] {
        let mut h = if unified {
            uta(false)
        } else {
            Harness::classic()
        };
        h.engine
            .store_mut()
            .upsert_order(native_order(OrderType::TrailingStopLoss));
        h.command(Command::AmendOrder {
            command: 95,
            id: "trail-1".into(),
            price: None,
            amount: Some(0.02),
        });
        if unified {
            ready(&mut h);
        }
        assert!(
            h.done
                .last()
                .unwrap()
                .1
                .as_ref()
                .unwrap_err()
                .contains("native trailing parameters")
        );
        assert!(h.requests.iter().all(|r| r.method == Method::Get));
    }
}

#[test]
fn inverse_amend_ack_and_resolved_fill_keep_display_remaining_and_native_metadata() {
    for resolve in [false, true] {
        let mut h = inverse_eth();
        h.command(Command::AmendOrder {
            command: 96,
            id: "1".into(),
            price: Some(96.0),
            amount: Some(20.0),
        });
        let (id, p) = body(&mut h, "/modify-order");
        if resolve {
            h.reply(id, 200, r#"{"code":"00000","data":{"orderId":"1"}}"#);
            let detail = h.take("/order/detail");
            assert!(detail.url.contains(p["newClientOid"].as_str().unwrap()));
            h.reply(detail.id,200,r#"{"code":"00000","data":{"orderId":"amended","baseVolume":"4","status":"partially_filled"}}"#);
        } else {
            success(&mut h, id, "amended");
        }
        let o = h.engine.store().order("amended").unwrap();
        let expected_native = if resolve { 16.0 } else { 20.0 };
        assert_eq!(
            o.remaining,
            (expected_native / 96.0 * 100_000_000.0_f64).round() / 100_000_000.0
        );
        assert_eq!(
            o.extra.as_ref().unwrap().remaining_native,
            Some(expected_native)
        );
    }
}

#[test]
fn stale_trailing_amend_ack_cannot_overwrite_newer_stream_callback() {
    let mut h = Harness::classic();
    h.engine
        .store_mut()
        .upsert_order(native_order(OrderType::TrailingStopLoss));
    h.command(Command::AmendOrder {
        command: 97,
        id: "trail-1".into(),
        price: Some(96.0),
        amount: None,
    });
    let (id, _) = body(&mut h, "/modify-plan-order");
    h.private_message(r#"{"arg":{"channel":"orders-algo"},"data":[{"orderId":"trail-1","symbol":"BTCUSDT","planType":"track_plan","size":"0.01","triggerPrice":"95","callbackRatio":"9","uTime":"100","side":"sell","reduceOnly":"yes","posMode":"one_way_mode","planStatus":"not_trigger"}]}"#);
    success(&mut h, id, "trail-1");
    h.command(Command::AmendOrder {
        command: 98,
        id: "trail-1".into(),
        price: None,
        amount: Some(0.03),
    });
    let (_, p) = body(&mut h, "/modify-plan-order");
    assert_eq!(p["newCallbackRatio"], "9");
}

#[test]
fn external_uta_limit_execution_and_index_activation_stay_visible_and_cancelable_but_not_editable()
{
    for (execution, activation) in [("limit", "market"), ("market", "index")] {
        let mut h = uta(false);
        h.private_message(&json!({"arg":{"instType":"UTA","topic":"strategy-order"},"data":[{
            "category":"USDT-FUTURES","type":"trailing_stop","orderId":"external-trail","symbol":"BTCUSDT",
            "qty":"0.01","side":"sell","status":"pending","reduceOnly":"yes",
            "trailingStopParams":{"activationPrice":"95","activationType":activation,"trailType":"distance",
                "trailVariance":"5","preOrderType":execution,"preOrderPrice":"94"}
        }]}).to_string());
        let order = h.engine.store().order("external-trail").unwrap();
        assert_eq!(order.kind, OrderType::TrailingStopLoss);
        assert_eq!(order.price, 95.0);
        assert_eq!(order.status, OrderStatus::Open);
        for (command, price, amount) in [(101, Some(96.0), None), (102, None, Some(0.02))] {
            h.command(Command::AmendOrder {
                command,
                id: "external-trail".into(),
                price,
                amount,
            });
            ready(&mut h);
            assert!(
                h.done
                    .last()
                    .unwrap()
                    .1
                    .as_ref()
                    .unwrap_err()
                    .contains("native trailing parameters")
            );
            assert!(h.requests.iter().all(|r| r.method == Method::Get));
            assert_eq!(
                h.engine.store().order("external-trail").unwrap().status,
                OrderStatus::Open
            );
        }
        h.command(Command::CancelOrders {
            command: 103,
            ids: vec!["external-trail".into()],
        });
        let (id, p) = body(&mut h, "/cancel-strategy-order");
        assert_eq!(p, json!({"orderId":"external-trail"}));
        h.reply(id, 200, r#"{"code":"00000","data":null}"#);
        assert!(h.done.last().unwrap().1.is_ok());
    }
}

#[test]
fn one_uta_write_starts_one_complete_refresh_without_a_duplicate_dirty_followup() {
    let mut h = uta(false);
    // Opening the authenticated socket requests another complete snapshot.
    while h
        .requests
        .iter()
        .any(|r| r.url.contains("/unfilled-orders"))
    {
        h.respond(
            "/unfilled-orders",
            200,
            r#"{"code":"00000","data":{"list":[]}}"#,
        );
        h.respond("type=tpsl", 200, r#"{"code":"00000","data":[]}"#);
        h.respond("type=trailing_stop", 200, r#"{"code":"00000","data":[]}"#);
    }
    place(&mut h, vec![opts("sell", 95.0)]);
    ready(&mut h);
    let (id, _) = body(&mut h, "/place-strategy-order");
    success(&mut h, id, "one-write-trailing");
    assert_eq!(
        h.requests
            .iter()
            .filter(|r| r.url.contains("/unfilled-orders"))
            .count(),
        1
    );
    h.respond(
        "/unfilled-orders",
        200,
        r#"{"code":"00000","data":{"list":[]}}"#,
    );
    h.respond("type=tpsl", 200, r#"{"code":"00000","data":[]}"#);
    h.respond("type=trailing_stop", 200, r#"{"code":"00000","data":[]}"#);
    assert!(
        !h.requests
            .iter()
            .any(|r| r.url.contains("/unfilled-orders")
                || r.url.contains("/unfilled-strategy-orders"))
    );
    assert_eq!(
        h.done.last().unwrap().1.as_ref().unwrap(),
        "[\"one-write-trailing\"]"
    );
}
