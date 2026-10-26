mod support;
mod trusted;
use safe_cex_engine::model::PlaceOrderOpts;
use safe_cex_engine::{Action, Command, Event, Notice};
use support::*;
fn place(h: &mut Harness, raw: &str) {
    let orders: Vec<PlaceOrderOpts> = serde_json::from_str(raw).unwrap();
    h.handle(
        1.0,
        Event::Command(Command::PlaceOrders { command: 7, orders }),
    );
    h.collect();
}
fn done(h: &Harness) -> bool {
    h.actions
        .iter()
        .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 7, .. })))
}
/// No write follows; the only request is the uncertain write's reconcile snapshot.
fn only_reconcile_pending(h: &Harness) {
    let urls: Vec<_> = h.pending.iter().map(|r| r.url.as_str()).collect();
    assert_eq!(
        urls,
        ["https://openapi.blofin.com/api/v1/trade/orders-pending?limit=100"]
    );
}
fn seed_order(h: &mut Harness, id: &str, extra: &str) {
    h.ws(&format!(r#"{{"arg":{{"channel":"orders"}},"data":[{{"instId":"BTC-USDT","orderCategory":"normal","orderId":"{id}","orderType":"limit","side":"buy","marginMode":"cross","positionSide":"net","size":"10","price":"60000","state":"live"{extra}}}]}}"#));
}
#[test]
fn bybit_execution_controls_reject_entire_batch_before_writes() {
    for (field, value) in [
        (
            "bybitBbo",
            serde_json::json!({"sideType":"Queue","level":1}),
        ),
        ("bybitSlippageToleranceTicks", serde_json::json!(2)),
    ] {
        let mut h = Harness::new();
        h.seed();
        let mut orders = serde_json::json!([
            {"symbol":"BTCUSDT","type":"limit","side":"buy","amount":1,"price":60000,"clientOrderId":"first"},
            {"symbol":"BTCUSDT","type":"limit","side":"buy","amount":1,"price":60000,"clientOrderId":"second"}
        ]);
        orders[1][field] = value;
        place(&mut h, &orders.to_string());
        assert!(h.pending.is_empty());
        assert!(h.e.store().orders().is_empty());
        assert!(h.actions.iter().any(|a| matches!(
            a,
            Action::Notice(Notice::CommandFailed { command: 7, error }) if error.legacy_text().contains("Bybit")
        )));
    }
}

#[test]
fn invalid_final_caller_id_rejects_whole_batch_before_any_write() {
    let mut h = Harness::new();
    h.seed();
    place(
        &mut h,
        r#"[{"symbol":"BTCUSDT","type":"limit","side":"buy","amount":1,"price":60000,"clientOrderId":"valid"},{"symbol":"BTCUSDT","type":"market","side":"buy","amount":1,"clientOrderId":"bad-id"}]"#,
    );
    assert!(h.pending.is_empty());
    assert!(done(&h));
    assert!(h.e.store().orders().is_empty());
}
#[test]
fn caller_id_is_verbatim_and_brackets_stay_on_normal_batch() {
    let mut h = Harness::new();
    h.seed();
    place(
        &mut h,
        r#"[{"symbol":"BTCUSDT","type":"limit","side":"buy","amount":1.23,"price":60000.04,"clientOrderId":"Bare123"},{"symbol":"BTCUSDT","type":"limit","side":"buy","amount":2,"price":60000,"takeProfit":65000,"stopLoss":55000}]"#,
    );
    let r = h.take("batch-orders");
    let p: serde_json::Value = serde_json::from_str(r.body.as_ref().unwrap()).unwrap();
    assert_eq!(p[0]["clientOrderId"], "Bare123");
    assert_eq!(p[0]["size"], "1.2");
    assert_eq!(p[0]["price"], "60000");
    assert_eq!(p[1]["attachAlgoOrders"][0]["slTriggerPrice"], "55000");
    h.respond(r,r#"{"code":"0","data":[{"orderId":"1","code":"0","msg":"Order placed"},{"orderId":"2","code":"0","msg":"success"}]}"#);
    assert!(!done(&h));
    assert!(h.e.store().orders().is_empty());
}
#[test]
fn lost_placement_response_is_never_retried() {
    let mut h = Harness::new();
    h.seed();
    place(
        &mut h,
        r#"[{"symbol":"BTCUSDT","type":"market","side":"buy","amount":1}]"#,
    );
    let r = h.take("batch-orders");
    h.handle(
        2.0,
        Event::HttpFailed {
            request: r.id,
            error: "response lost after acceptance",
        },
    );
    h.collect();
    assert!(done(&h));
    only_reconcile_pending(&h);
    assert!(h.e.store().orders().is_empty());
}
#[test]
fn successful_envelope_does_not_hide_mismatched_or_failed_cancel_rows() {
    let mut h = Harness::new();
    h.seed();
    seed_order(&mut h, "12", "");
    seed_order(&mut h, "123", "");
    h.handle(
        1.0,
        Event::Command(Command::CancelOrders {
            command: 7,
            ids: vec!["12".into()],
        }),
    );
    h.collect();
    h.reply(
        "cancel-batch-orders",
        r#"{"code":"0","data":[{"orderId":"123","code":"1000"}]}"#,
    );
    assert!(done(&h));
    assert!(h.e.store().order("12").is_some());
    assert!(h.e.store().order("123").is_some());
    h.actions.clear();
    h.handle(
        2.0,
        Event::Command(Command::CancelOrders {
            command: 7,
            ids: vec!["12".into()],
        }),
    );
    h.collect();
    h.reply(
        "cancel-batch-orders",
        r#"{"code":"0","data":[{"orderId":"12","code":"500","msg":"permission denied"}]}"#,
    );
    assert!(done(&h));
    assert!(h.e.store().order("12").is_some());
}
#[test]
fn already_gone_cancel_only_removes_correlated_identity() {
    let mut h = Harness::new();
    h.seed();
    seed_order(&mut h, "12", "");
    seed_order(&mut h, "123", "");
    h.handle(
        1.0,
        Event::Command(Command::CancelOrders {
            command: 7,
            ids: vec!["12".into()],
        }),
    );
    h.collect();
    h.reply("cancel-batch-orders",r#"{"code":"1","data":[{"orderId":"12","code":"1000","msg":"Cancel failed as the order has been filled, triggered, canceled or does not exist"}]}"#);
    assert!(!done(&h));
    assert!(h.e.store().order("12").is_none());
    assert!(h.e.store().order("123").is_some());
}
#[test]
fn rejected_cancel_prevents_amend_replacement() {
    let mut h = Harness::new();
    h.seed();
    seed_order(&mut h, "12", "");
    h.handle(
        1.0,
        Event::Command(Command::AmendOrder {
            command: 7,
            id: "12".into(),
            price: Some(61000.0),
            amount: None,
        }),
    );
    h.collect();
    h.reply(
        "cancel-batch-orders",
        r#"{"code":"0","data":[{"orderId":"12","code":"500","msg":"denied"}]}"#,
    );
    assert!(done(&h));
    only_reconcile_pending(&h);
}
#[test]
fn conditional_amend_preserves_nested_protection_and_native_execution_provenance() {
    let mut h = Harness::new();
    h.seed();
    h.ws(r#"{"arg":{"channel":"orders-algo"},"data":[{"instId":"BTC-USDT","algoId":"77","orderType":"trigger","side":"buy","size":"3","triggerPrice":"61000","orderPrice":"60900","triggerPriceType":"mark","marginMode":"isolated","positionSide":"long","state":"live","attachAlgoOrders":[{"tpTriggerPrice":"65000","tpOrderPrice":"-1","tpTriggerPriceType":"index","slTriggerPrice":"55000","slOrderPrice":"-1","slTriggerPriceType":"mark"}]}]}"#);
    h.handle(
        1.0,
        Event::Command(Command::AmendOrder {
            command: 7,
            id: "77".into(),
            price: Some(62000.0),
            amount: None,
        }),
    );
    h.collect();
    h.reply(
        "cancel-algo",
        r#"{"code":"0","data":{"algoId":"77","code":"0"}}"#,
    );
    let timer = h
        .actions
        .iter()
        .rev()
        .find_map(|a| {
            if let Action::SetTimer { timer, at_ms } = a
                && *at_ms == 350.0
            {
                Some(*timer)
            } else {
                None
            }
        })
        .unwrap();
    h.handle(350.0, Event::TimerFired { timer });
    h.collect();
    let r = h.take("order-algo");
    let p: serde_json::Value = serde_json::from_str(r.body.as_ref().unwrap()).unwrap();
    assert_eq!(p["triggerPrice"], "62000");
    assert_eq!(p["orderPrice"], "60900");
    assert_eq!(p["marginMode"], "isolated");
    assert_eq!(p["positionSide"], "long");
    assert_eq!(p["triggerPriceType"], "mark");
    assert_eq!(p["attachAlgoOrders"][0]["slTriggerPriceType"], "mark");
    h.respond(r, r#"{"code":"0","data":{"algoId":"88","code":"0"}}"#);
    assert!(!done(&h));
}
#[test]
fn settings_rejections_preserve_known_leverage() {
    let mut h = Harness::new();
    h.seed();
    h.handle(
        1.0,
        Event::Command(Command::SetLeverage {
            command: 7,
            symbol: "BTCUSDT".into(),
            leverage: 30.0,
            side: None,
        }),
    );
    h.collect();
    h.reply("set-leverage", r#"{"code":"500","msg":"denied"}"#);
    assert!(done(&h));
    assert_eq!(h.e.store().setting("BTCUSDT").unwrap().leverage, 20.0);
}
#[test]
fn split_sizing_preserves_native_quantity_and_stays_below_maximum() {
    let mut h = Harness::new();
    h.seed();
    place(
        &mut h,
        r#"[{"symbol":"BTCUSDT","type":"market","side":"buy","amount":1001}]"#,
    );
    let r = h.take("batch-orders");
    let p: serde_json::Value = serde_json::from_str(r.body.as_ref().unwrap()).unwrap();
    let sizes = p
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["size"].as_str().unwrap().parse::<f64>().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        sizes.iter().map(|s| (s * 10.0).round() as u64).sum::<u64>(),
        10010
    );
    assert!(sizes.iter().all(|s| *s <= 500.0));
}
#[test]
fn grouped_normal_batches_keep_their_own_native_receipt_slices() {
    let mut h = Harness::new();
    h.seed();
    let orders: Vec<_> = (0..21).map(|index| serde_json::json!({"symbol":"BTCUSDT","type":"limit","side":"buy","amount":1,"price":60000,"clientOrderId":format!("Caller{index}")})).collect();
    place(&mut h, &serde_json::to_string(&orders).unwrap());
    let first = h.take("batch-orders");
    let payload: serde_json::Value = serde_json::from_str(first.body.as_deref().unwrap()).unwrap();
    assert_eq!(payload.as_array().unwrap().len(), 20);
    let first_receipts = h
        .actions
        .iter()
        .find_map(|action| match action {
            Action::Notice(Notice::CommandProgress {
                command: 7,
                receipts,
            }) => Some(receipts),
            _ => None,
        })
        .unwrap();
    assert_eq!(first_receipts.len(), 20);
    assert_eq!(first_receipts[0].identity, "Caller0");
    assert_eq!(first_receipts[19].identity, "Caller19");
    let ack: Vec<_> = (0..20)
        .map(|index| serde_json::json!({"orderId":format!("Accepted{index}"),"code":"0"}))
        .collect();
    h.respond(
        first,
        &serde_json::json!({"code":"0","data":ack}).to_string(),
    );
    let timer = h
        .actions
        .iter()
        .rev()
        .find_map(|action| match action {
            Action::SetTimer { timer, at_ms } if *at_ms == 350. => Some(*timer),
            _ => None,
        })
        .unwrap();
    h.handle(350., Event::TimerFired { timer });
    h.collect();
    let second = h.take("batch-orders");
    let payload: serde_json::Value = serde_json::from_str(second.body.as_deref().unwrap()).unwrap();
    assert_eq!(payload.as_array().unwrap().len(), 1);
    assert_eq!(payload[0]["clientOrderId"], "Caller20");
    let second_receipts = h
        .actions
        .iter()
        .rev()
        .find_map(|action| match action {
            Action::Notice(Notice::CommandProgress {
                command: 7,
                receipts,
            }) => Some(receipts),
            _ => None,
        })
        .unwrap();
    assert_eq!(second_receipts.len(), 1);
    assert_eq!(second_receipts[0].symbol, "BTCUSDT");
    assert_eq!(second_receipts[0].identity, "Caller20");
}

#[test]
fn tiny_partial_protection_and_negative_amend_do_not_become_full_position() {
    let mut h = Harness::new();
    h.seed();
    place(
        &mut h,
        r#"[{"symbol":"BTCUSDT","type":"stop_market","side":"sell","amount":0.01,"price":55000}]"#,
    );
    assert!(done(&h));
    assert!(h.pending.is_empty());
    seed_order(&mut h, "12", "");
    h.actions.clear();
    h.handle(
        1.0,
        Event::Command(Command::AmendOrder {
            command: 7,
            id: "12".into(),
            price: None,
            amount: Some(-1.0),
        }),
    );
    h.collect();
    assert!(done(&h));
    assert!(h.pending.is_empty());
}
#[test]
fn full_position_combination_preserves_sibling_trigger_reference() {
    let mut h = Harness::new();
    h.seed();
    h.ws(r#"{"arg":{"channel":"orders-algo"},"data":[{"instId":"BTC-USDT","tpslId":"91","side":"sell","size":"-1","marginMode":"cross","positionSide":"net","tpTriggerPrice":"65000","tpOrderPrice":"-1","tpTriggerPriceType":"index","state":"live"}]}"#);
    place(
        &mut h,
        r#"[{"symbol":"BTCUSDT","type":"stop_market","side":"sell","amount":0,"price":55000}]"#,
    );
    h.reply(
        "cancel-tpsl",
        r#"{"code":"0","data":[{"tpslId":"91","code":"0","msg":null}]}"#,
    );
    let timer = h
        .actions
        .iter()
        .rev()
        .find_map(|a| {
            if let Action::SetTimer { timer, at_ms } = a
                && *at_ms == 1000.0
            {
                Some(*timer)
            } else {
                None
            }
        })
        .unwrap();
    h.handle(1000.0, Event::TimerFired { timer });
    h.collect();
    let r = h.take("order-tpsl");
    let p: serde_json::Value = serde_json::from_str(r.body.as_ref().unwrap()).unwrap();
    assert_eq!(p["size"], "-1");
    assert_eq!(p["tpTriggerPriceType"], "index");
    assert_eq!(p["slTriggerPrice"], "55000");
    h.respond(
        r,
        r#"{"code":"0","data":{"tpslId":"92","code":"0","msg":null}}"#,
    );
    assert!(!done(&h));
}
#[test]
fn cancel_one_combined_leg_recreates_only_its_sibling() {
    let mut h = Harness::new();
    h.seed();
    h.ws(r#"{"arg":{"channel":"orders-algo"},"data":[{"instId":"BTC-USDT","tpslId":"91","side":"sell","size":"-1","marginMode":"cross","positionSide":"net","tpTriggerPrice":"65000","tpOrderPrice":"-1","tpTriggerPriceType":"index","slTriggerPrice":"55000","slOrderPrice":"54900","slTriggerPriceType":"mark","state":"live"}]}"#);
    h.handle(
        1.0,
        Event::Command(Command::CancelOrders {
            command: 7,
            ids: vec!["91__take_profit".into()],
        }),
    );
    h.collect();
    h.reply(
        "cancel-tpsl",
        r#"{"code":"0","data":[{"tpslId":"91","code":"0","msg":null}]}"#,
    );
    let timer = h
        .actions
        .iter()
        .rev()
        .find_map(|a| {
            if let Action::SetTimer { timer, at_ms } = a
                && *at_ms == 1000.0
            {
                Some(*timer)
            } else {
                None
            }
        })
        .unwrap();
    h.handle(1000.0, Event::TimerFired { timer });
    h.collect();
    let r = h.take("order-tpsl");
    let p: serde_json::Value = serde_json::from_str(r.body.as_ref().unwrap()).unwrap();
    assert!(p.get("tpTriggerPrice").is_none());
    assert_eq!(p["slTriggerPrice"], "55000");
    assert_eq!(p["slTriggerPriceType"], "mark");
    assert_eq!(p["slOrderPrice"], "54900");
}
#[test]
fn normal_amend_preserves_brackets_before_fallback_children_exist() {
    let mut h = Harness::new();
    h.seed();
    seed_order(
        &mut h,
        "12",
        r#", "tpTriggerPrice":"65000", "tpOrderPrice":"64900", "tpTriggerPriceType":"index", "slTriggerPrice":"55000""#,
    );
    h.handle(
        1.0,
        Event::Command(Command::AmendOrder {
            command: 7,
            id: "12".into(),
            price: Some(61000.0),
            amount: None,
        }),
    );
    h.collect();
    h.reply(
        "cancel-batch-orders",
        r#"{"code":"0","data":[{"orderId":"12","code":"0","msg":null}]}"#,
    );
    let timer = h
        .actions
        .iter()
        .rev()
        .find_map(|a| {
            if let Action::SetTimer { timer, at_ms } = a
                && *at_ms == 350.0
            {
                Some(*timer)
            } else {
                None
            }
        })
        .unwrap();
    h.handle(350.0, Event::TimerFired { timer });
    h.collect();
    h.reply("order-detail", r#"{"code":"0","data":{"instId":"BTC-USDT","orderId":"12","state":"canceled","filledSize":"0"}}"#);
    let r = h.take("batch-orders");
    let p: serde_json::Value = serde_json::from_str(r.body.as_ref().unwrap()).unwrap();
    assert_eq!(p[0]["tpTriggerPrice"], "65000");
    assert_eq!(p[0]["tpOrderPrice"], "64900");
    assert_eq!(p[0]["attachAlgoOrders"][0]["tpTriggerPriceType"], "index");
    assert_eq!(p[0]["attachAlgoOrders"][0]["slTriggerPrice"], "55000");
}
#[test]
fn margin_default_change_preserves_live_position_margin() {
    use safe_cex_engine::model::MarginType;
    let mut h = Harness::new();
    h.seed();
    h.handle(
        1.0,
        Event::Command(Command::SetMarginType {
            command: 7,
            symbol: "BTCUSDT".into(),
            isolated: true,
        }),
    );
    h.collect();
    h.reply(
        "set-margin-mode",
        r#"{"code":"0","data":{"marginMode":"isolated"}}"#,
    );
    assert_eq!(
        h.e.store().positions()[0].margin_type,
        Some(MarginType::Cross)
    );
    assert_eq!(
        h.e.store().setting("BTCUSDT").unwrap().margin_type,
        Some(MarginType::Cross)
    );
}
#[test]
fn isolated_hedge_leverage_sends_both_sides_before_either_response() {
    let mut h = Harness::new();
    h.modes();
    h.reply("account/positions", r#"{"code":"0","data":[]}"#);
    h.reply(
        "balances",
        r#"{"code":"0","data":[{"available":"1000","frozen":"0"}]}"#,
    );
    for path in [
        "orders-pending",
        "orders-tpsl-pending",
        "orders-algo-pending",
    ] {
        h.reply(path, r#"{"code":"0","data":[]}"#);
    }
    h.handle(
        1.0,
        Event::Command(Command::SetMarginType {
            command: 8,
            symbol: "BTCUSDT".into(),
            isolated: true,
        }),
    );
    h.collect();
    h.reply(
        "set-margin-mode",
        r#"{"code":"0","data":{"marginMode":"isolated"}}"#,
    );
    h.handle(
        1.0,
        Event::Command(Command::SetPositionMode {
            command: 9,
            hedged: true,
            symbol: None,
        }),
    );
    h.collect();
    h.reply(
        "set-position-mode",
        r#"{"code":"0","data":{"positionMode":"long_short_mode"}}"#,
    );
    h.handle(
        2.0,
        Event::Command(Command::SetLeverage {
            command: 7,
            symbol: "BTCUSDT".into(),
            leverage: 30.0,
            side: None,
        }),
    );
    h.collect();
    let requests = h
        .pending
        .iter()
        .filter(|r| r.url.ends_with("set-leverage"))
        .collect::<Vec<_>>();
    assert_eq!(requests.len(), 2);
    assert!(
        requests
            .iter()
            .any(|r| r.body.as_ref().unwrap().contains("\"long\""))
    );
    assert!(
        requests
            .iter()
            .any(|r| r.body.as_ref().unwrap().contains("\"short\""))
    );
    h.reply("set-leverage", r#"{"code":"0","data":{"leverage":"30"}}"#);
    assert_ne!(h.e.store().setting("BTCUSDT").unwrap().leverage, 30.0);
    h.reply("set-leverage", r#"{"code":"0","data":{"leverage":"30"}}"#);
    assert_eq!(h.e.store().setting("BTCUSDT").unwrap().leverage, 30.0);
}

#[test]
fn editing_inline_virtual_protection_preserves_parent_remainder_despite_child_amount() {
    let mut h = Harness::new();
    h.modes();
    h.reply("account/positions", r#"{"code":"0","data":[]}"#);
    h.reply(
        "balances",
        r#"{"code":"0","data":[{"available":"1000","frozen":"0"}]}"#,
    );
    h.reply("orders-pending",r#"{"code":"0","data":[{"instId":"BTC-USDT","orderCategory":"normal","orderId":"12","orderType":"limit","side":"buy","marginMode":"cross","positionSide":"net","size":"10","filledSize":"4","price":"60000","state":"partially_filled","tpTriggerPrice":"65000","slTriggerPrice":"55000"}]}"#);
    for path in [
        "orders-pending",
        "orders-tpsl-pending",
        "orders-algo-pending",
    ] {
        h.reply(path, r#"{"code":"0","data":[]}"#);
    }
    h.handle(
        1.0,
        Event::Command(Command::AmendOrder {
            command: 7,
            id: "12__take_profit".into(),
            price: Some(66000.0),
            amount: Some(20.0),
        }),
    );
    h.collect();
    h.reply(
        "cancel-batch-orders",
        r#"{"code":"0","data":[{"orderId":"12","code":"0","msg":null}]}"#,
    );
    let timer = h
        .actions
        .iter()
        .rev()
        .find_map(|a| {
            if let Action::SetTimer { timer, at_ms } = a
                && *at_ms == 350.0
            {
                Some(*timer)
            } else {
                None
            }
        })
        .unwrap();
    h.handle(350.0, Event::TimerFired { timer });
    h.collect();
    h.reply("order-detail", r#"{"code":"0","data":{"instId":"BTC-USDT","orderId":"12","state":"canceled","filledSize":"4"}}"#);
    let r = h.take("batch-orders");
    let p: serde_json::Value = serde_json::from_str(r.body.as_ref().unwrap()).unwrap();
    assert_eq!(p[0]["size"], "6");
    assert_eq!(p[0]["side"], "buy");
    assert_eq!(p[0]["orderType"], "limit");
    assert_eq!(p[0]["price"], "60000");
    assert_eq!(p[0]["tpTriggerPrice"], "66000");
    assert_eq!(p[0]["attachAlgoOrders"][0]["slTriggerPrice"], "55000");
}

#[test]
fn terminal_rows_cannot_be_amended_or_resurrect_protection() {
    let mut h = Harness::new();
    h.seed();
    seed_order(&mut h, "12", "");
    h.ws(r#"{"arg":{"channel":"orders"},"data":[{"instId":"BTC-USDT","orderId":"12","orderCategory":"normal","orderType":"limit","side":"buy","size":"10","price":"60000","state":"canceled"}]}"#);
    h.handle(
        1.0,
        Event::Command(Command::AmendOrder {
            command: 7,
            id: "12".into(),
            price: Some(61000.0),
            amount: None,
        }),
    );
    h.collect();
    assert!(done(&h));
    assert!(h.pending.is_empty());
    let pair = |state| {
        format!(
            r#"{{"arg":{{"channel":"orders-algo"}},"data":[{{"instId":"BTC-USDT","tpslId":"91","side":"sell","size":"-1","marginMode":"cross","positionSide":"net","tpTriggerPrice":"65000","tpOrderPrice":"-1","slTriggerPrice":"55000","slOrderPrice":"-1","state":"{state}"}}]}}"#
        )
    };
    h.ws(&pair("live"));
    h.ws(&pair("canceled"));
    h.handle(
        2.0,
        Event::Command(Command::CancelOrders {
            command: 8,
            ids: vec!["91__take_profit".into()],
        }),
    );
    h.collect();
    assert!(h.pending.is_empty());
    place(
        &mut h,
        r#"[{"symbol":"BTCUSDT","type":"stop_market","side":"sell","amount":0,"price":54000}]"#,
    );
    let request = h.take("order-tpsl");
    let payload: serde_json::Value = serde_json::from_str(request.body.as_ref().unwrap()).unwrap();
    assert!(payload.get("tpTriggerPrice").is_none());
    assert_eq!(payload["slTriggerPrice"], "54000");
    assert!(h.pending.is_empty());
}

fn amend_normal(h: &mut Harness) {
    h.handle(
        1.0,
        Event::Command(Command::AmendOrder {
            command: 7,
            id: "12".into(),
            price: Some(61000.0),
            amount: None,
        }),
    );
    h.collect();
}
fn resume_cancel(h: &mut Harness) {
    let timer = h
        .actions
        .iter()
        .rev()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == 350.0 => Some(*timer),
            _ => None,
        })
        .unwrap();
    h.handle(350.0, Event::TimerFired { timer });
    h.collect();
}
#[test]
fn already_gone_amend_aborts_without_a_replacement() {
    let mut h = Harness::new();
    h.seed();
    seed_order(&mut h, "12", "");
    amend_normal(&mut h);
    h.reply("cancel-batch-orders", r#"{"code":"0","data":[{"orderId":"12","code":"1000","msg":"filled, triggered, canceled or does not exist"}]}"#);
    assert!(done(&h));
    only_reconcile_pending(&h);
}
#[test]
fn replacement_requires_exact_canceled_detail_with_unchanged_fill_quantity() {
    for detail in [
        r#"{"code":"0","data":{"instId":"BTC-USDT","orderId":"12","state":"canceled","filledSize":"1"}}"#,
        r#"{"code":"0","data":{"instId":"BTC-USDT","orderId":"12","state":"filled","filledSize":"10"}}"#,
        r#"{"code":"0","data":{"instId":"BTC-USDT","orderId":"other","state":"canceled","filledSize":"0"}}"#,
        r#"{"code":"0","data":{"instId":"ETH-USDT","orderId":"12","state":"canceled","filledSize":"0"}}"#,
        r#"{"code":"0","data":{"instId":"BTC-USDT","orderId":"12","state":"live","filledSize":"0"}}"#,
        r#"{"code":"0","data":{"instId":"BTC-USDT","orderId":"12","state":"canceled","filledSize":"bad"}}"#,
        r#"{"code":"0","data":{"instId":"BTC-USDT","orderId":"12","state":"canceled"}}"#,
        r#"{"code":"0","data":[]}"#,
        r#"{"code":"500","data":{"instId":"BTC-USDT","orderId":"12","state":"canceled","filledSize":"0"}}"#,
    ] {
        let mut h = Harness::new();
        h.seed();
        seed_order(&mut h, "12", "");
        amend_normal(&mut h);
        h.reply(
            "cancel-batch-orders",
            r#"{"code":"0","data":[{"orderId":"12","code":"0"}]}"#,
        );
        resume_cancel(&mut h);
        let request = h.take("order-detail");
        assert_eq!(request.method, safe_cex_engine::Method::Get);
        assert!(
            request
                .url
                .ends_with("order-detail?instId=BTC-USDT&orderId=12")
        );
        assert!(request.body.is_none());
        assert!(
            request
                .headers
                .iter()
                .any(|(name, _)| *name == "ACCESS-SIGN")
        );
        h.respond(request, detail);
        assert!(done(&h), "accepted {detail}");
        assert!(h.pending.is_empty());
    }
}
#[test]
fn stable_final_cancel_detail_allows_only_the_remaining_replacement() {
    let mut h = Harness::new();
    h.seed();
    seed_order(&mut h, "12", r#", "filledSize":"4""#);
    amend_normal(&mut h);
    h.reply(
        "cancel-batch-orders",
        r#"{"code":"0","data":[{"orderId":"12","code":"0"}]}"#,
    );
    resume_cancel(&mut h);
    h.reply("order-detail", r#"{"code":"0","data":{"instId":"BTC-USDT","orderId":"12","state":"canceled","filledSize":"4"}}"#);
    let r = h.take("batch-orders");
    let p: serde_json::Value = serde_json::from_str(r.body.as_ref().unwrap()).unwrap();
    assert_eq!(p[0]["size"], "6");
    h.respond(r, r#"{"code":"0","data":[{"orderId":"13","code":"0"}]}"#);
    assert!(!done(&h));
}
#[test]
fn filled_stream_during_cancel_pause_aborts_before_the_detail_or_replacement() {
    let mut h = Harness::new();
    h.seed();
    seed_order(&mut h, "12", "");
    amend_normal(&mut h);
    h.reply(
        "cancel-batch-orders",
        r#"{"code":"0","data":[{"orderId":"12","code":"0"}]}"#,
    );
    h.ws(r#"{"arg":{"channel":"orders"},"data":[{"instId":"BTC-USDT","orderCategory":"normal","orderId":"12","orderType":"limit","side":"buy","size":"10","filledSize":"10","averagePrice":"60000","state":"filled"}]}"#);
    resume_cancel(&mut h);
    assert!(done(&h));
    assert!(h.pending.is_empty());
}
#[test]
fn failed_final_detail_read_is_not_retried_and_never_places() {
    let mut h = Harness::new();
    h.seed();
    seed_order(&mut h, "12", "");
    amend_normal(&mut h);
    h.reply(
        "cancel-batch-orders",
        r#"{"code":"0","data":[{"orderId":"12","code":"0"}]}"#,
    );
    resume_cancel(&mut h);
    let request = h.take("order-detail");
    h.handle(
        350.0,
        Event::HttpFailed {
            request: request.id,
            error: "read unavailable",
        },
    );
    h.collect();
    assert!(done(&h));
    assert!(h.pending.is_empty());
}

#[test]
fn normal_batches_partition_instrument_and_margin_without_reordering_ids() {
    use safe_cex_engine::{SocketId, TimerId};
    use safe_cex_venue_blofin::Options;
    let mut e = trusted::trusted_engine(Options {
        key: Some("key".into()),
        secret: Some("secret".into()),
        passphrase: Some("pass".into()),
        ..Options::default()
    });
    e.start(0.0);
    let mut h = Harness {
        e,
        pending: vec![],
        actions: vec![],
        failures: vec![],
        private: SocketId(0),
        tick: TimerId(0),
        now: 0.0,
        start: 0.0,
    };
    h.collect_at(0.);
    let mut instruments: serde_json::Value = serde_json::from_str(INSTRUMENT).unwrap();
    let mut eth = instruments["data"][0].clone();
    eth["instId"] = "ETH-USDT".into();
    eth["baseCurrency"] = "ETH".into();
    instruments["data"].as_array_mut().unwrap().push(eth);
    h.reply("instruments", &instruments.to_string());
    h.reply("tickers", r#"{"code":"0","data":[]}"#);
    h.modes();
    h.reply(
        "account/positions",
        &format!(
            r#"{{"code":"0","data":[{POSITION},{}]}}"#,
            POSITION
                .replace("BTC-USDT", "ETH-USDT")
                .replace("\"cross\"", "\"isolated\"")
        ),
    );
    h.reply(
        "balances",
        r#"{"code":"0","data":[{"available":"1000","frozen":"0"}]}"#,
    );
    for path in [
        "orders-pending",
        "orders-tpsl-pending",
        "orders-algo-pending",
    ] {
        h.reply(path, r#"{"code":"0","data":[]}"#);
    }
    place(
        &mut h,
        r#"[
        {"symbol":"BTCUSDT","type":"limit","side":"buy","amount":1,"price":60000,"clientOrderId":"first"},
        {"symbol":"ETHUSDT","type":"limit","side":"buy","amount":1,"price":3000,"clientOrderId":"second"},
        {"symbol":"ETHUSDT","type":"limit","side":"buy","amount":1,"price":2900,"clientOrderId":"third"},
        {"symbol":"BTCUSDT","type":"limit","side":"buy","amount":1,"price":59000,"clientOrderId":"fourth"}
    ]"#,
    );
    for (index, (inst, margin, clients, ids)) in [
        ("BTC-USDT", "cross", vec!["first"], vec!["1"]),
        (
            "ETH-USDT",
            "isolated",
            vec!["second", "third"],
            vec!["2", "3"],
        ),
        ("BTC-USDT", "cross", vec!["fourth"], vec!["4"]),
    ]
    .into_iter()
    .enumerate()
    {
        if index > 0 {
            resume_cancel(&mut h);
        }
        let request = h.take("batch-orders");
        let rows: Vec<serde_json::Value> =
            serde_json::from_str(request.body.as_ref().unwrap()).unwrap();
        assert_eq!(rows.len(), clients.len());
        for (row, client) in rows.iter().zip(clients) {
            assert_eq!(row["instId"], inst);
            assert_eq!(row["marginMode"], margin);
            assert_eq!(row["clientOrderId"], client);
        }
        let data: Vec<_> = ids
            .iter()
            .map(|id| serde_json::json!({"orderId":id,"code":"0"}))
            .collect();
        h.respond(
            request,
            &serde_json::json!({"code":"0","data":data}).to_string(),
        );
    }
    assert!(h.actions.iter().any(|a| matches!(a,
        Action::Notice(Notice::CommandDone { command:7, result })
            if result.as_str() == r#"["1","2","3","4"]"#)));
}

#[test]
fn fill_arriving_during_final_detail_read_blocks_a_stale_detail_replacement() {
    let mut h = Harness::new();
    h.seed();
    seed_order(&mut h, "12", "");
    amend_normal(&mut h);
    h.reply(
        "cancel-batch-orders",
        r#"{"code":"0","data":[{"orderId":"12","code":"0"}]}"#,
    );
    resume_cancel(&mut h);
    h.ws(r#"{"arg":{"channel":"orders"},"data":[{"instId":"BTC-USDT","orderCategory":"normal","orderId":"12","orderType":"limit","side":"buy","size":"10","filledSize":"2","averagePrice":"60000","state":"partially_filled"}]}"#);
    h.reply("order-detail", r#"{"code":"0","data":{"instId":"BTC-USDT","orderId":"12","state":"canceled","filledSize":"0"}}"#);
    assert!(done(&h));
    assert!(h.pending.is_empty());
}

#[test]
fn rest_cancel_tombstone_survives_snapshot_then_delayed_live_replay() {
    for reply in [
        r#"{"code":"0","data":[{"orderId":"12","code":"0"}]}"#,
        r#"{"code":"1","data":[{"orderId":"12","code":"1000"}]}"#,
    ] {
        let mut h = Harness::new();
        h.seed();
        seed_order(&mut h, "12", "");
        h.handle(
            1.0,
            Event::Command(Command::CancelOrders {
                command: 7,
                ids: vec!["12".into()],
            }),
        );
        h.collect();
        h.reply("cancel-batch-orders", reply);
        assert!(!done(&h));
        h.handle(10_000.0, Event::TimerFired { timer: h.tick });
        h.collect();
        h.reply("account/positions", r#"{"code":"0","data":[]}"#);
        h.reply("balances", r#"{"code":"0","data":[]}"#);
        for path in [
            "orders-pending",
            "orders-tpsl-pending",
            "orders-algo-pending",
        ] {
            h.reply(path, r#"{"code":"0","data":[]}"#);
        }
        assert!(h.e.store().order("12").is_none());
        seed_order(&mut h, "12", "");
        assert!(h.e.store().order("12").is_none());
    }
}

#[test]
fn successful_cancel_ack_cannot_downgrade_an_intervening_filled_status() {
    use safe_cex_engine::model::OrderStatus;
    let mut h = Harness::new();
    h.seed();
    seed_order(&mut h, "12", "");
    h.handle(
        1.0,
        Event::Command(Command::CancelOrders {
            command: 7,
            ids: vec!["12".into()],
        }),
    );
    h.collect();
    h.ws(r#"{"arg":{"channel":"orders"},"data":[{"instId":"BTC-USDT","orderCategory":"normal","orderId":"12","orderType":"limit","side":"buy","size":"10","filledSize":"10","averagePrice":"60000","state":"filled"}]}"#);
    h.reply(
        "cancel-batch-orders",
        r#"{"code":"0","data":[{"orderId":"12","code":"0"}]}"#,
    );
    assert_eq!(h.e.store().order("12").unwrap().status, OrderStatus::Closed);
}

#[test]
fn cancel_symbol_includes_ack_before_private_or_rest_order_arrives() {
    let mut h = Harness::new();
    h.seed();
    place(
        &mut h,
        r#"[{"symbol":"BTCUSDT","type":"limit","side":"buy","amount":1,"price":60000,"clientOrderId":"Pending1"}]"#,
    );
    h.reply(
        "batch-orders",
        r#"{"code":"0","data":[{"orderId":"ack-only","code":"0","msg":"success"}]}"#,
    );
    assert!(h.e.store().orders().is_empty());
    h.handle(
        2.,
        Event::Command(Command::CancelSymbolOrders {
            command: 8,
            symbol: "BTCUSDT".into(),
        }),
    );
    h.collect();
    let cancel = h.take("cancel-batch-orders");
    let payload: serde_json::Value = serde_json::from_str(cancel.body.as_deref().unwrap()).unwrap();
    assert_eq!(payload[0]["orderId"], "ack-only");
    assert_eq!(payload[0]["instId"], "BTC-USDT");
    h.respond(
        cancel,
        r#"{"code":"0","data":[{"orderId":"ack-only","code":"0"}]}"#,
    );
    assert!(h.actions.iter().any(|a| matches!(
        a,
        Action::Notice(Notice::CommandDone {
            command: 8,
            result: _
        })
    )));
}

#[test]
fn retained_protective_and_trigger_orders_keep_their_cancel_family() {
    for (kind, trigger, place_path, cancel_path, id_field) in [
        ("stop_market", "", "order-tpsl", "cancel-tpsl", "tpslId"),
        (
            "conditional",
            ",\"triggerPrice\":61000",
            "order-algo",
            "cancel-algo",
            "algoId",
        ),
    ] {
        for bad_ack in [
            r#"{"code":"0","data":[]}"#,
            r#"{"code":"0","data":[{"orderId":"retained","code":"0"}]}"#,
        ] {
            let mut h = Harness::new();
            h.seed();
            place(
                &mut h,
                &format!(
                    r#"[{{"symbol":"BTCUSDT","type":"{kind}","side":"sell","amount":1,"price":60000{trigger}}}]"#
                ),
            );
            let placed = h.take(place_path);
            h.respond(
                placed,
                &format!(r#"{{"code":"0","data":{{"{id_field}":"retained","code":"0"}}}}"#),
            );
            assert!(h.e.store().orders().is_empty());
            h.handle(
                2.,
                Event::Command(Command::CancelSymbolOrders {
                    command: 8,
                    symbol: "BTCUSDT".into(),
                }),
            );
            h.collect();
            let cancel = h.take(cancel_path);
            let payload: serde_json::Value =
                serde_json::from_str(cancel.body.as_deref().unwrap()).unwrap();
            let row = if kind == "conditional" {
                &payload
            } else {
                &payload[0]
            };
            assert_eq!(row[id_field], "retained");
            assert_eq!(row["instId"], "BTC-USDT");
            h.respond(cancel, bad_ack);
            assert!(
                h.actions
                    .iter()
                    .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 8, .. })))
            );
            h.handle(
                3.,
                Event::Command(Command::CancelSymbolOrders {
                    command: 9,
                    symbol: "BTCUSDT".into(),
                }),
            );
            h.collect();
            let cancel = h.take(cancel_path);
            h.respond(
                cancel,
                &format!(r#"{{"code":"0","data":[{{"{id_field}":"retained","code":"0"}}]}}"#),
            );
            assert!(h.actions.iter().any(|a| matches!(
                a,
                Action::Notice(Notice::CommandDone {
                    command: 9,
                    result: _
                })
            )));
        }
    }
}

#[test]
fn ambiguous_placement_ack_cannot_authorize_empty_cancel_all_success() {
    let mut h = Harness::new();
    h.seed();
    place(
        &mut h,
        r#"[{"symbol":"BTCUSDT","type":"limit","side":"buy","amount":1,"price":60000}]"#,
    );
    h.reply("batch-orders", r#"{"code":"0","data":[]}"#);
    assert!(done(&h));
    h.handle(
        2.,
        Event::Command(Command::CancelSymbolOrders {
            command: 8,
            symbol: "BTCUSDT".into(),
        }),
    );
    h.collect();
    assert!(
        h.actions
            .iter()
            .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 8, .. }))),
        "successful empty ACK does not prove placement absence"
    );
}

#[test]
fn cancellation_missing_from_store_errors_before_any_write_even_in_a_mixed_batch() {
    use safe_cex_engine::model::{Order, OrderSide, OrderStatus, OrderType};
    use safe_cex_engine::{Action, Notice};
    for mixed in [false, true] {
        let mut h = Harness::new();
        h.seed();
        h.e.store_mut().set_orders(vec![Order {
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
        h.e.drain_actions().for_each(drop);
        h.handle(
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
        let actions = h.e.drain_actions().collect::<Vec<_>>();
        assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
        assert!(
            actions.iter().any(|a| matches!(
                a,
                Action::Notice(Notice::CommandFailed { command: 991, error })
                    if error.message.contains("456") && error.message.contains("not found")
                        && error.outcome == Some(safe_cex_engine::Outcome::NotSent)
            )),
            "missing cancellation error: {actions:?}"
        );
        assert_eq!(h.e.store().order("123").unwrap().status, OrderStatus::Open);
    }
}

#[test]
fn broker_payload_gate_covers_normal_trigger_and_protective_orders_without_rewriting_ids() {
    for attributed in [false, true] {
        for (raw, endpoint) in [
            (
                r#"[{"symbol":"BTCUSDT","type":"limit","side":"buy","amount":1,"price":50000,"clientOrderId":"Caller123"}]"#,
                "batch-orders",
            ),
            (
                r#"[{"symbol":"BTCUSDT","type":"conditional","side":"buy","amount":1,"price":50000,"triggerPrice":51000}]"#,
                "order-algo",
            ),
            (
                r#"[{"symbol":"BTCUSDT","type":"stop_market","side":"sell","amount":1,"price":40000}]"#,
                "order-tpsl",
            ),
        ] {
            let mut h = Harness::with_broker_attribution(attributed);
            h.seed();
            place(&mut h, raw);
            let request = h.take(endpoint);
            let body: serde_json::Value =
                serde_json::from_str(request.body.as_ref().unwrap()).unwrap();
            let row = body.as_array().map_or(&body, |rows| &rows[0]);
            if attributed {
                assert_eq!(row["brokerId"], "b3cdedc0a20880ba");
            } else {
                assert!(row.get("brokerId").is_none());
            }
            let client = row["clientOrderId"].as_str().unwrap();
            if raw.contains("Caller123") {
                assert_eq!(client, "Caller123");
            } else {
                assert!(client.starts_with("b3cdedc0a20880ba"));
                assert_eq!(client.len(), 32);
            }
        }
    }
}

#[test]
fn mixed_unknown_cancel_keeps_ack_route_and_sends_no_partial_write() {
    for cached_first in [false, true] {
        let mut h = Harness::new();
        h.seed();
        place(
            &mut h,
            r#"[{"symbol":"BTCUSDT","type":"limit","side":"buy","amount":1,"price":60000,"clientOrderId":"Pending1"}]"#,
        );
        h.reply(
            "batch-orders",
            r#"{"code":"0","data":[{"orderId":"ack-only","code":"0","msg":"success"}]}"#,
        );
        assert!(h.e.store().orders().is_empty());
        h.handle(
            2.,
            Event::Command(Command::CancelOrders {
                command: 991,
                ids: if cached_first {
                    vec!["ack-only".into(), "missing".into()]
                } else {
                    vec!["missing".into(), "ack-only".into()]
                },
            }),
        );
        h.collect();
        assert!(
            h.pending
                .iter()
                .all(|request| request.method == safe_cex_engine::Method::Get)
        );
        assert!(h.actions.iter().any(|action| matches!(
            action,
            Action::Notice(Notice::CommandFailed { command: 991, error })
                if error.legacy_text().contains("missing") && error.legacy_text().contains("not found")
        )));
        h.handle(
            3.,
            Event::Command(Command::CancelOrders {
                command: 992,
                ids: vec!["ack-only".into()],
            }),
        );
        h.collect();
        let cancel = h.take("cancel-batch-orders");
        let payload: serde_json::Value =
            serde_json::from_str(cancel.body.as_deref().unwrap()).unwrap();
        assert_eq!(payload[0]["orderId"], "ack-only");
        assert_eq!(payload[0]["instId"], "BTC-USDT");
        h.respond(
            cancel,
            r#"{"code":"0","data":[{"orderId":"ack-only","code":"0"}]}"#,
        );
        assert!(h.actions.iter().any(|action| matches!(
            action,
            Action::Notice(Notice::CommandDone {
                command: 992,
                result: _
            })
        )));
    }
}

#[test]
fn invalid_dynamic_leverage_limits_reject_without_native_write() {
    for (min, max) in [
        (30.0, 20.0),
        (f64::NAN, 20.0),
        (1.0, 0.0),
        (1.0, f64::INFINITY),
    ] {
        let mut h = Harness::new();
        h.seed();
        h.pending.clear();
        h.actions.clear();
        h.e.store_mut().update_markets(|market| {
            market.limits.leverage.min = min;
            market.limits.leverage.max = max;
        });
        h.handle(
            1.0,
            Event::Command(Command::SetLeverage {
                command: 99,
                symbol: "BTCUSDT".into(),
                leverage: 5.0,
                side: None,
            }),
        );
        h.collect();
        assert!(h.pending.is_empty());
        assert!(h.actions.iter().any(|action| matches!(action, Action::Notice(Notice::CommandFailed { command: 99, error }) if error.legacy_text().contains("leverage limits"))));
    }
}

#[test]
fn closed_position_restores_account_margin_default_before_next_order() {
    use safe_cex_engine::model::MarginType;
    // Empty snapshots and explicit zero rows are both valid closure signals.
    for rest in [false, true] {
        for explicit_zero in [false, true] {
            let mut h = Harness::new();
            h.seed();
            h.handle(
                1.0,
                Event::Command(Command::SetMarginType {
                    command: 8,
                    symbol: "BTCUSDT".into(),
                    isolated: true,
                }),
            );
            h.collect();
            h.reply("set-margin-mode", r#"{"code":"0","data":{}}"#);
            assert_eq!(
                h.e.store().positions()[0].margin_type,
                Some(MarginType::Cross)
            );
            let data = if explicit_zero {
                format!(
                    "[{}]",
                    POSITION.replace("\"positions\":\"4\"", "\"positions\":\"0\"")
                )
            } else {
                "[]".into()
            };
            if rest {
                h.reply(
                    "account/positions",
                    &format!(r#"{{"code":"0","data":{data}}}"#),
                );
                h.reply(
                    "balances",
                    r#"{"code":"0","data":[{"available":"1000","frozen":"0"}]}"#,
                );
            } else {
                h.ws(&format!(
                    r#"{{"arg":{{"channel":"positions"}},"data":{data}}}"#
                ));
            }
            assert!(h.e.store().positions().is_empty());
            let setting = h.e.store().setting("BTCUSDT").unwrap();
            assert_eq!(
                setting.margin_type,
                Some(MarginType::Isolated),
                "rest={rest}, zero={explicit_zero}"
            );
            assert_eq!(setting.leverage, 20.0);
            place(
                &mut h,
                r#"[{"symbol":"BTCUSDT","type":"limit","side":"buy","amount":1,"price":60000}]"#,
            );
            let request = h.take("batch-orders");
            let payload: serde_json::Value =
                serde_json::from_str(request.body.as_ref().unwrap()).unwrap();
            assert_eq!(payload[0]["marginMode"], "isolated");
        }
    }
}

#[test]
fn trading_failures_are_classified_without_changing_their_text() {
    use safe_cex_engine::{ErrorKind, Outcome, Response};
    let market = r#"[{"symbol":"BTCUSDT","type":"market","side":"buy","amount":1}]"#;
    let mut h = Harness::new();
    h.seed();
    place(&mut h, market);
    let r = h.take("batch-orders");
    h.handle(
        2.0,
        Event::HttpFailed {
            request: r.id,
            error: "response lost",
        },
    );
    h.collect();
    assert_eq!(h.failures[0].outcome, Some(Outcome::Unknown));
    assert_eq!(h.failures[0].kind, Some(ErrorKind::Network));

    // D-a: a documented row code is a typed refusal; a 5xx or an unproven
    // acknowledgement carries the shared projection's kind. Text is unchanged.
    for (status, body, kind, outcome) in [
        (
            502,
            "Bad Gateway",
            Some(ErrorKind::VenueUnavailable),
            Some(Outcome::Unknown),
        ),
        (
            429,
            "{}",
            Some(ErrorKind::RateLimited),
            Some(Outcome::Rejected),
        ),
        (
            200,
            r#"{"code":"1","msg":"","data":[{"orderId":"","code":"102002","msg":"Insufficient balance"}]}"#,
            Some(ErrorKind::Rejected),
            Some(Outcome::Rejected),
        ),
        (
            200,
            r#"{"code":"0","msg":"","data":[{"orderId":"9","code":"0","msg":"Order placed successfully"}]}"#,
            Some(ErrorKind::Internal),
            Some(Outcome::Unknown),
        ),
        (
            200,
            r#"{"code":"0","msg":"","data":[]}"#,
            Some(ErrorKind::Internal),
            Some(Outcome::Unknown),
        ),
    ] {
        let mut h = Harness::new();
        h.seed();
        place(&mut h, market);
        let r = h.take("batch-orders");
        h.handle(
            2.0,
            Event::HttpResponse {
                request: r.id,
                response: Response {
                    status,
                    headers: "",
                    body,
                },
            },
        );
        h.collect();
        let [error] = h.failures.as_slice() else {
            panic!("{status} {body}: {:?}", h.failures);
        };
        assert_eq!(
            (error.kind, error.outcome),
            (kind, outcome),
            "{status} {body}: {}",
            error.message
        );
    }
}

#[test]
fn attached_take_profit_and_stop_loss_are_rounded_to_the_price_tick() {
    let mut h = Harness::new();
    h.seed();
    place(
        &mut h,
        r#"[{"symbol":"BTCUSDT","type":"limit","side":"buy","amount":1,"price":60000,"takeProfit":65000.04,"stopLoss":55000.06}]"#,
    );
    let r = h.take("batch-orders");
    let p: serde_json::Value = serde_json::from_str(r.body.as_ref().unwrap()).unwrap();
    let attached = &p[0]["attachAlgoOrders"][0];
    assert_eq!(attached["tpTriggerPrice"], "65000");
    assert_eq!(attached["slTriggerPrice"], "55000.1");
}

#[test]
fn attached_trigger_that_rounds_to_zero_is_refused_before_any_write() {
    let mut h = Harness::new();
    h.seed();
    place(
        &mut h,
        r#"[{"symbol":"BTCUSDT","type":"limit","side":"buy","amount":1,"price":60000,"stopLoss":0.04}]"#,
    );
    assert!(h.pending.is_empty());
    assert!(done(&h));
}

#[test]
fn multi_position_mode_refusal_explains_how_to_continue() {
    for reply in [
        r#"{"code":"102143","msg":"position selection required","data":[]}"#,
        r#"{"code":"0","data":[{"orderId":"","code":"102143","msg":"position selection required"}]}"#,
    ] {
        let mut h = Harness::new();
        h.seed();
        place(
            &mut h,
            r#"[{"symbol":"BTCUSDT","type":"limit","side":"buy","amount":1,"price":60000}]"#,
        );
        let r = h.take("batch-orders");
        h.respond(r, reply);
        let error = h.failures.last().expect("placement refused");
        assert!(
            error
                .message
                .contains("Turn off Multi-Position mode on BloFin")
        );
        assert_ne!(error.outcome, Some(safe_cex_engine::Outcome::Unknown));
    }
}

#[test]
fn margin_type_change_warns_while_orders_are_open() {
    for open in [false, true] {
        let mut h = Harness::new();
        h.modes();
        h.reply("account/positions", r#"{"code":"0","data":[]}"#);
        h.reply(
            "balances",
            r#"{"code":"0","data":[{"currency":"USDT","available":"1000","frozen":"0"}]}"#,
        );
        for path in [
            "orders-pending",
            "orders-tpsl-pending",
            "orders-algo-pending",
        ] {
            h.reply(path, r#"{"code":"0","data":[]}"#);
        }
        if open {
            seed_order(&mut h, "1", "");
        }
        h.handle(
            1.0,
            Event::Command(Command::SetMarginType {
                command: 7,
                symbol: "BTCUSDT".into(),
                isolated: true,
            }),
        );
        h.collect();
        assert_eq!(margin_warned(&h), open);
        assert!(h.pending.iter().any(|r| r.url.contains("set-margin-mode")));
    }
}

#[test]
fn margin_type_change_warns_while_a_position_is_open() {
    let mut h = Harness::new();
    h.seed();
    assert!(h.e.store().positions().iter().any(|p| p.contracts > 0.0));
    h.handle(
        1.0,
        Event::Command(Command::SetMarginType {
            command: 7,
            symbol: "BTCUSDT".into(),
            isolated: true,
        }),
    );
    h.collect();
    assert!(margin_warned(&h));
}

fn margin_warned(h: &Harness) -> bool {
    h.actions.iter().any(|a| {
        matches!(
            a,
            Action::Notice(Notice::Event(event))
                if event.level == safe_cex_engine::Level::Error
                    && event.message.starts_with("Changing margin type with open positions / orders")
        )
    })
}

#[test]
fn multi_position_mode_refusal_of_a_protective_order_is_explained() {
    let mut h = Harness::new();
    h.seed();
    place(
        &mut h,
        r#"[{"symbol":"BTCUSDT","type":"stop_market","side":"sell","amount":0,"price":55000}]"#,
    );
    let r = h.take("order-tpsl");
    h.respond(
        r,
        r#"{"code":"102143","msg":"position selection required","data":null}"#,
    );
    let error = h.failures.last().expect("placement refused");
    assert!(
        error
            .message
            .contains("Turn off Multi-Position mode on BloFin")
    );
}
