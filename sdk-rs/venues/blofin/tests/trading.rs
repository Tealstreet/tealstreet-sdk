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
fn editing_inline_virtual_protection_recreates_the_entry_and_both_brackets() {
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
    let r = h.take("batch-orders");
    let p: serde_json::Value = serde_json::from_str(r.body.as_ref().unwrap()).unwrap();
    assert_eq!(p[0]["size"], "6");
    assert_eq!(p[0]["side"], "buy");
    assert_eq!(p[0]["orderType"], "limit");
    assert_eq!(p[0]["price"], "60000");
    assert_eq!(p[0]["tpTriggerPrice"], "66000");
    assert_eq!(p[0]["attachAlgoOrders"][0]["slTriggerPrice"], "55000");
}
