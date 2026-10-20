mod support;
use safe_cex_engine::model::PlaceOrderOpts;
use safe_cex_engine::{Action, Command, Event, Notice};
use support::*;
fn place(h: &mut Harness, raw: &str) {
    let orders: Vec<PlaceOrderOpts> = serde_json::from_str(raw).unwrap();
    h.e.handle(
        1.0,
        Event::Command(Command::PlaceOrders { command: 7, orders }),
    );
    h.collect();
}
fn done(h: &Harness) -> bool {
    h.actions.iter().any(|a| {
        matches!(
            a,
            Action::Notice(Notice::CommandDone {
                command: 7,
                result: Err(_)
            })
        )
    })
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
            Action::Notice(Notice::CommandDone {
                command: 7,
                result: Err(error)
            }) if error.contains("Bybit")
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
    h.e.handle(
        2.0,
        Event::HttpFailed {
            request: r.id,
            error: "response lost after acceptance",
        },
    );
    h.collect();
    assert!(done(&h));
    assert!(h.pending.is_empty());
    assert!(h.e.store().orders().is_empty());
}
#[test]
fn successful_envelope_does_not_hide_mismatched_or_failed_cancel_rows() {
    let mut h = Harness::new();
    h.seed();
    seed_order(&mut h, "12", "");
    seed_order(&mut h, "123", "");
    h.e.handle(
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
    h.e.handle(
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
    h.e.handle(
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
    h.e.handle(
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
    assert!(h.pending.is_empty());
}
#[test]
fn conditional_amend_preserves_nested_protection_and_native_execution_provenance() {
    let mut h = Harness::new();
    h.seed();
    h.ws(r#"{"arg":{"channel":"orders-algo"},"data":[{"instId":"BTC-USDT","algoId":"77","orderType":"trigger","side":"buy","size":"3","triggerPrice":"61000","orderPrice":"60900","triggerPriceType":"mark","marginMode":"isolated","positionSide":"long","state":"live","attachAlgoOrders":[{"tpTriggerPrice":"65000","tpOrderPrice":"-1","tpTriggerPriceType":"index","slTriggerPrice":"55000","slOrderPrice":"-1","slTriggerPriceType":"mark"}]}]}"#);
    h.e.handle(
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
    h.e.handle(350.0, Event::TimerFired { timer });
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
    h.e.handle(
        1.0,
        Event::Command(Command::SetLeverage {
            command: 7,
            symbol: "BTCUSDT".into(),
            leverage: 30.0,
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
    h.e.handle(
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
    h.e.handle(1000.0, Event::TimerFired { timer });
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
    h.e.handle(
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
    h.e.handle(1000.0, Event::TimerFired { timer });
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
    h.e.handle(
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
    h.e.handle(350.0, Event::TimerFired { timer });
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
    h.e.handle(
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
    h.e.handle(
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
    h.e.handle(
        1.0,
        Event::Command(Command::SetPositionMode {
            command: 9,
            hedged: true,
        }),
    );
    h.collect();
    h.reply(
        "set-position-mode",
        r#"{"code":"0","data":{"positionMode":"long_short_mode"}}"#,
    );
    h.e.handle(
        2.0,
        Event::Command(Command::SetLeverage {
            command: 7,
            symbol: "BTCUSDT".into(),
            leverage: 30.0,
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
    h.e.handle(
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
    h.e.handle(350.0, Event::TimerFired { timer });
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
    h.e.handle(
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
    h.e.handle(
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
    h.e.handle(
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
    h.e.handle(350.0, Event::TimerFired { timer });
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
    assert!(h.pending.is_empty());
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
    h.e.handle(
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
    use safe_cex_engine::{Engine, SocketId, TimerId};
    use safe_cex_venue_blofin::{Blofin, Options};
    let mut e = Engine::new(Blofin::new(Options {
        key: Some("key".into()),
        secret: Some("secret".into()),
        passphrase: Some("pass".into()),
        ..Options::default()
    }));
    e.start(0.0);
    let mut h = Harness {
        e,
        pending: vec![],
        actions: vec![],
        private: SocketId(0),
        tick: TimerId(0),
    };
    h.collect();
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
        Action::Notice(Notice::CommandDone { command:7, result:Ok(result) })
            if result == r#"["1","2","3","4"]"#)));
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
        h.e.handle(
            1.0,
            Event::Command(Command::CancelOrders {
                command: 7,
                ids: vec!["12".into()],
            }),
        );
        h.collect();
        h.reply("cancel-batch-orders", reply);
        assert!(!done(&h));
        h.e.handle(10_000.0, Event::TimerFired { timer: h.tick });
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
    h.e.handle(
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
    h.e.handle(
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
            result: Ok(_)
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
            h.e.handle(
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
            assert!(h.actions.iter().any(|a| matches!(
                a,
                Action::Notice(Notice::CommandDone {
                    command: 8,
                    result: Err(_)
                })
            )));
            h.e.handle(
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
                    result: Ok(_)
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
    h.e.handle(
        2.,
        Event::Command(Command::CancelSymbolOrders {
            command: 8,
            symbol: "BTCUSDT".into(),
        }),
    );
    h.collect();
    assert!(
        h.actions.iter().any(|a| matches!(
            a,
            Action::Notice(Notice::CommandDone {
                command: 8,
                result: Err(_)
            })
        )),
        "successful empty ACK does not prove placement absence"
    );
}
