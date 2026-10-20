#![allow(dead_code)]
mod support;
use safe_cex_engine::model::PlaceOrderOpts;
use safe_cex_engine::{Action, Command, Event, Notice};
use support::*;
fn order(text: &str) -> PlaceOrderOpts {
    serde_json::from_str(text).unwrap()
}
fn place(
    e: &mut safe_cex_engine::Engine<safe_cex_venue_okx::Okx>,
    orders: Vec<PlaceOrderOpts>,
) -> Vec<Action> {
    e.handle(
        5.0,
        Event::Command(Command::PlaceOrders { command: 1, orders }),
    );
    actions(e)
}
#[test]
fn caller_id_and_contract_amount_are_sent_verbatim_after_lot_adjustment() {
    let mut e = trading_engine();
    let a = place(
        &mut e,
        vec![order(
            r#"{"symbol":"BTCUSDT","type":"limit","side":"buy","amount":2.345,"price":38000.04,"clientOrderId":"Caller123"}"#,
        )],
    );
    let request = a.iter().find(|a| matches!(a, Action::Http(_))).unwrap();
    let Action::Http(r) = request else { panic!() };
    let body = r.body.as_deref().unwrap();
    assert!(body.contains("\"sz\":\"2.35\""));
    assert!(body.contains("\"clOrdId\":\"Caller123\""));
    assert!(body.contains("\"px\":\"38000\""));
    assert!(r.headers.iter().any(|(k, _)| *k == "OK-ACCESS-SIGN"));
    reply(
        &mut e,
        request,
        &envelope(r#"{"ordId":"1234","sCode":"0","sMsg":""}"#),
        6.0,
    );
    assert!(actions(&mut e).iter().any(|a| matches!(a, Action::Notice(Notice::CommandDone { result: Ok(s), .. }) if s == "[\"1234\"]")));
}
#[test]
fn whole_batch_rejects_unhonorable_or_duplicate_caller_id_before_writes() {
    for bad in [
        r#"{"symbol":"BTCUSDT","type":"limit","side":"buy","amount":2,"price":38000,"clientOrderId":"bad-id"}"#,
        r#"{"symbol":"BTCUSDT","type":"limit","side":"buy","amount":2,"price":38000,"clientOrderId":"Good123","stopLoss":36000}"#,
        r#"{"symbol":"BTCUSDT","type":"limit","side":"buy","amount":2000,"price":38000,"clientOrderId":"Good123"}"#,
    ] {
        let mut e = trading_engine();
        let out = place(
            &mut e,
            vec![
                order(r#"{"symbol":"BTCUSDT","type":"market","side":"buy","amount":1}"#),
                order(bad),
            ],
        );
        assert!(!out.iter().any(|a| matches!(a, Action::Http(_))));
        assert!(out.iter().any(|a| matches!(
            a,
            Action::Notice(Notice::CommandDone { result: Err(_), .. })
        )));
    }
}
#[test]
fn bracket_entry_then_stop_then_take_profit_are_sequenced() {
    let mut e = trading_engine();
    let out = place(
        &mut e,
        vec![order(
            r#"{"symbol":"BTCUSDT","type":"limit","side":"buy","amount":2,"price":38000,"stopLoss":36000,"takeProfit":42000}"#,
        )],
    );
    let a = out
        .into_iter()
        .find(|a| matches!(a, Action::Http(_)))
        .unwrap();
    assert!(matches!(&a, Action::Http(r) if r.url.ends_with("/batch-orders")));
    reply(
        &mut e,
        &a,
        &envelope(r#"{"ordId":"1","sCode":"0","sMsg":""}"#),
        6.0,
    );
    let a = actions(&mut e)
        .into_iter()
        .find(|a| matches!(a, Action::Http(_)))
        .unwrap();
    assert!(
        matches!(&a, Action::Http(r) if r.body.as_ref().unwrap().contains("slTriggerPx") && r.body.as_ref().unwrap().contains("\"reduceOnly\":true"))
    );
    reply(
        &mut e,
        &a,
        &envelope(r#"{"algoId":"2","sCode":"0","sMsg":""}"#),
        7.0,
    );
    let a = actions(&mut e)
        .into_iter()
        .find(|a| matches!(a, Action::Http(_)))
        .unwrap();
    assert!(matches!(&a, Action::Http(r) if r.body.as_ref().unwrap().contains("tpTriggerPx")));
    reply(
        &mut e,
        &a,
        &envelope(r#"{"algoId":"3","sCode":"0","sMsg":""}"#),
        8.0,
    );
    assert!(actions(&mut e).iter().any(|a| matches!(a, Action::Notice(Notice::CommandDone { result: Ok(s), .. }) if s == "[\"1\",\"2\",\"3\"]")));
}
#[test]
fn lost_write_response_never_retries() {
    let mut e = trading_engine();
    let out = place(
        &mut e,
        vec![order(
            r#"{"symbol":"BTCUSDT","type":"market","side":"buy","amount":2}"#,
        )],
    );
    let request = out
        .iter()
        .find_map(|a| {
            if let Action::Http(r) = a {
                Some(r.id)
            } else {
                None
            }
        })
        .unwrap();
    e.handle(
        6.0,
        Event::HttpFailed {
            request,
            error: "lost response after acceptance",
        },
    );
    let out = actions(&mut e);
    assert!(
        !out.iter()
            .any(|a| matches!(a, Action::Http(_) | Action::SetTimer { .. }))
    );
    assert!(out.iter().any(|a| matches!(
        a,
        Action::Notice(Notice::CommandDone { result: Err(_), .. })
    )));
}

fn stored(id: &str, kind: safe_cex_engine::model::OrderType) -> safe_cex_engine::model::Order {
    use safe_cex_engine::model::*;
    Order {
        id: id.into(),
        parent_id: None,
        status: OrderStatus::Open,
        symbol: "BTCUSDT".into(),
        kind,
        side: OrderSide::Sell,
        price: 36000.0,
        amount: 2.0,
        filled: 0.0,
        remaining: 0.02,
        reduce_only: true,
        close: None,
        client_order_id: None,
        time_in_force: Some(TimeInForce::GoodTillCancel),
        last_update: 0.0,
        trigger_type: Some(StopTriggerType::MarkPrice),
        take_profit: None,
        stop_loss: None,
        extra: None,
    }
}
fn http(out: &[Action]) -> &Action {
    out.iter()
        .find(|a| matches!(a, Action::Http(_)))
        .expect("HTTP action")
}
fn body(action: &Action) -> &str {
    let Action::Http(r) = action else {
        panic!("HTTP action")
    };
    r.body.as_deref().unwrap()
}
fn ack(
    e: &mut safe_cex_engine::Engine<safe_cex_venue_okx::Okx>,
    a: &Action,
    algo: bool,
) -> Vec<Action> {
    reply(
        e,
        a,
        &envelope(if algo {
            r#"{"algoId":"42","sCode":"0","sMsg":""}"#
        } else {
            r#"{"ordId":"41","sCode":"0","sMsg":""}"#
        }),
        6.0,
    );
    actions(e)
}

#[test]
fn negative_protective_amount_never_turns_into_close_all() {
    let mut e = trading_engine();
    let out = place(
        &mut e,
        vec![order(
            r#"{"symbol":"BTCUSDT","type":"stop_market","side":"sell","amount":-0.001,"price":36000}"#,
        )],
    );
    assert!(!out.iter().any(|a| matches!(a, Action::Http(_))));
    assert!(out.iter().any(|a| matches!(
        a,
        Action::Notice(Notice::CommandDone { result: Err(_), .. })
    )));
}

#[test]
fn exact_decimal_max_keeps_one_caller_id_and_split_lots_keep_their_size() {
    let mut e = trading_engine();
    let mut markets = e.store().markets().to_vec();
    markets[0].precision.amount = 0.00001;
    markets[0].limits.amount.max = 1.0;
    e.store_mut().set_markets(markets);
    let out = place(
        &mut e,
        vec![order(
            r#"{"symbol":"BTCUSDT","type":"market","side":"buy","amount":1,"clientOrderId":"ExactMax"}"#,
        )],
    );
    assert!(body(http(&out)).contains("\"sz\":\"1\""));
    assert_eq!(body(http(&out)).matches("ExactMax").count(), 1);
    let mut e = trading_engine();
    let mut markets = e.store().markets().to_vec();
    markets[0].precision.amount = 0.1;
    markets[0].limits.amount.max = 0.3;
    e.store_mut().set_markets(markets);
    let out = place(
        &mut e,
        vec![order(
            r#"{"symbol":"BTCUSDT","type":"market","side":"buy","amount":0.8}"#,
        )],
    );
    let payload = body(http(&out));
    assert_eq!(payload.matches("\"sz\":\"0.3\"").count(), 2);
    assert_eq!(payload.matches("\"sz\":\"0.2\"").count(), 1);
}

#[test]
fn rounded_caller_amount_cannot_split_and_rejects_whole_batch() {
    let mut e = trading_engine();
    let mut markets = e.store().markets().to_vec();
    markets[0].precision.amount = 0.1;
    markets[0].limits.amount.max = 0.26;
    e.store_mut().set_markets(markets);
    let out = place(
        &mut e,
        vec![
            order(r#"{"symbol":"BTCUSDT","type":"market","side":"buy","amount":0.1}"#),
            order(
                r#"{"symbol":"BTCUSDT","type":"market","side":"buy","amount":0.26,"clientOrderId":"MustNotSplit"}"#,
            ),
        ],
    );
    assert!(!out.iter().any(|a| matches!(a, Action::Http(_))));
}

#[test]
fn split_brackets_protect_each_accepted_lot_before_another_entry() {
    let mut e = trading_engine();
    let out = place(
        &mut e,
        vec![order(
            r#"{"symbol":"BTCUSDT","type":"market","side":"buy","amount":2001,"stopLoss":36000,"takeProfit":42000}"#,
        )],
    );
    let a = http(&out);
    assert_eq!(body(a).matches("\"sz\"").count(), 1);
    assert!(body(a).contains("\"sz\":\"1000\""));
    let out = ack(&mut e, a, false);
    let stop = http(&out);
    assert!(body(stop).contains("slTriggerPx"));
    assert!(body(stop).contains("\"sz\":\"1000\""));
    let out = ack(&mut e, stop, true);
    assert!(body(http(&out)).contains("tpTriggerPx"));
    let out = ack(&mut e, http(&out), true);
    let second = http(&out);
    assert!(matches!(second, Action::Http(r) if r.url.ends_with("/batch-orders")));
    reply(
        &mut e,
        second,
        r#"{"code":"1","msg":"All operations failed","data":[{"sCode":"51008","sMsg":"Insufficient margin"}]}"#,
        7.0,
    );
    let out = actions(&mut e);
    assert!(!out.iter().any(|a| matches!(a, Action::Http(_))));
    assert!(out.iter().any(|a| matches!(
        a,
        Action::Notice(Notice::CommandDone { result: Err(_), .. })
    )));
}

#[test]
fn plain_orders_still_share_a_native_batch() {
    let mut e = trading_engine();
    let out = place(
        &mut e,
        vec![
            order(r#"{"symbol":"BTCUSDT","type":"market","side":"buy","amount":1}"#),
            order(r#"{"symbol":"BTCUSDT","type":"market","side":"sell","amount":2}"#),
        ],
    );
    assert_eq!(body(http(&out)).matches("\"sz\"").count(), 2);
}

#[test]
fn market_and_terminal_amends_are_rejected_before_writes() {
    use safe_cex_engine::model::*;
    for (kind, status) in [
        (OrderType::Market, OrderStatus::Open),
        (OrderType::Limit, OrderStatus::Closed),
        (OrderType::StopLoss, OrderStatus::Canceled),
    ] {
        let mut e = trading_engine();
        let mut o = stored("91", kind);
        o.status = status;
        e.store_mut().upsert_order(o);
        e.handle(
            5.0,
            Event::Command(Command::AmendOrder {
                command: 2,
                id: "91".into(),
                price: Some(37000.0),
                amount: None,
            }),
        );
        let out = actions(&mut e);
        assert!(!out.iter().any(|a| matches!(a, Action::Http(_))));
        assert!(out.iter().any(|a| matches!(
            a,
            Action::Notice(Notice::CommandDone { result: Err(_), .. })
        )));
    }
}

#[test]
fn oco_amend_keeps_sibling_stop_and_amends_only_selected_native_leg() {
    use safe_cex_engine::model::*;
    let mut e = trading_engine();
    e.store_mut()
        .upsert_order(stored("91_sl", OrderType::StopLoss));
    let mut tp = stored("91_tp", OrderType::TakeProfit);
    tp.extra = Some(OrderExtra {
        conditional_order_price: Some(41900.0),
        ..OrderExtra::default()
    });
    e.store_mut().upsert_order(tp);
    e.handle(
        5.0,
        Event::Command(Command::AmendOrder {
            command: 2,
            id: "91_tp".into(),
            price: Some(42000.04),
            amount: Some(3.145),
        }),
    );
    let out = actions(&mut e);
    let a = http(&out);
    assert!(matches!(a, Action::Http(r) if r.url.ends_with("/amend-algos")));
    assert!(body(a).contains("\"algoId\":\"91\""));
    assert!(body(a).contains("\"newTpTriggerPx\":\"42000\""));
    assert!(body(a).contains("\"newTpTriggerPxType\":\"mark\""));
    assert!(body(a).contains("\"newSz\":\"3.15\""));
    assert!(body(a).contains("\"newTpOrdPx\":\"41900\""));
    assert!(!body(a).contains("newSl"));
    let out = ack(&mut e, a, true);
    assert!(!out.iter().any(|a| matches!(a, Action::Http(_))));
    assert_eq!(e.store().orders().len(), 2);
    assert_eq!(e.store().order("91_sl").unwrap().status, OrderStatus::Open);
}

#[test]
fn standalone_cancel_logs_item_errors_and_continues_regular_and_algo_batches() {
    use safe_cex_engine::model::*;
    let mut e = trading_engine();
    let mut ids = Vec::new();
    for i in 0..21 {
        let id = i.to_string();
        e.store_mut().upsert_order(stored(&id, OrderType::Limit));
        ids.push(id);
    }
    e.store_mut()
        .upsert_order(stored("91_sl", OrderType::StopLoss));
    ids.push("91_sl".into());
    e.handle(
        5.0,
        Event::Command(Command::CancelOrders { command: 2, ids }),
    );
    let out = actions(&mut e);
    let mut rows = vec![r#"{"ordId":"1","sCode":"0","sMsg":""}"#; 19];
    rows.push(r#"{"ordId":"2","sCode":"51400","sMsg":"Order does not exist"}"#);
    let mixed = format!(
        r#"{{"code":"2","msg":"Bulk operation partially succeeded","data":[{}]}}"#,
        rows.join(",")
    );
    reply(&mut e, http(&out), &mixed, 6.0);
    let out = actions(&mut e);
    assert!(out.iter().any(
        |a| matches!(a, Action::Notice(Notice::Log { message, .. }) if message.contains("51400"))
    ));
    let a = http(&out);
    assert!(matches!(a, Action::Http(r) if r.url.ends_with("/cancel-batch-orders")));
    assert!(body(a).contains("\"ordId\":\"20\""));
    let out = ack(&mut e, a, false);
    let a = http(&out);
    assert!(matches!(a, Action::Http(r) if r.url.ends_with("/cancel-algos")));
    let out = ack(&mut e, a, true);
    assert!(
        out.iter()
            .any(|a| matches!(a, Action::Notice(Notice::CommandDone { result: Ok(_), .. })))
    );
}

#[test]
fn replacement_never_runs_after_cancellation_rejection() {
    use safe_cex_engine::model::*;
    let mut e = trading_engine();
    e.store_mut()
        .upsert_order(stored("91_sl", OrderType::StopLoss));
    e.handle(
        5.0,
        Event::Command(Command::AmendOrder {
            command: 2,
            id: "91_sl".into(),
            price: Some(37000.0),
            amount: None,
        }),
    );
    let out = actions(&mut e);
    assert!(matches!(http(&out), Action::Http(r) if r.url.ends_with("/cancel-algos")));
    reply(
        &mut e,
        http(&out),
        &envelope(r#"{"algoId":"91","sCode":"51400","sMsg":"Order does not exist"}"#),
        6.0,
    );
    let out = actions(&mut e);
    assert!(!out.iter().any(|a| matches!(a, Action::Http(_))));
    assert!(out.iter().any(|a| matches!(
        a,
        Action::Notice(Notice::CommandDone { result: Err(_), .. })
    )));
    assert_eq!(e.store().order("91_sl").unwrap().status, OrderStatus::Open);
}

#[test]
fn terminal_ws_status_survives_a_late_amend_error() {
    use safe_cex_engine::model::*;
    let mut e = trading_engine();
    e.store_mut().upsert_order(stored("91", OrderType::Limit));
    e.handle(
        5.0,
        Event::Command(Command::AmendOrder {
            command: 2,
            id: "91".into(),
            price: Some(37000.0),
            amount: None,
        }),
    );
    let out = actions(&mut e);
    e.store_mut().set_order_status("91", OrderStatus::Closed);
    reply(
        &mut e,
        http(&out),
        &envelope(r#"{"ordId":"91","sCode":"51503","sMsg":"Already filled"}"#),
        6.0,
    );
    actions(&mut e);
    assert_eq!(e.store().order("91").unwrap().status, OrderStatus::Closed);
}

#[test]
fn conditional_amend_keeps_limit_execution_price_and_trigger_source() {
    use safe_cex_engine::model::*;
    let mut e = trading_engine();
    let mut o = stored("91", OrderType::Conditional);
    o.extra = Some(OrderExtra {
        conditional_order_price: Some(37900.0),
        ..OrderExtra::default()
    });
    e.store_mut().upsert_order(o);
    e.handle(
        5.0,
        Event::Command(Command::AmendOrder {
            command: 2,
            id: "91".into(),
            price: Some(38000.04),
            amount: Some(3.145),
        }),
    );
    let out = actions(&mut e);
    let a = http(&out);
    assert!(matches!(a, Action::Http(r) if r.url.ends_with("/amend-algos")));
    assert!(body(a).contains("\"newTriggerPx\":\"38000\""));
    assert!(body(a).contains("\"newOrdPx\":\"37900\""));
    assert!(body(a).contains("\"newTriggerPxType\":\"mark\""));
    assert!(body(a).contains("\"newSz\":\"3.15\""));
    assert!(!body(a).contains("clOrdId"));
}

#[test]
fn replacement_is_fully_preflighted_before_canceling_protection() {
    use safe_cex_engine::model::*;
    let mut e = trading_engine();
    e.store_mut()
        .upsert_order(stored("91_sl", OrderType::StopLoss));
    e.handle(
        5.0,
        Event::Command(Command::AmendOrder {
            command: 2,
            id: "91_sl".into(),
            price: Some(0.0),
            amount: None,
        }),
    );
    let out = actions(&mut e);
    assert!(!out.iter().any(|a| matches!(a, Action::Http(_))));
    assert_eq!(e.store().order("91_sl").unwrap().status, OrderStatus::Open);
}

#[test]
fn http_error_cannot_mutate_settings_even_with_success_body() {
    use safe_cex_engine::{Response, model::*};
    let mut e = trading_engine();
    e.handle(
        5.0,
        Event::Command(Command::SetLeverage {
            command: 2,
            symbol: "BTCUSDT".into(),
            leverage: 20.0,
        }),
    );
    let out = actions(&mut e);
    let Action::Http(r) = http(&out) else {
        unreachable!()
    };
    let body = envelope(r#"{"lever":"20","mgnMode":"cross"}"#);
    e.handle(
        6.0,
        Event::HttpResponse {
            request: r.id,
            response: Response {
                status: 500,
                headers: "",
                body: &body,
            },
        },
    );
    let out = actions(&mut e);
    assert!(!out.iter().any(|a| matches!(a, Action::Http(_))));
    assert!(out.iter().any(|a| matches!(
        a,
        Action::Notice(Notice::CommandDone { result: Err(_), .. })
    )));
    assert!(
        !e.store()
            .setting("BTCUSDT")
            .is_some_and(|s| s.leverage == 20.0 || s.margin_type == Some(MarginType::Cross))
    );
}

#[test]
fn positive_dust_protection_rejects_the_whole_batch_before_writes() {
    for kind in ["stop_market", "take_profit_market"] {
        let mut e = trading_engine();
        let protective = format!(
            r#"{{"symbol":"BTCUSDT","type":"{kind}","side":"sell","amount":0.001,"price":36000}}"#
        );
        let out = place(
            &mut e,
            vec![
                order(r#"{"symbol":"BTCUSDT","type":"market","side":"buy","amount":2}"#),
                order(&protective),
            ],
        );
        assert!(!out.iter().any(|a| matches!(a, Action::Http(_))));
        assert!(out.iter().any(|a| matches!(a, Action::Notice(Notice::CommandDone { result: Err(error), .. }) if error.contains("too small"))));
    }
}

#[test]
fn explicit_zero_sl_and_tp_still_close_the_whole_position() {
    for kind in ["stop_market", "take_profit_market"] {
        let mut e = trading_engine();
        let protective = format!(
            r#"{{"symbol":"BTCUSDT","type":"{kind}","side":"sell","amount":0,"price":36000}}"#
        );
        let out = place(&mut e, vec![order(&protective)]);
        let request = http(&out);
        assert!(matches!(request, Action::Http(r) if r.url.ends_with("/order-algo")));
        assert!(body(request).contains("\"closeFraction\":\"1\""));
        assert!(!body(request).contains("\"sz\""));
    }
}

#[test]
fn standalone_protective_limit_amend_preserves_original_by_rejecting_before_cancel() {
    use safe_cex_engine::model::*;
    for kind in [OrderType::StopLoss, OrderType::TakeProfit] {
        let mut e = trading_engine();
        let id = if kind == OrderType::StopLoss {
            "91_sl"
        } else {
            "91_tp"
        };
        let mut original = stored(id, kind);
        original.extra = Some(OrderExtra {
            conditional_order_price: Some(35900.0),
            ..OrderExtra::default()
        });
        e.store_mut().upsert_order(original.clone());
        e.handle(
            5.0,
            Event::Command(Command::AmendOrder {
                command: 2,
                id: id.into(),
                price: Some(36500.0),
                amount: None,
            }),
        );
        let out = actions(&mut e);
        assert!(!out.iter().any(|a| matches!(a, Action::Http(_))));
        assert!(out.iter().any(|a| matches!(a, Action::Notice(Notice::CommandDone { result: Err(error), .. }) if error.contains("execution price"))));
        assert_eq!(e.store().order(id), Some(&original));
    }
}

#[test]
fn bybit_execution_controls_reject_the_whole_batch_before_writes() {
    for control in [
        r#""bybitBbo":{"sideType":"Queue","level":1}"#,
        r#""bybitSlippageToleranceTicks":5"#,
    ] {
        let mut engine = trading_engine();
        let bad =
            format!(r#"{{"symbol":"BTCUSDT","type":"market","side":"buy","amount":1,{control}}}"#);
        let out = place(
            &mut engine,
            vec![
                order(r#"{"symbol":"BTCUSDT","type":"market","side":"buy","amount":1}"#),
                order(&bad),
            ],
        );
        assert!(!out.iter().any(|action| matches!(action, Action::Http(_))));
        assert!(out.iter().any(|action| matches!(action,
            Action::Notice(Notice::CommandDone { result: Err(message), .. })
                if message.contains("only on Bybit"))));
    }
}

#[test]
fn entry_and_algo_child_receipts_precede_each_write_and_use_sdk_symbol() {
    let mut e = trading_engine();
    let mut out = place(
        &mut e,
        vec![order(
            r#"{"symbol":"BTCUSDT","type":"limit","side":"buy","amount":2,"price":38000,"stopLoss":36000,"takeProfit":42000}"#,
        )],
    );
    let mut ids = std::collections::HashSet::new();
    for i in 0..3 {
        let write = out
            .iter()
            .position(|a| matches!(a, Action::Http(_)))
            .unwrap();
        let rows: Vec<_> = out[..write]
            .iter()
            .filter_map(|a| match a {
                Action::Notice(Notice::CommandProgress {
                    command: 1,
                    receipts,
                }) => Some(receipts),
                _ => None,
            })
            .flatten()
            .collect();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].symbol, "BTCUSDT");
        assert!(ids.insert(rows[0].identity.clone()));
        let Action::Http(r) = &out[write] else {
            panic!()
        };
        let body: serde_json::Value = serde_json::from_str(r.body.as_deref().unwrap()).unwrap();
        assert_eq!(
            rows[0].identity,
            if i == 0 {
                body[0]["clOrdId"].as_str().unwrap()
            } else {
                body["algoClOrdId"].as_str().unwrap()
            }
        );
        reply(
            &mut e,
            &out[write],
            &envelope(&format!(r#"{{"ordId":"{i}","algoId":"{i}","sCode":"0"}}"#)),
            6.0 + i as f64,
        );
        out = actions(&mut e);
    }
    assert_eq!(ids.len(), 3);
}
