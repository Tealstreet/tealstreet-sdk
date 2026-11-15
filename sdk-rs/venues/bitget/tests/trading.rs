mod support;
use safe_cex_engine::model::*;
use safe_cex_engine::{Command, Event, Method};
use serde_json::{Value, json};
use support::*;

fn opts(value: Value) -> PlaceOrderOpts {
    serde_json::from_value(value).unwrap()
}
fn place(h: &mut Harness, values: Vec<Value>) {
    h.command(Command::PlaceOrders {
        command: 1,
        orders: values.into_iter().map(opts).collect(),
    });
}
fn limit() -> Value {
    json!({"symbol":"BTCUSDT","type":"limit","side":"buy","amount":0.0021,"price":90.04})
}

fn classic_with_market_cap(cap: Option<&str>) -> Harness {
    let mut h = Harness::started(FixtureOptions {
        key: Some("key".into()),
        secret: Some("secret".into()),
        passphrase: Some("pass".into()),
        ..FixtureOptions::default()
    });
    let contracts = cap.map_or_else(
        || CONTRACTS.to_owned(),
        |cap| {
            CONTRACTS.replace(
                "\"maxOrderQty\":\"120\"",
                &format!("\"maxOrderQty\":\"120\",\"maxMarketOrderQty\":\"{cap}\""),
            )
        },
    );
    h.respond("/contracts", 200, &contracts);
    h.respond("/tickers", 200, TICKERS);
    h.respond(
        "/account/settings",
        200,
        r#"{"code":"40084","msg":"Classic Account mode","data":null}"#,
    );
    h.respond("/all-position", 200, POSITION);
    h.respond("/account/accounts", 200, ACCOUNT);
    h.orders(OPEN_ORDER);
    h.login();
    h
}

#[test]
fn market_quantity_cap_splits_reduce_only_children_but_keeps_the_limit_cap() {
    for (kind, reduce_only, expected) in [
        ("market", false, vec![26.667, 26.667, 26.667, 20.0]),
        ("market", true, vec![26.667, 26.667, 26.667, 20.0]),
        ("limit", false, vec![100.0]),
    ] {
        let mut h = classic_with_market_cap(Some("40"));
        place(
            &mut h,
            vec![
                json!({"symbol":"BTCUSDT", "type":kind, "side":"sell", "amount":100.0, "price":100.0, "reduceOnly":reduce_only}),
            ],
        );
        let (_, payload) = body(&mut h, "/batch-place-order");
        let children = payload["orderList"].as_array().unwrap();
        let quantities: Vec<f64> = children
            .iter()
            .map(|child| child["size"].as_str().unwrap().parse().unwrap())
            .collect();
        assert_eq!(quantities, expected, "{kind}, reduceOnly={reduce_only}");
        assert!(
            children
                .iter()
                .all(|child| child["reduceOnly"] == if reduce_only { "YES" } else { "NO" })
        );
        assert_eq!(
            h.engine
                .store()
                .market("BTCUSDT")
                .unwrap()
                .limits
                .amount
                .max,
            40.0
        );
    }
}

#[test]
fn market_quantity_cap_never_exceeds_limit_and_missing_field_keeps_previous_cap() {
    for cap in [None, Some("200"), Some("0"), Some("-1")] {
        let mut h = classic_with_market_cap(cap);
        place(
            &mut h,
            vec![
                json!({"symbol":"BTCUSDT", "type":"market", "side":"sell", "amount":150.0, "reduceOnly":true}),
            ],
        );
        let (_, payload) = body(&mut h, "/batch-place-order");
        let quantities: Vec<f64> = payload["orderList"]
            .as_array()
            .unwrap()
            .iter()
            .map(|child| child["size"].as_str().unwrap().parse().unwrap())
            .collect();
        assert_eq!(quantities, vec![60.0, 60.0, 30.0]);
    }
}

#[test]
fn market_quantity_cap_refuses_a_caller_identity_spanning_children() {
    let mut h = classic_with_market_cap(Some("40"));
    place(
        &mut h,
        vec![
            json!({"symbol":"BTCUSDT", "type":"market", "side":"sell", "amount":100.0, "reduceOnly":true, "clientOrderId":"cap-control"}),
        ],
    );
    assert!(
        !h.requests
            .iter()
            .any(|request| request.url.contains("/batch-place-order"))
    );
    assert!(
        h.done
            .iter()
            .any(|(id, result)| *id == 1 && result.is_err())
    );
}

fn body(h: &mut Harness, path: &str) -> (safe_cex_engine::RequestId, Value) {
    let request = h.take(path);
    (
        request.id,
        serde_json::from_str(request.body.as_deref().unwrap()).unwrap(),
    )
}
fn batch_success(h: &mut Harness, request: safe_cex_engine::RequestId, ids: &[&str]) {
    h.reply(request,200,&json!({"code":"00000","data":{"successList":ids.iter().map(|id|json!({"orderId":id})).collect::<Vec<_>>(),"failureList":[]}}).to_string());
}
fn order_event(h: &mut Harness, value: Value) {
    h.private_message(
        &json!({"arg":{"channel":"orders"},"action":"snapshot","data":[value]}).to_string(),
    );
}
fn uta() -> Harness {
    let mut h = Harness::private_started();
    h.respond(
        "/account/settings",
        200,
        r#"{"code":"00000","data":{"holdMode":"hedge_mode"}}"#,
    );
    h.respond(
        "/current-position",
        200,
        &POSITION
            .replace("\"data\":[", "\"data\":{\"list\":[")
            .replace("]}", "]}}"),
    );
    h.respond(
        "/account/assets",
        200,
        r#"{"code":"00000","data":{"accountEquity":"1000","effEquity":"900","unrealisedPnl":"0"}}"#,
    );
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
    h.engine.store_mut().set_hedged(true);
    h
}

#[test]
fn place_honors_caller_id_rounds_and_uses_broker_header() {
    let mut h = Harness::classic();
    let mut o = limit();
    o["clientOrderId"] = json!("caller.id:/_-1");
    place(&mut h, vec![o]);
    let request = h.take("/batch-place-order");
    assert_eq!(request.method, Method::Post);
    assert!(
        request
            .headers
            .contains(&("X-CHANNEL-API-CODE", "Tealstreet".into()))
    );
    let p: Value = serde_json::from_str(request.body.as_deref().unwrap()).unwrap();
    assert_eq!(p["symbol"], "BTCUSDT");
    assert_eq!(p["marginCoin"], "USDT");
    assert_eq!(p["marginMode"], "crossed");
    assert_eq!(p["orderList"][0]["size"], "0.002");
    assert_eq!(p["orderList"][0]["price"], "90");
    assert_eq!(p["orderList"][0]["clientOid"], "caller.id:/_-1");
    assert_eq!(p["orderList"][0]["reduceOnly"], "NO");
    batch_success(&mut h, request.id, &["42"]);
    assert_eq!(h.done.last().unwrap().1.as_ref().unwrap(), "[\"42\"]");
}

#[test]
fn all_caller_ids_are_checked_before_any_order_write() {
    let mut h = Harness::classic();
    let good = limit();
    let mut bad = limit();
    bad["clientOrderId"] = json!("bad id");
    place(&mut h, vec![good, bad]);
    assert!(h.requests.iter().all(|r| r.method == Method::Get));
    assert!(h.done[0].1.is_err());
    let mut one = limit();
    one["clientOrderId"] = json!("duplicate");
    place(&mut h, vec![one.clone(), one]);
    assert!(h.requests.iter().all(|r| r.method == Method::Get));
}

#[test]
fn bybit_controls_reject_the_whole_batch_before_any_order_write() {
    for uta_mode in [false, true] {
        for (field, value) in [
            ("bybitBbo", json!({"sideType":"Queue","level":1})),
            ("bybitSlippageToleranceTicks", json!(2)),
        ] {
            let mut h = if uta_mode { uta() } else { Harness::classic() };
            let mut unsupported = limit();
            unsupported[field] = value;
            place(&mut h, vec![limit(), unsupported]);
            assert!(
                h.requests.iter().all(|r| r.method == Method::Get),
                "{field} emitted an order write (UTA: {uta_mode})"
            );
            assert!(
                h.done
                    .last()
                    .unwrap()
                    .1
                    .as_ref()
                    .unwrap_err()
                    .contains("Bybit"),
                "{field} was not rejected as unsupported (UTA: {uta_mode})"
            );
        }
    }
}

#[test]
fn caller_id_rejects_brackets_stops_and_precision_induced_split() {
    for shape in [
        json!({"stopLoss":80}),
        json!({"type":"stop_market","price":80}),
    ] {
        let mut h = Harness::classic();
        let mut o = limit();
        o["clientOrderId"] = json!("caller");
        for (key, value) in shape.as_object().unwrap() {
            o[key] = value.clone();
        }
        place(&mut h, vec![o]);
        assert!(h.done[0].1.is_err());
        assert!(h.requests.iter().all(|r| r.method == Method::Get));
    }
    let mut h = Harness::classic();
    h.engine.store_mut().update_markets(|m| {
        m.precision.amount = 1.0;
        m.limits.amount.max = 10.6;
    });
    let mut o = limit();
    o["amount"] = json!(10.6);
    o["clientOrderId"] = json!("caller");
    place(&mut h, vec![o]);
    assert!(h.done[0].1.is_err());
    assert!(h.requests.iter().all(|r| r.method == Method::Get));
}

#[test]
fn classic_lot_allocation_matches_the_six_ts_audit_sequences() {
    assert_lot_allocation(false);
}

#[test]
fn uta_lot_allocation_matches_the_six_ts_audit_sequences() {
    assert_lot_allocation(true);
}

fn assert_lot_allocation(unified: bool) {
    let mut mismatches = vec![];
    for (amount, max, step, expected, expected_sum) in [
        // KNOWN-ISSUE: legacy TS under-allocates (sum 25 of requested 26); kept for exact TS parity per Sam's decision
        (
            26.0,
            5.0,
            1.0,
            vec!["4", "4", "4", "4", "4", "4", "1"],
            25.0,
        ),
        // KNOWN-ISSUE: legacy TS under-allocates (sum 10 of requested 11); kept for exact TS parity per Sam's decision
        (11.0, 5.0, 1.0, vec!["3", "3", "3", "1"], 10.0),
        // KNOWN-ISSUE: legacy TS under-allocates (sum 11 of requested 12); kept for exact TS parity per Sam's decision
        (12.0, 5.0, 1.0, vec!["3", "3", "3", "2"], 11.0),
        (10.0, 5.0, 1.0, vec!["5", "5"], 10.0),
        (7.0, 3.0, 0.5, vec!["2", "2", "2", "1"], 7.0),
        (
            1.0005,
            0.3,
            0.001,
            vec!["0.225", "0.225", "0.225", "0.225", "0.101"],
            1.001,
        ),
    ] {
        let mut h = if unified { uta() } else { Harness::classic() };
        h.engine.store_mut().update_markets(|market| {
            market.precision.amount = step;
            market.limits.amount.min = step;
            market.limits.amount.max = max;
        });
        let mut order = limit();
        order["amount"] = json!(amount);
        place(&mut h, vec![order]);
        let mut actual = vec![];
        while h.done.is_empty() {
            if unified {
                let (request, payload) = body(&mut h, "/place-order");
                actual.push(payload["qty"].as_str().unwrap().to_owned());
                h.reply(
                    request,
                    200,
                    &json!({"code":"00000","data":{"orderId":format!("native-{}", actual.len())}})
                        .to_string(),
                );
            } else {
                let (request, payload) = body(&mut h, "/batch-place-order");
                actual.extend(
                    payload["orderList"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|row| row["size"].as_str().unwrap().to_owned()),
                );
                let ids: Vec<_> = (0..actual.len())
                    .map(|index| format!("native-{index}"))
                    .collect();
                batch_success(
                    &mut h,
                    request,
                    &ids.iter().map(String::as_str).collect::<Vec<_>>(),
                );
            }
        }
        if actual != expected {
            mismatches.push(format!("{amount}/{max}/{step}: {actual:?} != {expected:?}"));
        }
        let sum: f64 = actual
            .iter()
            .map(|quantity| quantity.parse::<f64>().unwrap())
            .sum();
        assert!(
            (sum - expected_sum).abs() < 1e-12,
            "{amount}/{max}/{step}: sum {sum} != {expected_sum}"
        );
        assert!(h.done[0].1.is_ok());
        assert!(
            h.requests
                .iter()
                .all(|request| request.method == Method::Get)
        );
    }
    assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
}

#[test]
fn split_uses_ts_equal_lots_and_batches_at_fifty() {
    let mut h = Harness::classic();
    h.engine.store_mut().update_markets(|m| {
        m.precision.amount = 1.0;
        m.limits.amount.max = 3.0;
    });
    let mut o = limit();
    o["amount"] = json!(10);
    place(&mut h, vec![o]);
    let (request, p) = body(&mut h, "/batch-place-order");
    assert_eq!(
        p["orderList"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p["size"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["2", "2", "2", "2", "1"]
    );
    batch_success(&mut h, request, &["a", "b", "c", "d", "e"]);
    let mut h = Harness::classic();
    place(&mut h, (0..51).map(|_| limit()).collect());
    let (request, p) = body(&mut h, "/batch-place-order");
    assert_eq!(p["orderList"].as_array().unwrap().len(), 50);
    let ids: Vec<_> = (0..50).map(|i| i.to_string()).collect();
    let refs: Vec<_> = ids.iter().map(String::as_str).collect();
    batch_success(&mut h, request, &refs);
    let (_, p) = body(&mut h, "/batch-place-order");
    assert_eq!(p["orderList"].as_array().unwrap().len(), 1);
}

#[test]
fn sized_stop_clamps_and_floors_while_explicit_zero_stays_full_position() {
    let mut h = Harness::classic();
    h.engine
        .store_mut()
        .update_markets(|m| m.precision.amount = 1.0);
    h.engine
        .store_mut()
        .update_positions(|p| p.contracts = 30.6);
    place(
        &mut h,
        vec![json!({"symbol":"BTCUSDT","type":"stop_market","side":"sell","amount":31,"price":80})],
    );
    let (request, p) = body(&mut h, "/place-tpsl-order");
    assert_eq!(p["planType"], "loss_plan");
    assert_eq!(p["size"], "30");
    assert_eq!(p["holdSide"], "buy");
    h.reply(request, 200, r#"{"code":"00000","data":{"orderId":"3"}}"#);
    place(
        &mut h,
        vec![
            json!({"symbol":"BTCUSDT","type":"take_profit_market","side":"sell","amount":0,"price":120}),
        ],
    );
    let (_, p) = body(&mut h, "/place-tpsl-order");
    assert_eq!(p["planType"], "pos_profit");
    assert!(p.get("size").is_none());
}

#[test]
fn plan_size_uses_correct_hedge_side_and_dust_downgrades() {
    let mut h = Harness::classic();
    h.engine.store_mut().set_hedged(true);
    h.engine
        .store_mut()
        .update_markets(|m| m.precision.amount = 1.0);
    h.engine.store_mut().update_positions(|p| p.contracts = 0.4);
    place(
        &mut h,
        vec![json!({"symbol":"BTCUSDT","type":"stop_market","side":"sell","amount":1,"price":80})],
    );
    let (_, p) = body(&mut h, "/place-tpsl-order");
    assert_eq!(p["holdSide"], "long");
    assert_eq!(p["planType"], "pos_loss");
    assert!(p.get("size").is_none());
}

#[test]
fn hedge_reduce_entry_inverts_classic_wire_side_only() {
    let mut h = Harness::classic();
    h.engine.store_mut().set_hedged(true);
    let mut o = limit();
    o["side"] = json!("sell");
    o["reduceOnly"] = json!(true);
    place(&mut h, vec![o]);
    let (_, p) = body(&mut h, "/batch-place-order");
    let o = &p["orderList"][0];
    assert_eq!(o["side"], "buy");
    assert_eq!(o["tradeSide"], "close");
    assert!(o.get("reduceOnly").is_none());
}

#[test]
fn cancel_plan_retains_immutable_sized_type_after_position_is_flat() {
    let mut h = Harness::classic();
    h.private_message(r#"{"arg":{"channel":"orders-algo"},"data":[{"orderId":"3","symbol":"BTCUSDT","size":"1","triggerPrice":"80","holdSide":"long","planType":"sl","status":"live"}]}"#);
    h.engine.store_mut().set_positions(vec![]);
    h.command(Command::CancelOrders {
        command: 1,
        ids: vec!["3".into()],
    });
    let (_, p) = body(&mut h, "/cancel-plan-order");
    assert_eq!(p["planType"], "loss_plan");
}

#[test]
fn native_amend_sends_both_fields_new_client_id_and_returns_new_native_id() {
    let mut h = Harness::classic();
    h.command(Command::AmendOrder {
        command: 1,
        id: "1".into(),
        price: Some(95.04),
        amount: None,
    });
    let (request, p) = body(&mut h, "/modify-order");
    assert_eq!(p["newPrice"], "95");
    assert_eq!(p["newSize"], "2");
    assert_ne!(p["newClientOid"], "caller");
    h.reply(request, 200, r#"{"code":"00000","data":{"orderId":"2"}}"#);
    assert_eq!(h.done[0].1.as_ref().unwrap(), "[\"2\"]");
    assert_eq!(
        h.engine.store().order("1").unwrap().status,
        OrderStatus::Canceled
    );
    assert_eq!(
        h.engine
            .store()
            .order("2")
            .unwrap()
            .client_order_id
            .as_deref(),
        p["newClientOid"].as_str()
    );
}

#[test]
fn async_amend_resolves_only_by_new_client_id_without_repeating_write() {
    let mut h = Harness::classic();
    h.command(Command::AmendOrder {
        command: 1,
        id: "1".into(),
        price: Some(95.0),
        amount: None,
    });
    let (request, p) = body(&mut h, "/modify-order");
    h.reply(request, 200, r#"{"code":"00000","data":{}}"#);
    let read = h.take("/order/detail");
    assert!(read.url.contains(p["newClientOid"].as_str().unwrap()));
    assert_eq!(read.method, Method::Get);
    h.reply(
        read.id,
        200,
        r#"{"code":"43025","msg":"order not yet available","data":null}"#,
    );
    h.timer_after(250.0);
    let read = h.take("/order/detail");
    h.reply(read.id, 200, r#"{"code":"00000","data":{"orderId":"2"}}"#);
    assert_eq!(h.done[0].1.as_ref().unwrap(), "[\"2\"]");
    assert!(h.requests.iter().all(|r| r.method == Method::Get));
}

#[test]
fn unknown_amend_response_never_cancels_or_replaces() {
    let mut h = Harness::classic();
    h.command(Command::AmendOrder {
        command: 1,
        id: "1".into(),
        price: Some(95.0),
        amount: None,
    });
    let request = h.take("/modify-order");
    h.event(Event::HttpFailed {
        request: request.id,
        error: "lost response",
    });
    assert!(h.done[0].1.is_err());
    assert!(h.requests.iter().all(|r| r.method == Method::Get));
}

#[test]
fn rejected_amend_falls_back_but_cancel_failure_prevents_replacement() {
    let mut h = Harness::classic();
    h.command(Command::AmendOrder {
        command: 1,
        id: "1".into(),
        price: Some(95.0),
        amount: None,
    });
    h.respond(
        "/modify-order",
        200,
        r#"{"code":"40017","msg":"cannot modify","data":null}"#,
    );
    let (request, _) = body(&mut h, "/batch-cancel-orders");
    h.reply(request,200,r#"{"code":"00000","data":{"successList":[],"failureList":[{"orderId":"1","errorMsg":"cancel failed"}]}}"#);
    assert!(h.done[0].1.is_err());
    assert!(
        !h.requests
            .iter()
            .any(|r| r.url.contains("/batch-place-order"))
    );
    assert_eq!(
        h.engine.store().order("1").unwrap().status,
        OrderStatus::Open
    );
}

#[test]
fn canceling_virtual_child_cancels_parent_and_tombstones_both_siblings() {
    let mut h = Harness::classic();
    order_event(
        &mut h,
        json!({"orderId":"1","symbol":"BTCUSDT","size":"2","baseVolume":"0","price":"90","side":"buy","orderType":"limit","status":"live","presetStopLossPrice":"80","presetStopSurplusPrice":"120"}),
    );
    h.command(Command::CancelOrders {
        command: 1,
        ids: vec!["1__stop_loss".into()],
    });
    let (request, p) = body(&mut h, "/batch-cancel-orders");
    assert_eq!(p["orderIdList"], json!([{"orderId":"1"}]));
    batch_success(&mut h, request, &["1"]);
    for id in ["1", "1__stop_loss", "1__take_profit"] {
        assert_eq!(
            h.engine.store().order(id).unwrap().status,
            OrderStatus::Canceled
        );
    }
}

#[test]
fn classic_sized_stop_amend_zero_preserves_size() {
    let mut h = Harness::classic();
    h.private_message(r#"{"arg":{"channel":"orders-algo"},"data":[{"orderId":"3","symbol":"BTCUSDT","size":"0.5","triggerPrice":"80","executePrice":"0","holdSide":"long","planType":"loss_plan","status":"live"}]}"#);
    h.command(Command::AmendOrder {
        command: 1,
        id: "3".into(),
        price: Some(85.0),
        amount: Some(0.0),
    });
    let (_, p) = body(&mut h, "/modify-tpsl-order");
    assert_eq!(p["size"], "0.5");
    assert_eq!(p["triggerPrice"], "85");
}

#[test]
fn uta_symbol_cancel_hits_the_venue_with_no_order_in_the_store() {
    let mut h = uta();
    h.command(Command::CancelSymbolOrders {
        command: 9,
        symbol: "BTCUSDT".into(),
    });
    let (request, sent) = body(&mut h, "cancel-symbol-order");
    assert_eq!(sent["symbol"], "BTCUSDT");
    assert_eq!(sent["category"], "USDT-FUTURES");
    h.reply(
        request,
        200,
        r#"{"code":"00000","msg":"success","data":{"list":[]}}"#,
    );
    assert!(h.done.iter().any(|(id, result)| *id == 9 && result.is_ok()));
}

#[test]
fn uta_entry_and_strategy_use_v3_shapes_and_actual_execution_side() {
    let mut h = uta();
    let mut o = limit();
    o["side"] = json!("sell");
    o["reduceOnly"] = json!(true);
    o["takeProfit"] = json!(120);
    place(&mut h, vec![o]);
    let (request, p) = body(&mut h, "/api/v3/trade/place-order");
    assert_eq!(p["category"], "USDT-FUTURES");
    assert_eq!(p["side"], "sell");
    assert_eq!(p["posSide"], "long");
    assert_eq!(p["qty"], "0.002");
    assert!(p.get("size").is_none());
    assert_eq!(p["takeProfit"], "120");
    h.reply(request, 200, r#"{"code":"00000","data":{"orderId":"4"}}"#);
    place(
        &mut h,
        vec![json!({"symbol":"BTCUSDT","type":"stop_market","side":"sell","amount":0,"price":80})],
    );
    let (_, p) = body(&mut h, "/place-strategy-order");
    assert_eq!(p["tpslMode"], "partial");
    assert_eq!(p["qty"], "1");
    assert_eq!(p["stopLoss"], "80");
}

#[test]
fn uta_client_only_cancel_omits_native_id_and_strategy_absence_is_idempotent() {
    let mut h = uta();
    h.private_message(r#"{"arg":{"topic":"order"},"data":[{"category":"usdt-futures","clientOid":"strategy__close","symbol":"BTCUSDT","qty":"1","price":"90","side":"buy","orderType":"limit","orderStatus":"new"}]}"#);
    h.command(Command::CancelOrders {
        command: 1,
        ids: vec!["strategy__close".into()],
    });
    let (request, p) = body(&mut h, "/api/v3/trade/cancel-order");
    assert!(p.get("orderId").is_none());
    assert_eq!(p["clientOid"], "strategy__close");
    h.reply(request, 200, r#"{"code":"00000","data":{}}"#);
    h.private_message(r#"{"arg":{"topic":"strategy-order"},"data":[{"category":"usdt-futures","orderId":"stop","symbol":"BTCUSDT","qty":"1","stopLoss":"80","side":"sell","status":"pending"}]}"#);
    h.command(Command::CancelOrders {
        command: 2,
        ids: vec!["stop".into()],
    });
    h.respond("/cancel-strategy-order",400,r#"{"code":"45001","msg":"Failed to stop the strategy because the corresponding strategy ID could not be found","data":null}"#);
    assert!(h.done.last().unwrap().1.is_ok());
}

#[test]
fn uta_zero_amend_resolves_full_position_and_failed_replace_rolls_back_once() {
    let mut h = uta();
    h.private_message(r#"{"arg":{"topic":"strategy-order"},"data":[{"category":"usdt-futures","orderId":"stop","symbol":"BTCUSDT","qty":"0.5","stopLoss":"80","slTriggerBy":"market","slOrderType":"market","side":"sell","posSide":"long","status":"pending"}]}"#);
    h.command(Command::AmendOrder {
        command: 1,
        id: "stop".into(),
        price: Some(85.0),
        amount: Some(0.0),
    });
    h.respond(
        "/cancel-strategy-order",
        200,
        r#"{"code":"00000","data":{}}"#,
    );
    assert_eq!(h.progress.len(), 1);
    let (request, p) = body(&mut h, "/place-strategy-order");
    assert_eq!(p["qty"], "1");
    assert_eq!(h.progress[0].1[0].identity, p["clientOid"]);
    assert_eq!(p["stopLoss"], "85");
    h.reply(
        request,
        200,
        r#"{"code":"40017","msg":"replace rejected","data":null}"#,
    );
    let (rollback, p) = body(&mut h, "/place-strategy-order");
    assert_eq!(p["qty"], "0.5");
    assert_eq!(h.progress.len(), 2);
    assert_eq!(h.progress[1].1[0].identity, p["clientOid"]);
    assert_ne!(h.progress[0].1[0].identity, h.progress[1].1[0].identity);
    assert_eq!(p["stopLoss"], "80");
    h.reply(
        rollback,
        200,
        r#"{"code":"00000","data":{"orderId":"restored"}}"#,
    );
    assert!(
        h.done[0]
            .1
            .as_ref()
            .unwrap_err()
            .contains("replace rejected")
    );
}

#[test]
fn setting_commands_update_store_only_after_ack_and_failures_preserve_settings() {
    let mut h = Harness::classic();
    h.command(Command::SetLeverage {
        command: 1,
        symbol: "BTCUSDT".into(),
        leverage: 500.0,
        side: None,
    });
    let (request, p) = body(&mut h, "/set-leverage");
    assert_eq!(p["leverage"], "125");
    assert_eq!(h.engine.store().setting("BTCUSDT").unwrap().leverage, 20.0);
    h.reply(request, 200, r#"{"code":"00000","data":{}}"#);
    assert_eq!(h.engine.store().setting("BTCUSDT").unwrap().leverage, 125.0);
    h.command(Command::SetMarginType {
        command: 2,
        symbol: "BTCUSDT".into(),
        isolated: true,
    });
    h.respond(
        "/set-margin-mode",
        200,
        r#"{"code":"40017","msg":"failed","data":null}"#,
    );
    assert_eq!(
        h.engine.store().setting("BTCUSDT").unwrap().margin_type,
        Some(MarginType::Cross)
    );
    h.command(Command::SetPositionMode {
        command: 3,
        hedged: true,
        symbol: None,
    });
    h.respond("/set-position-mode", 200, r#"{"code":"00000","data":{}}"#);
    assert!(h.engine.store().hedged());
}

#[test]
fn metadata_for_flat_market_resolves_mode_and_reads_retry_without_writes() {
    let mut h = Harness::classic();
    h.engine.store_mut().set_positions(vec![]);
    place(&mut h, vec![limit()]);
    let request = h.take("/account/account?");
    h.event(Event::HttpFailed {
        request: request.id,
        error: "network reset",
    });
    h.timer_after(250.0);
    let request = h.take("/account/account?");
    h.reply(request.id,200,r#"{"code":"00000","data":{"posMode":"hedge_mode","marginMode":"isolated","isolatedLongLever":"10","isolatedShortLever":"10"}}"#);
    let (_, p) = body(&mut h, "/batch-place-order");
    assert_eq!(p["marginMode"], "isolated");
    assert_eq!(p["orderList"][0]["tradeSide"], "open");
}

#[test]
fn post_write_private_watchdog_ignores_pongs_and_promotes_classic_fallback() {
    let mut h = Harness::classic();
    place(&mut h, vec![limit()]);
    let (request, _) = body(&mut h, "/batch-place-order");
    batch_success(&mut h, request, &["4"]);
    h.now += 4_000.0;
    h.private_message("pong");
    h.now += 1_000.0;
    for (timer, at) in h.timers.clone() {
        if at <= h.now {
            h.event(Event::TimerFired { timer });
        }
    }
    assert!(h.socket_contexts.iter().any(|(_, url, context)| {
        url == "wss://ws.bitget.com/v2/ws/private"
            && context
                .as_ref()
                .is_some_and(|context| context.feed == "private" && context.recover)
    }));
    assert!(h.sockets.iter().all(|(_, url)| !url.contains("tealstreet")));
}

#[test]
fn amend_old_ack_is_resolved_and_terminal_detail_is_preserved() {
    let mut h = Harness::classic();
    h.command(Command::AmendOrder {
        command: 1,
        id: "1".into(),
        price: Some(95.0),
        amount: None,
    });
    let request = h.take("/modify-order");
    h.reply(
        request.id,
        200,
        r#"{"code":"00000","data":{"orderId":"1"}}"#,
    );
    let read = h.take("/order/detail");
    h.reply(
        read.id,
        200,
        r#"{"code":"00000","data":{"orderId":"2","status":"filled","baseVolume":"2"}}"#,
    );
    assert_eq!(
        h.engine.store().order("2").unwrap().status,
        OrderStatus::Closed
    );
    assert_eq!(h.engine.store().order("2").unwrap().filled, 2.0);
}

#[test]
fn amend_old_cancellation_and_old_rest_cannot_hide_new_identity() {
    let mut h = Harness::classic();
    let old = h.take("/orders-pending");
    h.command(Command::AmendOrder {
        command: 1,
        id: "1".into(),
        price: Some(95.0),
        amount: None,
    });
    let request = h.take("/modify-order");
    order_event(
        &mut h,
        json!({"orderId":"1","symbol":"BTCUSDT","size":"2","baseVolume":"0","price":"90","side":"buy","orderType":"limit","status":"canceled"}),
    );
    h.reply(
        request.id,
        200,
        r#"{"code":"00000","data":{"orderId":"2"}}"#,
    );
    h.reply(
        old.id,
        200,
        r#"{"code":"00000","data":{"entrustedList":[]}}"#,
    );
    h.respond(
        "planType=profit_loss",
        200,
        r#"{"code":"00000","data":{"entrustedList":[]}}"#,
    );
    h.respond(
        "planType=track_plan",
        200,
        r#"{"code":"00000","data":{"entrustedList":[]}}"#,
    );
    assert_eq!(
        h.engine.store().order("2").unwrap().status,
        OrderStatus::Open
    );
    h.command(Command::CancelOrders {
        command: 2,
        ids: vec!["2".into()],
    });
    assert!(h.take("/batch-cancel-orders").body.unwrap().contains("2"));
}

#[test]
fn unknown_server_amend_failure_never_replaces() {
    for (status, code) in [
        (200, 40010),
        (200, 40725),
        (200, 45001),
        (400, 40010),
        (400, 40725),
        (400, 45001),
    ] {
        let mut h = Harness::classic();
        h.command(Command::AmendOrder {
            command: 1,
            id: "1".into(),
            price: Some(95.0),
            amount: None,
        });
        let r = h.take("/modify-order");
        h.reply(
            r.id,
            status,
            &json!({"code":code.to_string(),"msg":"unknown result","data":null}).to_string(),
        );
        assert!(h.done[0].1.is_err());
        assert!(h.requests.iter().all(|r| r.method == Method::Get));
    }
}

#[test]
fn late_uta_write_wait_has_a_deadline_after_probe_failure() {
    let mut h = Harness::classic();
    place(&mut h, vec![limit()]);
    h.respond(
        "/batch-place-order",
        200,
        r#"{"code":"40085","msg":"Classic API unsupported","data":null}"#,
    );
    h.respond(
        "/account/settings",
        200,
        r#"{"code":"40014","msg":"bad passphrase","data":null}"#,
    );
    h.timer_after(10_000.0);
    // TS-visible since slice 3: the job still ends at the detection bound, but
    // the sent and definitively refused 40085 call is the projected terminal
    // (Rejected, its native text), not a NotSent detection failure.
    let [error] = std::mem::take(&mut h.failures).try_into().unwrap();
    assert_eq!(error.outcome, Some(safe_cex_engine::Outcome::Rejected));
    assert!(error.message.contains("40085"), "{}", error.message);
}

#[test]
fn isolated_hedge_leverage_ack_only_updates_the_requested_long_side() {
    for uta_account in [false, true] {
        let mut h = if uta_account {
            uta()
        } else {
            Harness::classic()
        };
        h.engine.store_mut().set_hedged(true);
        h.engine
            .store_mut()
            .update_setting("BTCUSDT", |s| s.margin_type = Some(MarginType::Isolated));
        let mut long = h.engine.store().positions()[0].clone();
        long.id = "long".into();
        long.side = PositionSide::Long;
        long.leverage = 10.0;
        long.margin_type = Some(MarginType::Isolated);
        let mut short = long.clone();
        short.id = "short".into();
        short.side = PositionSide::Short;
        h.engine.store_mut().set_positions(vec![long, short]);
        h.command(Command::SetLeverage {
            command: 1,
            symbol: "BTCUSDT".into(),
            leverage: 20.0,
            side: None,
        });
        let (request, p) = body(&mut h, "/set-leverage");
        assert_eq!(p[if uta_account { "posSide" } else { "holdSide" }], "long");
        h.reply(request, 200, r#"{"code":"00000","data":{}}"#);
        assert_eq!(h.engine.store().position("long").unwrap().leverage, 20.0);
        assert_eq!(h.engine.store().position("short").unwrap().leverage, 10.0);
    }
}

#[test]
fn symbol_cancel_covers_acknowledged_placement_before_private_echo() {
    let mut h = Harness::classic();
    place(&mut h, vec![limit()]);
    let placed = h.take("/batch-place-order");
    batch_success(&mut h, placed.id, &["ack-only"]);
    assert!(h.engine.store().order("ack-only").is_none());
    h.engine.store_mut().set_orders(vec![]);
    h.command(Command::CancelSymbolOrders {
        command: 2,
        symbol: "BTCUSDT".into(),
    });
    let cancel = h.take("/batch-cancel-orders");
    let payload: Value = serde_json::from_str(cancel.body.as_deref().unwrap()).unwrap();
    assert_eq!(payload["orderIdList"][0]["orderId"], "ack-only");
    batch_success(&mut h, cancel.id, &["ack-only"]);
    assert!(h.done.iter().any(|(id, r)| *id == 2 && r.is_ok()));
    h.command(Command::CancelSymbolOrders {
        command: 3,
        symbol: "BTCUSDT".into(),
    });
    assert!(
        !h.requests
            .iter()
            .any(|r| r.url.contains("/batch-cancel-orders"))
    );
}

#[test]
fn retained_conditional_cancels_require_their_own_semantic_ack() {
    for (uta_mode, kind, place_path, cancel_path) in [
        (
            false,
            "stop_market",
            "/place-tpsl-order",
            "/cancel-plan-order",
        ),
        (
            false,
            "trailing_stop_market",
            "/place-plan-order",
            "/cancel-plan-order",
        ),
        (
            true,
            "limit",
            "/api/v3/trade/place-order",
            "/api/v3/trade/cancel-order",
        ),
        (
            true,
            "stop_market",
            "/place-strategy-order",
            "/cancel-strategy-order",
        ),
    ] {
        for bad_ack in [
            r#"{"code":"00000","data":{}}"#,
            r#"{"code":"00000","data":{"orderId":"unrelated"}}"#,
        ] {
            let mut h = if uta_mode { uta() } else { Harness::classic() };
            place(
                &mut h,
                vec![
                    json!({"symbol":"BTCUSDT","type":kind,"side":"sell","amount":0.01,"price":95}),
                ],
            );
            let placed = h.take(place_path);
            h.reply(
                placed.id,
                200,
                r#"{"code":"00000","data":{"orderId":"retained-conditional"}}"#,
            );
            h.engine.store_mut().set_orders(vec![]);
            h.command(Command::CancelSymbolOrders {
                command: 2,
                symbol: "BTCUSDT".into(),
            });
            let (cancel, payload) = body(&mut h, cancel_path);
            if !uta_mode {
                assert_eq!(payload["orderIdList"][0]["orderId"], "retained-conditional");
                assert!(payload.get("orderId").is_none());
                if kind == "trailing_stop_market" {
                    assert_eq!(payload["planType"], "track_plan");
                }
            } else {
                assert_eq!(payload["orderId"], "retained-conditional");
            }
            if !uta_mode {
                assert_eq!(payload["symbol"], "BTCUSDT");
            }
            h.reply(cancel, 200, bad_ack);
            if let Some(i) = h
                .requests
                .iter()
                .position(|r| r.url.contains("orderId=retained-conditional"))
            {
                // A classic empty plan acknowledgement reads the plan back; still listed.
                let confirm = h.requests.remove(i);
                h.reply(confirm.id, 200, r#"{"code":"00000","data":{"entrustedList":[{"orderId":"retained-conditional","symbol":"BTCUSDT","planStatus":"live"}],"endId":"retained-conditional"}}"#);
            }
            assert!(
                h.done
                    .iter()
                    .any(|(id, result)| *id == 2 && result.is_err()),
                "{kind}: {bad_ack}"
            );
            // An ambiguous cancel does not consume the only retained native route.
            h.command(Command::CancelSymbolOrders {
                command: 3,
                symbol: "BTCUSDT".into(),
            });
            let (cancel, _) = body(&mut h, cancel_path);
            h.reply(
                cancel,
                200,
                r#"{"code":"00000","data":{"orderId":"retained-conditional"}}"#,
            );
            if h.requests
                .iter()
                .any(|r| r.url.contains("cancel-symbol-order"))
            {
                h.respond(
                    "cancel-symbol-order",
                    200,
                    r#"{"code":"00000","data":{"list":[]}}"#,
                );
            }
            assert!(h.done.iter().any(|(id, result)| *id == 3 && result.is_ok()));
            h.command(Command::CancelSymbolOrders {
                command: 4,
                symbol: "BTCUSDT".into(),
            });
            assert!(!h.requests.iter().any(|r| r.url.contains(cancel_path)));
        }
    }
}

#[test]
fn ambiguous_placement_ack_cannot_be_followed_by_false_empty_cancel_all_success() {
    let mut h = Harness::classic();
    h.engine.store_mut().set_orders(vec![]);
    place(&mut h, vec![limit()]);
    let placed = h.take("/batch-place-order");
    h.reply(placed.id, 200, r#"{"code":"00000","data":{}}"#);
    assert!(
        h.done
            .iter()
            .any(|(id, result)| *id == 1 && result.is_err())
    );
    h.command(Command::CancelSymbolOrders {
        command: 2,
        symbol: "BTCUSDT".into(),
    });
    assert!(
        h.done
            .iter()
            .any(|(id, result)| *id == 2 && result.is_err()),
        "a missing native ID must not mean the order was never placed"
    );
}

#[test]
fn delayed_post_write_recovery_preserves_authentication_backoff_and_recovery_hint() {
    let mut h = Harness::classic();
    place(&mut h, vec![limit()]);
    let (request, _) = body(&mut h, "/batch-place-order");
    batch_success(&mut h, request, &["recovery-order"]);
    let write_check = h
        .timers
        .iter()
        .rev()
        .find(|(_, at)| *at == h.now + 5_000.0)
        .unwrap()
        .0;
    let mut socket = h
        .sockets
        .iter()
        .rev()
        .find(|(_, url)| url.ends_with("/private"))
        .unwrap()
        .0;
    let mut waiting = None;
    for attempt in 0..4 {
        h.now += 1.0;
        h.event(Event::WsMessage {
            socket,
            text: r#"{"event":"login","code":"40009","msg":"bad key"}"#,
        });
        let nominal = 1000.0 * 2f64.powi(attempt);
        let pending = *h
            .timers
            .iter()
            .rev()
            .find(|(_, at)| (nominal * 0.75..=nominal).contains(&(*at - h.now)))
            .unwrap();
        if attempt == 3 {
            waiting = Some(pending);
            break;
        }
        h.now = pending.1;
        h.event(Event::TimerFired { timer: pending.0 });
        socket = h
            .sockets
            .iter()
            .rev()
            .find(|(_, url)| url.ends_with("/private"))
            .unwrap()
            .0;
        h.event(Event::WsOpened { socket });
    }
    let (retry, at) = waiting.unwrap();
    assert!(at - h.now >= 6_000.0);
    let before = h.sockets.len();
    h.event(Event::TimerFired { timer: write_check });
    assert_eq!(
        h.sockets.len(),
        before,
        "write recovery must honor pending authentication backoff"
    );
    assert!(
        h.timers.iter().any(|(timer, _)| *timer == retry),
        "pending retry must remain armed"
    );
    h.now = at;
    h.event(Event::TimerFired { timer: retry });
    assert_eq!(h.sockets.len(), before + 1);
    assert!(
        h.socket_contexts
            .last()
            .unwrap()
            .2
            .as_ref()
            .is_some_and(|c| c.feed == "private" && c.recover),
        "deferred reconnect must retain recovery hint"
    );
}

#[test]
fn split_batch_and_uta_receipts_match_only_dispatched_native_ids() {
    for unified in [false, true] {
        let mut h = if unified { uta() } else { Harness::classic() };
        h.engine.store_mut().update_markets(|m| {
            m.precision.amount = 1.0;
            m.limits.amount.max = 3.0;
        });
        let mut o = limit();
        o["amount"] = json!(7);
        place(&mut h, vec![o]);
        let mut ids = Vec::new();
        for _ in 0..if unified { 4 } else { 1 } {
            let request = h.take(if unified {
                "/place-order"
            } else {
                "/batch-place-order"
            });
            let p: Value = serde_json::from_str(request.body.as_deref().unwrap()).unwrap();
            let count = h
                .progress_at_request
                .iter()
                .find(|(id, _)| *id == request.id)
                .unwrap()
                .1;
            let (command, receipts) = &h.progress[count - 1];
            assert_eq!(*command, 1);
            let actual: Vec<_> = if unified {
                vec![p["clientOid"].as_str().unwrap()]
            } else {
                p["orderList"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|o| o["clientOid"].as_str().unwrap())
                    .collect()
            };
            assert_eq!(
                receipts
                    .iter()
                    .map(|r| r.identity.as_str())
                    .collect::<Vec<_>>(),
                actual
            );
            assert!(receipts.iter().all(|r| r.symbol == "BTCUSDT"));
            ids.extend(actual.into_iter().map(str::to_owned));
            if unified {
                h.reply(
                    request.id,
                    200,
                    r#"{"code":"00000","data":{"orderId":"created"}}"#,
                );
            } else {
                batch_success(&mut h, request.id, &["a", "b", "c", "d"]);
            }
        }
        let expected = 4;
        assert_eq!(ids.len(), expected);
        assert_eq!(
            ids.iter().collect::<std::collections::HashSet<_>>().len(),
            expected
        );
    }
}
#[test]
fn native_amend_receipt_excludes_unused_fallback_replacement() {
    let mut h = Harness::classic();
    h.command(Command::AmendOrder {
        command: 91,
        id: "1".into(),
        price: Some(95.0),
        amount: None,
    });
    let request = h.take("/modify-order");
    let p: Value = serde_json::from_str(request.body.as_deref().unwrap()).unwrap();
    assert_eq!(h.progress.len(), 1);
    assert_eq!(h.progress[0].0, 91);
    assert_eq!(h.progress[0].1.len(), 1);
    assert_eq!(h.progress[0].1[0].identity, p["newClientOid"]);
    assert_eq!(h.progress[0].1[0].symbol, "BTCUSDT");
    h.reply(
        request.id,
        200,
        r#"{"code":"40017","msg":"cannot modify","data":null}"#,
    );
    let cancel = h.take("/batch-cancel-orders");
    h.reply(cancel.id,200,r#"{"code":"00000","data":{"successList":[],"failureList":[{"orderId":"1","errorMsg":"cancel failed"}]}}"#);
    assert_eq!(h.progress.len(), 1);
}

#[test]
fn cancellation_missing_from_store_errors_before_any_write_even_in_a_mixed_batch() {
    use safe_cex_engine::model::{Order, OrderSide, OrderStatus, OrderType};
    use safe_cex_engine::{Action, Notice};
    for mixed in [false, true] {
        let mut h = Harness::classic();
        h.engine.store_mut().set_orders(vec![Order {
            id: "123".into(),
            parent_id: None,
            status: OrderStatus::Open,
            symbol: "BTCUSDT".into(),
            kind: OrderType::Limit,
            side: OrderSide::Buy,
            price: 90.0,
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
        }]);
        h.engine.drain_actions().for_each(drop);
        h.engine.handle(
            2000.0,
            Event::Command(Command::CancelOrders {
                command: 991,
                ids: if mixed {
                    vec!["123".into(), "456".into()]
                } else {
                    vec!["456".into()]
                },
            }),
        );
        let actions = plain(h.engine.drain_actions());
        assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
        assert!(
            actions.iter().any(|a| matches!(
                a,
                Action::Notice(Notice::CommandFailed { command: 991, error })
                    if error.legacy_text().contains("456") && error.legacy_text().contains("not found")
            )),
            "missing cancellation error: {actions:?}"
        );
        assert_eq!(
            h.engine.store().order("123").unwrap().status,
            OrderStatus::Open
        );
    }
}

#[test]
fn acknowledged_cancel_route_survives_missing_store_preflight_but_not_an_unknown_mixed_id() {
    use safe_cex_engine::{Action, Notice};

    for mixed in [false, true] {
        let mut h = Harness::classic();
        place(&mut h, vec![limit()]);
        let placed = h.take("/batch-place-order");
        batch_success(&mut h, placed.id, &["ack-only"]);
        assert!(h.engine.store().order("ack-only").is_none());
        h.engine.drain_actions().for_each(drop);
        h.engine.handle(
            2000.0,
            Event::Command(Command::CancelOrders {
                command: 992,
                ids: if mixed {
                    vec!["ack-only".into(), "missing".into()]
                } else {
                    vec!["ack-only".into()]
                },
            }),
        );
        let out = plain(
            safe_cex_engine::test_support::prepared_actions(&mut h.engine, 2000.0).into_iter(),
        );
        if mixed {
            assert!(!out.iter().any(|a| matches!(a, Action::Http(_))));
            assert!(out.iter().any(|a| matches!(
                a,
                Action::Notice(Notice::CommandFailed { command: 992, error })
                    if error.legacy_text().contains("missing") && error.legacy_text().contains("not found")
            )));
        } else {
            let cancel = out
                .iter()
                .find_map(|a| match a {
                    Action::Http(r) if r.url.contains("/batch-cancel-orders") => Some(r),
                    _ => None,
                })
                .expect("acknowledged native identity is sufficient before a stream echo");
            let payload: Value = serde_json::from_str(cancel.body.as_deref().unwrap()).unwrap();
            assert_eq!(payload["orderIdList"][0]["orderId"], "ack-only");
        }
    }
}

#[test]
fn app_protective_payload_with_matching_trigger_price_places_like_price_only() {
    let app = |kind: &str, price: f64, trigger: f64| json!({"symbol":"BTCUSDT","side":"sell","type":kind,"notional":100,"timeInForce":"GoodTillCancel","reduceOnly":true,"price":price,"triggerPrice":trigger,"amount":1});
    for uta_mode in [false, true] {
        let path = if uta_mode {
            "/place-strategy-order"
        } else {
            "/place-tpsl-order"
        };
        for (kind, price) in [("stop_market", 80.0), ("take_profit_market", 120.0)] {
            let mut sent = vec![];
            let mut price_only = app(kind, price, price);
            price_only.as_object_mut().unwrap().remove("triggerPrice");
            for o in [app(kind, price, price), price_only] {
                let mut h = if uta_mode { uta() } else { Harness::classic() };
                place(&mut h, vec![o]);
                let (_, mut p) = body(&mut h, path);
                p.as_object_mut().unwrap().remove("clientOid");
                sent.push(p);
            }
            let field = match (uta_mode, kind) {
                (false, _) => "triggerPrice",
                (true, "stop_market") => "stopLoss",
                (true, _) => "takeProfit",
            };
            assert_eq!(sent[0][field], price.to_string(), "{kind} {uta_mode}");
            assert_eq!(sent[0], sent[1], "{kind} {uta_mode}");
        }
        let mut h = if uta_mode { uta() } else { Harness::classic() };
        place(&mut h, vec![app("stop_market", 80.0, 79.0)]);
        assert!(h.done[0].1.is_err());
        assert!(h.requests.iter().all(|r| r.method == Method::Get));
    }
}

/// Failures as the plain text the host renders.
fn plain(actions: impl Iterator<Item = safe_cex_engine::Action>) -> Vec<safe_cex_engine::Action> {
    actions
        .map(|a| match a {
            safe_cex_engine::Action::Notice(safe_cex_engine::Notice::CommandFailed {
                command,
                error,
            }) => safe_cex_engine::Action::Notice(safe_cex_engine::Notice::CommandFailed {
                command,
                error: error.clone(),
            }),
            a => a,
        })
        .collect()
}

#[test]
fn write_failures_are_classified_without_changing_their_text() {
    use safe_cex_engine::{ErrorKind, Outcome};
    let unknown = (None, Some(Outcome::Unknown));
    for (status, body, classified) in [
        (
            400,
            r#"{"code":"40762","msg":"The order amount exceeds the balance"}"#,
            (
                Some(ErrorKind::InsufficientBalance),
                Some(Outcome::Rejected),
            ),
        ),
        (
            429,
            r#"{"code":"429","msg":"Too Many Requests"}"#,
            (Some(ErrorKind::RateLimited), Some(Outcome::Unknown)),
        ),
        // Ruled 2026-10-25: a 5xx write is the engine's unproven answer.
        (
            503,
            r#"{"code":"50000","msg":"busy"}"#,
            (Some(ErrorKind::VenueUnavailable), Some(Outcome::Unknown)),
        ),
        (200, r#"{"code":"45001","msg":"unknown"}"#, unknown),
        // Ruled 2026-10-25: Bitget lists every order as not placed, so Rejected.
        (
            200,
            r#"{"code":"00000","data":{"successList":[],"failureList":[{"errorMsg":"balance"}]}}"#,
            (Some(ErrorKind::Rejected), Some(Outcome::Rejected)),
        ),
        // A coded failed row keeps its code, so the table names the kind.
        (
            200,
            r#"{"code":"00000","data":{"successList":[],"failureList":[{"errorMsg":"Insufficient balance","errorCode":"43012"}]}}"#,
            (
                Some(ErrorKind::InsufficientBalance),
                Some(Outcome::Rejected),
            ),
        ),
        (
            200,
            r#"{"code":"00000","data":{"successList":[],"failureList":[{"errorMsg":"The order amount exceeds the balance","errorCode":"40762"}]}}"#,
            (
                Some(ErrorKind::InsufficientBalance),
                Some(Outcome::Rejected),
            ),
        ),
        // The reduce-only position refusal, by its code, its text, or both;
        // the text wins over an absent or different code, in any case.
        (
            200,
            r#"{"code":"00000","data":{"successList":[],"failureList":[{"errorMsg":"Not enough position is available","errorCode":"40757"}]}}"#,
            (Some(ErrorKind::InvalidInput), Some(Outcome::Rejected)),
        ),
        (
            200,
            r#"{"code":"00000","data":{"successList":[],"failureList":[{"errorMsg":"position","errorCode":"40757"}]}}"#,
            (Some(ErrorKind::InvalidInput), Some(Outcome::Rejected)),
        ),
        (
            200,
            r#"{"code":"00000","data":{"successList":[],"failureList":[{"errorMsg":"Not enough position is available"}]}}"#,
            (Some(ErrorKind::InvalidInput), Some(Outcome::Rejected)),
        ),
        (
            200,
            r#"{"code":"00000","data":{"successList":[],"failureList":[{"errorMsg":"not enough position is available.","errorCode":"40999"}]}}"#,
            (Some(ErrorKind::InvalidInput), Some(Outcome::Rejected)),
        ),
        (
            400,
            r#"{"code":"40757","msg":"position"}"#,
            (Some(ErrorKind::InvalidInput), Some(Outcome::Rejected)),
        ),
        (
            400,
            r#"{"code":"40999","msg":"Not enough position is available"}"#,
            (Some(ErrorKind::InvalidInput), Some(Outcome::Rejected)),
        ),
        // The text never upgrades an unproven code to a refusal.
        (
            200,
            r#"{"code":"00000","data":{"successList":[],"failureList":[{"errorMsg":"Not enough position is available","errorCode":"40010"}]}}"#,
            (None, Some(Outcome::Unknown)),
        ),
        // Per-row outcome change: a row coded with an unproven code (40010,
        // 40725, 45001) is now Unknown, where a code-less row was Rejected.
        (
            200,
            r#"{"code":"00000","data":{"successList":[],"failureList":[{"errorMsg":"Request timed out","errorCode":"40010"}]}}"#,
            (None, Some(Outcome::Unknown)),
        ),
        // An acknowledgement that accounts for no order is the engine's unproven reply.
        (
            200,
            r#"{"code":"00000","data":{"successList":[],"failureList":[]}}"#,
            (Some(ErrorKind::Internal), Some(Outcome::Unknown)),
        ),
    ] {
        let mut h = Harness::classic();
        place(&mut h, vec![limit()]);
        let request = h.take("/batch-place-order");
        h.reply(request.id, status, body);
        let [error] = std::mem::take(&mut h.failures).try_into().unwrap();
        assert_eq!(
            (error.kind, error.outcome),
            classified,
            "{status} {body}: {}",
            error.message
        );
        assert_eq!(h.done.last().unwrap().1, Err(error.legacy_text()));
        if status == 400 {
            let own: serde_json::Value = serde_json::from_str(body).unwrap();
            assert_eq!(error.venue_code.as_deref(), own["code"].as_str(), "{body}");
        }
    }
    // Transport loss on a write is Unknown.
    let mut h = Harness::classic();
    place(&mut h, vec![limit()]);
    let request = h.take("/batch-place-order");
    h.event(Event::HttpFailed {
        request: request.id,
        error: "reset",
    });
    assert_eq!(
        (h.failures[0].kind, h.failures[0].outcome),
        (Some(ErrorKind::Network), Some(Outcome::Unknown))
    );
    // Preflight refusals were never sent.
    let mut h = Harness::classic();
    let mut bad = limit();
    bad["clientOrderId"] = json!("bad id");
    place(&mut h, vec![bad]);
    assert_eq!(h.failures[0].outcome, Some(Outcome::NotSent));
}

/// A 40009 (signature) row inside a batch is a refusal of that row only: the
/// one fresh resend applies to a refused request, never to one batch row, so
/// the batch is dispatched once.
#[test]
fn a_signature_refused_batch_row_is_never_re_dispatched() {
    use safe_cex_engine::{ErrorKind, Outcome};
    let mut h = Harness::classic();
    place(&mut h, vec![limit()]);
    let request = h.take("/batch-place-order");
    h.reply(
        request.id,
        200,
        r#"{"code":"00000","data":{"successList":[],"failureList":[{"errorMsg":"sign signature error","errorCode":"40009"}]}}"#,
    );
    let [error] = std::mem::take(&mut h.failures).try_into().unwrap();
    assert_eq!(
        (error.kind, error.outcome),
        (Some(ErrorKind::AuthInvalid), Some(Outcome::Rejected))
    );
    assert!(
        !h.requests
            .iter()
            .any(|r| r.url.contains("/batch-place-order")),
        "one batch request, no per-row resend"
    );
}

/// Per-row outcome change: an unproven code (45001) on one row beside an
/// accepted row leaves that row unknown, so the command is a partial with an
/// unknown outcome (reconciled), never a definitive refusal.
#[test]
fn an_unproven_batch_row_beside_an_accepted_one_is_partial_unknown() {
    use safe_cex_engine::Outcome;
    let mut h = Harness::classic();
    place(&mut h, vec![limit(), limit()]);
    let request = h.take("/batch-place-order");
    h.reply(
        request.id,
        200,
        r#"{"code":"00000","data":{"successList":[{"orderId":"9"}],"failureList":[{"errorMsg":"unknown error","errorCode":"45001"}]}}"#,
    );
    let [error] = std::mem::take(&mut h.failures).try_into().unwrap();
    assert_eq!(error.outcome, Some(Outcome::Unknown), "{}", error.message);
    let partial = error.partial.as_deref().expect("the accepted id is kept");
    assert_eq!(partial.order_ids, ["9"]);
}

#[test]
fn a_symbol_cancel_finding_an_order_gone_succeeds() {
    let mut h = Harness::classic();
    place(&mut h, vec![limit()]);
    let placed = h.take("/batch-place-order");
    batch_success(&mut h, placed.id, &["ack-only"]);
    h.engine.store_mut().set_orders(vec![]);
    h.command(Command::CancelSymbolOrders {
        command: 2,
        symbol: "BTCUSDT".into(),
    });
    let cancel = h.take("/batch-cancel-orders");
    h.reply(
        cancel.id,
        200,
        r#"{"code":"25204","msg":"Order does not exist"}"#,
    );
    assert!(
        h.done.iter().any(|(id, r)| *id == 2 && r.is_ok()),
        "{:?}",
        h.done
    );
}

#[test]
fn a_replacement_refused_after_the_cancel_says_the_order_is_gone() {
    let mut h = Harness::classic();
    h.command(Command::AmendOrder {
        command: 1,
        id: "1".into(),
        price: Some(95.0),
        amount: None,
    });
    h.respond(
        "/modify-order",
        200,
        r#"{"code":"40017","msg":"cannot modify","data":null}"#,
    );
    let (request, _) = body(&mut h, "/batch-cancel-orders");
    h.reply(
        request,
        200,
        r#"{"code":"00000","data":{"successList":[{"orderId":"1"}],"failureList":[]}}"#,
    );
    let (request, _) = body(&mut h, "/batch-place-order");
    h.reply(
        request,
        200,
        r#"{"code":"45110","msg":"less than the minimum amount 5 USDT"}"#,
    );
    let error = h.done[0].1.clone().unwrap_err();
    assert!(error.contains("Your BTCUSDT order is gone"), "{error}");
}

#[test]
fn an_amend_below_the_venue_minimum_value_is_refused_before_any_cancel() {
    let mut h = Harness::classic();
    h.command(Command::AmendOrder {
        command: 1,
        id: "1".into(),
        price: Some(1.0),
        amount: None,
    });
    let error = h.done[0].1.clone().unwrap_err();
    assert!(error.contains("the order is unchanged"), "{error}");
    assert!(!h.requests.iter().any(|r| r.url.contains("cancel")));
}

#[test]
fn an_amend_finding_its_order_gone_does_not_place_the_replacement() {
    let mut h = Harness::classic();
    h.command(Command::AmendOrder {
        command: 1,
        id: "1".into(),
        price: Some(95.0),
        amount: None,
    });
    h.respond(
        "/modify-order",
        200,
        r#"{"code":"40017","msg":"cannot modify","data":null}"#,
    );
    let (request, _) = body(&mut h, "/batch-cancel-orders");
    h.reply(
        request,
        200,
        r#"{"code":"25204","msg":"Order does not exist"}"#,
    );
    assert!(h.done[0].1.is_err());
    assert!(
        !h.requests
            .iter()
            .any(|r| r.url.contains("/batch-place-order"))
    );
}

#[test]
fn split_remainder_stays_on_the_lot_step() {
    let mut h = Harness::classic();
    let mut market = h.engine.store().market("BTCUSDT").unwrap().clone();
    market.precision.amount = 0.1;
    market.limits.amount.min = 0.1;
    market.limits.amount.max = 72000.0;
    h.engine.store_mut().set_markets(vec![market]);
    place(
        &mut h,
        vec![
            json!({"symbol":"BTCUSDT","type":"limit","side":"buy","amount":180000.05,"price":90.0}),
        ],
    );
    let (_, payload) = body(&mut h, "/batch-place-order");
    let sizes: Vec<_> = payload["orderList"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["size"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(sizes, ["48000", "48000", "48000", "36000.1"]);
}

fn account_cancel(h: &mut Harness, command: u32) {
    h.command(Command::Venue {
        command,
        action: safe_cex_engine::VenueAction::CancelAccountOrders,
    });
}

#[test]
fn a_nuke_account_cancel_on_classic_sends_the_product_wide_cancel_all() {
    let mut h = Harness::classic();
    account_cancel(&mut h, 21);
    let (request, sent) = body(&mut h, "/api/v2/mix/order/cancel-all-orders");
    assert_eq!(sent["productType"], "usdt-futures");
    assert_eq!(sent["marginCoin"], "USDT");
    assert!(sent.get("symbol").is_none(), "{sent}");
    h.reply(
        request,
        200,
        r#"{"code":"00000","msg":"success","data":{"successList":[],"failureList":[]}}"#,
    );
    // Plan orders (triggers, TP/SL, trailing) are not normal orders: each type is
    // cancelled product-wide, so one the store never saw is reached too.
    for plan in [
        "normal_plan",
        "profit_plan",
        "loss_plan",
        "pos_profit",
        "pos_loss",
        "track_plan",
        "moving_plan",
    ] {
        assert!(h.done.iter().all(|(id, _)| *id != 21), "{plan}");
        let (request, sent) = body(&mut h, "/api/v2/mix/order/cancel-plan-order");
        assert_eq!(sent["planType"], plan);
        assert_eq!(sent["productType"], "usdt-futures");
        assert_eq!(sent["marginCoin"], "USDT");
        assert!(sent.get("symbol").is_none(), "{sent}");
        h.reply(
            request,
            200,
            r#"{"code":"00000","msg":"success","data":{"successList":[],"failureList":[]}}"#,
        );
    }
    assert!(
        h.done
            .iter()
            .any(|(id, result)| *id == 21 && result.is_ok()),
        "{:?}",
        h.done
    );
}

#[test]
fn a_nuke_account_cancel_on_uta_cancels_the_whole_category() {
    let mut h = uta();
    account_cancel(&mut h, 22);
    let (request, sent) = body(&mut h, "cancel-symbol-order");
    assert_eq!(sent["category"], "USDT-FUTURES");
    assert!(sent.get("symbol").is_none(), "{sent}");
    h.reply(
        request,
        200,
        r#"{"code":"00000","msg":"success","data":{"list":[]}}"#,
    );
    assert!(
        h.done
            .iter()
            .any(|(id, result)| *id == 22 && result.is_ok()),
        "{:?}",
        h.done
    );
}

#[test]
fn a_symbol_cancel_never_sends_the_account_wide_cancel() {
    for mut h in [Harness::classic(), uta()] {
        h.command(Command::CancelSymbolOrders {
            command: 23,
            symbol: String::new(),
        });
        assert!(
            !h.requests
                .iter()
                .any(|r| r.url.contains("cancel-all-orders")
                    || r.url.contains("cancel-symbol-order"))
        );
    }
}

fn protective_plan_orders(plan: &str, size: &str, side: &str) -> Harness {
    let mut h = Harness::classic();
    let hold_side = if side == "sell" { "long" } else { "short" };
    h.private_message(
        &json!({
            "arg": {"channel":"orders-algo"},
            "data": (["target-plan", "sibling-plan"].map(|id| json!({
                "orderId":id,"clientOid":format!("client-{id}"),"symbol":"BTCUSDT","size":size,"triggerPrice":"120",
                "side":side,"holdSide":hold_side,"planType":plan,"status":"live"
            })))
        })
        .to_string(),
    );
    assert!(h.engine.store().order("target-plan").is_some(), "{plan}");
    h
}

#[test]
fn classic_protective_cancels_address_one_order_with_the_documented_plan_list() {
    for (raw, expected, size) in [
        ("pos_profit", "pos_profit", "0"),
        ("ptp", "pos_profit", "0"),
        ("pos_loss", "pos_loss", "0"),
        ("psl", "pos_loss", "0"),
        ("profit_plan", "profit_plan", "1"),
        ("tp", "profit_plan", "1"),
        ("loss_plan", "loss_plan", "1"),
        ("sl", "loss_plan", "1"),
        ("moving_plan", "moving_plan", "1"),
        ("track_plan", "track_plan", "1"),
        ("track", "track_plan", "1"),
    ] {
        for side in ["sell", "buy"] {
            let mut h = protective_plan_orders(raw, size, side);
            h.engine.store_mut().set_positions(vec![]);
            h.command(Command::CancelOrders {
                command: 91,
                ids: vec!["target-plan".into()],
            });
            let (request, payload) = body(&mut h, "/api/v2/mix/order/cancel-plan-order");
            assert_eq!(payload["planType"], expected, "{raw} {side}");
            assert_eq!(
                payload["orderIdList"],
                json!([{"orderId":"target-plan"}]),
                "{raw} {side}: {payload}"
            );
            assert!(payload.get("orderId").is_none(), "{payload}");
            assert_eq!(payload["symbol"], "BTCUSDT");
            assert_eq!(payload["productType"], "usdt-futures");
            assert_eq!(payload["marginCoin"], "USDT");
            batch_success(&mut h, request, &["target-plan"]);
            assert!(
                h.done
                    .iter()
                    .any(|(id, result)| *id == 91 && result.is_ok()),
                "{:?}",
                h.done
            );
            assert!(
                h.engine
                    .store()
                    .order("target-plan")
                    .is_none_or(|o| o.status != OrderStatus::Open)
            );
            assert_eq!(
                h.engine.store().order("sibling-plan").unwrap().status,
                OrderStatus::Open
            );
        }
    }
}

#[test]
fn whole_position_tp_and_sl_cancel_resolve_only_when_the_requested_id_is_acknowledged() {
    for plan in ["pos_profit", "pos_loss"] {
        for side in ["sell", "buy"] {
            for acknowledgement in [
                "addressed",
                "client-only",
                "native-with-malformed-client",
                "empty",
                "unrelated",
                "unrelated-client",
                "conflicting-ids",
                "refused",
            ] {
                let mut h = protective_plan_orders(plan, "0", side);
                h.command(Command::CancelOrders {
                    command: 92,
                    ids: vec!["target-plan".into()],
                });
                let (request, payload) = body(&mut h, "/api/v2/mix/order/cancel-plan-order");
                // Only the documented per-order selector proves the intended request reached the venue.
                let addressed = payload["orderIdList"] == json!([{"orderId":"target-plan"}]);
                let accepted = match acknowledgement {
                    "addressed" if addressed => vec![json!({"orderId":"target-plan"})],
                    "client-only" if addressed => vec![json!({"clientOid":"client-target-plan"})],
                    "native-with-malformed-client" if addressed => {
                        vec![json!({"orderId":"target-plan","clientOid":{"unexpected":"value"}})]
                    }
                    "unrelated-client" => vec![json!({"clientOid":"client-sibling-plan"})],
                    "conflicting-ids" => {
                        vec![json!({"orderId":"another-plan", "clientOid":"client-target-plan"})]
                    }
                    "unrelated" => vec![json!({"orderId":"another-plan"})],
                    _ => vec![],
                };
                let failures = if acknowledgement == "refused" {
                    vec![json!({"orderId":"target-plan","errorMsg":"plan not cancellable"})]
                } else {
                    vec![]
                };
                h.reply(
                    request,
                    200,
                    &json!({"code":"00000","data":{"successList":accepted,"failureList":failures}})
                        .to_string(),
                );
                if let Some(i) = h
                    .requests
                    .iter()
                    .position(|r| r.url.contains("orderId=target-plan"))
                {
                    // An empty acknowledgement reads the plan back; still listed.
                    let confirm = h.requests.remove(i);
                    h.reply(confirm.id, 200, r#"{"code":"00000","data":{"entrustedList":[{"orderId":"target-plan","symbol":"BTCUSDT","planStatus":"live"}],"endId":"target-plan"}}"#);
                }
                let result = &h.done.iter().find(|(id, _)| *id == 92).unwrap().1;
                let proven = matches!(
                    acknowledgement,
                    "addressed" | "client-only" | "native-with-malformed-client"
                );
                assert_eq!(
                    result.is_ok(),
                    proven,
                    "{plan} {side} {acknowledgement}: {payload}; {result:?}"
                );
                if !proven {
                    assert_eq!(
                        h.engine.store().order("target-plan").unwrap().status,
                        OrderStatus::Open
                    );
                }
                assert_eq!(
                    h.engine.store().order("sibling-plan").unwrap().status,
                    OrderStatus::Open
                );
            }
        }
    }
}

#[test]
fn retained_whole_position_cancels_keep_their_plan_and_client_identity_before_private_echo() {
    for (kind, plan) in [
        ("take_profit_market", "pos_profit"),
        ("stop_market", "pos_loss"),
    ] {
        let mut h = Harness::classic();
        place(
            &mut h,
            vec![json!({"symbol":"BTCUSDT","type":kind,"side":"sell","amount":0,"price":120})],
        );
        let placed = h.take("/place-tpsl-order");
        let placed_payload: Value = serde_json::from_str(placed.body.as_deref().unwrap()).unwrap();
        let client = placed_payload["clientOid"].as_str().unwrap();
        h.reply(
            placed.id,
            200,
            r#"{"code":"00000","data":{"orderId":"unseen-plan"}}"#,
        );
        h.engine.store_mut().set_orders(vec![]);
        h.command(Command::CancelSymbolOrders {
            command: 93,
            symbol: "BTCUSDT".into(),
        });
        let (request, payload) = body(&mut h, "/cancel-plan-order");
        assert_eq!(payload["orderIdList"], json!([{"orderId":"unseen-plan"}]));
        assert_eq!(payload["planType"], plan);
        assert!(payload.get("orderId").is_none());
        h.reply(
            request,
            200,
            &json!({"code":"00000","data":{"successList":[{"clientOid":client}],"failureList":[]}})
                .to_string(),
        );
        assert!(
            h.done
                .iter()
                .any(|(id, result)| *id == 93 && result.is_ok()),
            "{:?}",
            h.done
        );
    }
}
