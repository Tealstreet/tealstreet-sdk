#![allow(dead_code)]
mod support;
use crate::support::*;
use safe_cex_engine::model::PlaceOrderOpts;
use safe_cex_engine::{Action, Command, Event, Notice};
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
    prepared(e, 5.0)
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
    assert!(prepared(&mut e, 6.0).iter().any(
        |a| matches!(a, Action::Notice(Notice::CommandDone { result: s, .. }) if s == "[\"1234\"]")
    ));
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
        assert!(
            out.iter()
                .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { .. })))
        );
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
    let a = prepared(&mut e, 6.0)
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
    let a = prepared(&mut e, 7.0)
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
    assert!(prepared(&mut e, 8.0).iter().any(|a| matches!(a, Action::Notice(Notice::CommandDone { result: s, .. }) if s == "[\"1\",\"2\",\"3\"]")));
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
    let out = prepared(&mut e, 6.0);
    assert!(out.iter().any(|a| matches!(a, Action::Http(r) if r.method == safe_cex_engine::Method::Get && r.url.contains("orders-pending"))));
    assert!(
        !out.iter()
            .any(|a| matches!(a, Action::Http(r) if r.method != safe_cex_engine::Method::Get))
    );
    assert!(
        out.iter()
            .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { .. })))
    );
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
    prepared(e, 6.0)
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
    assert!(
        out.iter()
            .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { .. })))
    );
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
    let out = prepared(&mut e, 7.0);
    assert!(!out.iter().any(|a| matches!(a, Action::Http(_))));
    assert!(
        out.iter()
            .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { .. })))
    );
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
        let out = prepared(&mut e, 5.0);
        assert!(!out.iter().any(|a| matches!(a, Action::Http(_))));
        assert!(
            out.iter()
                .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { .. })))
        );
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
    let out = prepared(&mut e, 5.0);
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
    let out = prepared(&mut e, 5.0);
    let mut rows = vec![r#"{"ordId":"1","sCode":"0","sMsg":""}"#; 19];
    rows.push(r#"{"ordId":"2","sCode":"51400","sMsg":"Order does not exist"}"#);
    let mixed = format!(
        r#"{{"code":"2","msg":"Bulk operation partially succeeded","data":[{}]}}"#,
        rows.join(",")
    );
    reply(&mut e, http(&out), &mixed, 6.0);
    let out = prepared(&mut e, 6.0);
    assert!(out.iter().any(
        |a| matches!(a, Action::Notice(Notice::CommandError { command:2, message }) if message.contains("51400"))
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
            .any(|a| matches!(a, Action::Notice(Notice::CommandDone { .. })))
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
    let out = prepared(&mut e, 5.0);
    assert!(matches!(http(&out), Action::Http(r) if r.url.ends_with("/cancel-algos")));
    reply(
        &mut e,
        http(&out),
        &envelope(r#"{"algoId":"91","sCode":"51400","sMsg":"Order does not exist"}"#),
        6.0,
    );
    let out = prepared(&mut e, 6.0);
    assert!(!out.iter().any(|a| matches!(a, Action::Http(_))));
    assert!(
        out.iter()
            .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { .. })))
    );
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
    let out = prepared(&mut e, 5.0);
    e.store_mut().set_order_status("91", OrderStatus::Closed);
    reply(
        &mut e,
        http(&out),
        &envelope(r#"{"ordId":"91","sCode":"51503","sMsg":"Already filled"}"#),
        6.0,
    );
    prepared(&mut e, 6.0);
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
    let out = prepared(&mut e, 5.0);
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
    let out = prepared(&mut e, 5.0);
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
            side: None,
        }),
    );
    let out = prepared(&mut e, 5.0);
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
    let out = prepared(&mut e, 6.0);
    assert!(
        out.iter()
            .any(|a| matches!(a, Action::Http(r) if r.method == safe_cex_engine::Method::Get)),
        "ambiguous server error must reconcile"
    );
    assert!(
        !out.iter()
            .any(|a| matches!(a, Action::Http(r) if r.method != safe_cex_engine::Method::Get)),
        "write must not be retried"
    );
    assert!(
        out.iter()
            .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { .. })))
    );
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
        assert!(out.iter().any(|a| matches!(a, Action::Notice(Notice::CommandFailed { error, .. }) if error.legacy_text().contains("too small"))));
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
        let out = prepared(&mut e, 5.0);
        assert!(!out.iter().any(|a| matches!(a, Action::Http(_))));
        assert!(out.iter().any(|a| matches!(a, Action::Notice(Notice::CommandFailed { error, .. }) if error.legacy_text().contains("execution price"))));
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
            Action::Notice(Notice::CommandFailed { error: message, .. })
                if message.legacy_text().contains("only on Bybit"))));
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
        out = prepared(&mut e, 6.0 + i as f64);
    }
    assert_eq!(ids.len(), 3);
}

#[test]
fn lost_cancel_and_amend_responses_keep_pending_state_until_a_fresh_read() {
    use safe_cex_engine::model::{OrderStatus, OrderType};
    for amend in [false, true] {
        let mut e = trading_engine();
        e.store_mut()
            .upsert_order(stored("uncertain", OrderType::Limit));
        e.handle(
            5.0,
            Event::Command(if amend {
                Command::AmendOrder {
                    command: 11,
                    id: "uncertain".into(),
                    price: Some(37000.0),
                    amount: None,
                }
            } else {
                Command::CancelOrders {
                    command: 11,
                    ids: vec!["uncertain".into()],
                }
            }),
        );
        let request = prepared(&mut e, 5.0)
            .into_iter()
            .find_map(|a| match a {
                Action::Http(r) => Some(r),
                _ => None,
            })
            .unwrap();
        e.handle(
            6.0,
            Event::HttpFailed {
                request: request.id,
                error: "timeout after exchange acceptance",
            },
        );
        let out = prepared(&mut e, 6.0);
        assert_eq!(
            e.store().order("uncertain").unwrap().status,
            if amend {
                OrderStatus::Updating
            } else {
                OrderStatus::Canceling
            }
        );
        assert!(out.iter().any(|a| matches!(a, Action::Http(r) if r.method == safe_cex_engine::Method::Get && r.url.contains("orders-pending"))));
        assert!(
            !out.iter()
                .any(|a| matches!(a, Action::Http(r) if r.method != safe_cex_engine::Method::Get))
        );
        assert!(
            out.iter()
                .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 11, .. })))
        );
    }
}

#[test]
fn cancel_all_includes_acknowledged_entries_and_brackets_before_any_stream_rows() {
    let mut e = trading_engine();
    let mut out = place(
        &mut e,
        vec![order(
            r#"{"symbol":"BTCUSDT","type":"limit","side":"buy","amount":2,"price":38000,"stopLoss":36000,"takeProfit":42000}"#,
        )],
    );
    for (id, algo) in [("entry", false), ("stop", true), ("profit", true)] {
        let request = out.iter().find(|a| matches!(a, Action::Http(_))).unwrap();
        let ack = if algo {
            format!(r#"{{"algoId":"{id}","sCode":"0"}}"#)
        } else {
            format!(r#"{{"ordId":"{id}","sCode":"0"}}"#)
        };
        reply(&mut e, request, &envelope(&ack), 6.0);
        out = prepared(&mut e, 6.0);
    }
    assert!(
        e.store().orders().is_empty(),
        "the private stream has not populated any rows"
    );
    e.handle(
        7.0,
        Event::Command(Command::CancelSymbolOrders {
            command: 2,
            symbol: "BTCUSDT".into(),
        }),
    );
    let out = prepared(&mut e, 7.0);
    let request = out.iter().find(|a| matches!(a, Action::Http(_))).unwrap();
    assert!(
        matches!(request, Action::Http(r) if r.url.ends_with("/cancel-batch-orders") && r.body.as_ref().unwrap().contains("entry"))
    );
    reply(
        &mut e,
        request,
        &envelope(r#"{"ordId":"entry","sCode":"0"}"#),
        8.0,
    );
    let out = prepared(&mut e, 8.0);
    let request = out.iter().find(|a| matches!(a, Action::Http(_))).unwrap();
    assert!(
        matches!(request, Action::Http(r) if r.url.ends_with("/cancel-algos") && r.body.as_ref().unwrap().contains("stop") && r.body.as_ref().unwrap().contains("profit"))
    );
    reply(
        &mut e,
        request,
        r#"{"code":"0","data":[{"algoId":"stop","sCode":"0"},{"algoId":"profit","sCode":"0"}]}"#,
        9.0,
    );
    assert!(prepared(&mut e, 9.0).iter().any(|a| matches!(
        a,
        Action::Notice(Notice::CommandDone {
            command: 2,
            result: _
        })
    )));
}

#[test]
fn partial_batch_accepted_order_remains_cancelable_before_the_stream() {
    let mut e = trading_engine();
    let out = place(
        &mut e,
        vec![
            order(r#"{"symbol":"BTCUSDT","type":"limit","side":"buy","amount":2,"price":38000}"#),
            order(r#"{"symbol":"BTCUSDT","type":"limit","side":"sell","amount":2,"price":42000}"#),
        ],
    );
    let request = out.iter().find(|a| matches!(a, Action::Http(_))).unwrap();
    reply(
        &mut e,
        request,
        r#"{"code":"2","data":[{"ordId":"accepted","sCode":"0"},{"sCode":"51008","sMsg":"Insufficient margin"}]}"#,
        6.0,
    );
    assert!(
        prepared(&mut e, 6.0)
            .iter()
            .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 1, .. })))
    );
    e.handle(
        7.0,
        Event::Command(Command::CancelSymbolOrders {
            command: 2,
            symbol: "BTCUSDT".into(),
        }),
    );
    assert!(prepared(&mut e, 7.0).iter().any(|a| matches!(a, Action::Http(r) if r.url.ends_with("/cancel-batch-orders") && r.body.as_ref().unwrap().contains("accepted"))));
}

fn ready_with_held_reconciliation() -> (
    safe_cex_engine::Engine<safe_cex_venue_okx::Okx>,
    Vec<Action>,
) {
    use crate::support::FixtureOptions as Options;
    let mut e = crate::support::engine(Options {
        key: Some("key".into()),
        secret: Some("secret".into()),
        passphrase: Some("pass".into()),
        ..Default::default()
    });
    e.start(0.);
    let request = http(&prepared(&mut e, 0.)).clone();
    reply(&mut e, &request, &envelope(MARKET), 1.);
    // Private startup begins with the catalogue, alongside the ticker seed.
    let out = prepared(&mut e, 1.);
    let tickers = out
        .iter()
        .find(|a| is_get(a, "/api/v5/market/tickers?instType=SWAP"))
        .unwrap();
    reply(&mut e, tickers, &envelope(TICKER), 2.);
    let config = out
        .iter()
        .find(|a| is_get(a, "/api/v5/account/config"))
        .unwrap();
    reply(
        &mut e,
        config,
        &envelope(r#"{"posMode":"net_mode","acctLv":"2"}"#),
        3.,
    );
    let out = prepared(&mut e, 3.);
    let private = out
        .iter()
        .find_map(|a| match a {
            Action::WsConnect { socket, url, .. } if url.ends_with("/private") => Some(*socket),
            _ => None,
        })
        .unwrap();
    let mut reads: Vec<_> = out
        .into_iter()
        .filter(|a| matches!(a, Action::Http(_)))
        .collect();
    while let Some(read) = reads.pop() {
        let body = if matches!(&read, Action::Http(r) if r.url.contains("account-position-risk")) {
            envelope(r#"{"balData":[{"ccy":"USDT","eq":"5000"}]}"#)
        } else {
            envelope("")
        };
        reply(&mut e, &read, &body, 4.);
        reads.extend(
            prepared(&mut e, 4.)
                .into_iter()
                .filter(|a| matches!(a, Action::Http(_))),
        );
    }
    e.handle(5., Event::WsOpened { socket: private });
    prepared(&mut e, 5.);
    e.handle(
        5.,
        Event::WsMessage {
            socket: private,
            text: r#"{"event":"login","code":"0","msg":""}"#,
        },
    );
    let mut handed_off = prepared(&mut e, 5.);
    let wakeups: Vec<_> = handed_off
        .iter()
        .filter_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == 5. => Some(*timer),
            _ => None,
        })
        .collect();
    for timer in wakeups {
        e.handle(5., Event::TimerFired { timer });
        handed_off.extend(prepared(&mut e, 5.));
    }
    let held: Vec<_> = handed_off
        .into_iter()
        .filter(|a| matches!(a, Action::Http(_)))
        .collect();
    assert!(
        held.iter()
            .any(|a| matches!(a, Action::Http(r) if r.url.contains("orders-pending")))
    );
    (e, held)
}

#[test]
fn ambiguous_http_and_native_timeouts_preserve_writes_and_force_fresh_reconciliation() {
    use safe_cex_engine::model::{OrderStatus, OrderType};
    use safe_cex_engine::{Method, Response};
    // https://www.okx.com/en-us/help/api-faq-eea: 50004 may succeed or fail.
    for (status, body) in [
        (408, "request timed out"),
        (500, "internal error"),
        (503, "unavailable"),
        (504, "gateway timeout"),
        (
            200,
            r#"{"code":"50004","msg":"Endpoint request timeout","data":[]}"#,
        ),
        (
            400,
            r#"{"code":"50004","msg":"Endpoint request timeout","data":[]}"#,
        ),
        (
            200,
            r#"{"code":"0","data":[{"ordId":"uncertain","sCode":"50004","sMsg":"Operation timeout"}]}"#,
        ),
        (200, "truncated"),
        (200, r#"{"code":"0","data":{}}"#),
        (200, r#"{"code":"0","data":[]}"#),
        (200, r#"{"code":"0","data":[{}]}"#),
    ] {
        for amend in [false, true] {
            let (mut e, held) = ready_with_held_reconciliation();
            e.store_mut()
                .upsert_order(stored("uncertain", OrderType::Limit));
            e.handle(
                5.,
                Event::Command(if amend {
                    Command::AmendOrder {
                        command: 81,
                        id: "uncertain".into(),
                        price: Some(37000.),
                        amount: None,
                    }
                } else {
                    Command::CancelOrders {
                        command: 81,
                        ids: vec!["uncertain".into()],
                    }
                }),
            );
            let write = http(&prepared(&mut e, 5.)).clone();
            let Action::Http(write) = write else {
                unreachable!()
            };
            e.handle(
                6.,
                Event::HttpResponse {
                    request: write.id,
                    response: Response {
                        status,
                        headers: "",
                        body,
                    },
                },
            );
            let out = prepared(&mut e, 6.);
            let expected = if amend {
                OrderStatus::Updating
            } else {
                OrderStatus::Canceling
            };
            assert_eq!(
                e.store().order("uncertain").unwrap().status,
                expected,
                "{status}: {body}"
            );
            assert!(
                out.iter().any(|a| matches!(
                    a,
                    Action::Notice(Notice::CommandFailed { command: 81, .. })
                ))
            );
            assert!(
                !out.iter()
                    .any(|a| matches!(a, Action::Http(r) if r.method != Method::Get)),
                "write was replayed"
            );
            let mut fresh: Vec<_> = out
                .into_iter()
                .filter(|a| matches!(a, Action::Http(r) if r.url.contains("/trade/orders")))
                .collect();
            assert!(
                !fresh.is_empty(),
                "held snapshot cannot prevent a new order read"
            );
            for read in held {
                reply(&mut e, &read, &envelope(""), 7.);
            }
            assert_eq!(
                e.store().order("uncertain").unwrap().status,
                expected,
                "pre-write empty snapshot must not clear pending state"
            );
            prepared(&mut e, 7.);
            while let Some(read) = fresh.pop() {
                reply(&mut e, &read, &envelope(""), 8.);
                fresh.extend(
                    prepared(&mut e, 8.).into_iter().filter(
                        |a| matches!(a, Action::Http(r) if r.url.contains("/trade/orders")),
                    ),
                );
            }
            assert!(
                e.store().order("uncertain").is_none(),
                "fresh complete empty reconciliation must retire the order"
            );
        }
    }
}

#[test]
fn definite_business_rejection_restores_cancel_or_amend_state() {
    use safe_cex_engine::{
        Response,
        model::{OrderStatus, OrderType},
    };
    for amend in [false, true] {
        let mut e = trading_engine();
        e.store_mut()
            .upsert_order(stored("rejected", OrderType::Limit));
        e.handle(
            5.,
            Event::Command(if amend {
                Command::AmendOrder {
                    command: 82,
                    id: "rejected".into(),
                    price: Some(37000.),
                    amount: None,
                }
            } else {
                Command::CancelOrders {
                    command: 82,
                    ids: vec!["rejected".into()],
                }
            }),
        );
        let Action::Http(write) = http(&prepared(&mut e, 5.)).clone() else {
            unreachable!()
        };
        e.handle(
            6.,
            Event::HttpResponse {
                request: write.id,
                response: Response {
                    status: 200,
                    headers: "",
                    body: r#"{"code":"1","data":[{"sCode":"51000","sMsg":"Parameter error"}]}"#,
                },
            },
        );
        let out = prepared(&mut e, 6.);
        assert_eq!(
            e.store().order("rejected").unwrap().status,
            OrderStatus::Open
        );
        assert!(
            out.iter()
                .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 82, .. })))
        );
    }
}

#[test]
fn cancellation_missing_from_store_errors_before_any_write_even_in_a_mixed_batch() {
    use safe_cex_engine::model::{Order, OrderSide, OrderStatus, OrderType};
    use safe_cex_engine::{Action, Notice};
    for mixed in [false, true] {
        let mut e = trading_engine();
        e.store_mut().set_orders(vec![Order {
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
        e.drain_actions().for_each(drop);
        e.handle(
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
        let actions = prepared(&mut e, 2000.0);
        assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
        assert!(
            actions.iter().any(|a| matches!(
                a,
                Action::Notice(Notice::CommandFailed { command: 991, error })
                    if error.legacy_text().contains("456") && error.legacy_text().contains("not found")
            )),
            "missing cancellation error: {actions:?}"
        );
        assert_eq!(e.store().order("123").unwrap().status, OrderStatus::Open);
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
        let mut e = trading_engine();
        e.store_mut().update_markets(|market| {
            market.limits.leverage.min = min;
            market.limits.leverage.max = max;
        });
        e.handle(
            5.0,
            Event::Command(Command::SetLeverage {
                command: 99,
                symbol: "BTCUSDT".into(),
                leverage: 5.0,
                side: None,
            }),
        );
        let out = prepared(&mut e, 5.0);
        assert!(!out.iter().any(|action| matches!(action, Action::Http(_))));
        assert!(out.iter().any(|action| matches!(action, Action::Notice(Notice::CommandFailed { command: 99, error }) if error.legacy_text().contains("leverage limits"))));
    }
}

#[test]
fn fetch_leverage_reads_flat_symbol_cross_leverage_into_settings() {
    let mut e = trading_engine();
    assert!(
        e.store()
            .setting("BTCUSDT")
            .is_none_or(|s| s.leverage == 0.0)
    );
    e.handle(
        5.0,
        Event::Command(Command::FetchLeverage {
            command: 7,
            symbol: "BTCUSDT".into(),
        }),
    );
    let out = prepared(&mut e, 5.0);
    let request = out
        .iter()
        .find(|a| {
            is_get(
                a,
                "/api/v5/account/leverage-info?instId=BTC-USDT-SWAP&mgnMode=cross",
            )
        })
        .expect("leverage-info read");
    reply(
        &mut e,
        request,
        &envelope(r#"{"instId":"BTC-USDT-SWAP","mgnMode":"cross","posSide":"","lever":"25"}"#),
        6.0,
    );
    assert!(prepared(&mut e, 6.0).iter().any(|a| matches!(a, Action::Notice(Notice::CommandDone { command: 7, result: s }) if s.contains("\"buyLeverage\":25.0") && s.contains("\"sellLeverage\":25.0"))));
    assert_eq!(e.store().setting("BTCUSDT").unwrap().leverage, 25.0);
}

#[test]
fn fetch_leverage_failure_answers_instead_of_hanging() {
    let mut e = trading_engine();
    e.handle(
        5.0,
        Event::Command(Command::FetchLeverage {
            command: 8,
            symbol: "BTCUSDT".into(),
        }),
    );
    let out = prepared(&mut e, 5.0);
    let request = out.iter().find(|a| is_get(a, "mgnMode=cross")).unwrap();
    reply(
        &mut e,
        request,
        r#"{"code":"50011","msg":"Too Many Requests","data":[]}"#,
        6.0,
    );
    assert!(
        prepared(&mut e, 6.0)
            .iter()
            .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 8, .. })))
    );
    e.handle(
        7.0,
        Event::Command(Command::FetchLeverage {
            command: 9,
            symbol: "NOPE".into(),
        }),
    );
    assert!(
        prepared(&mut e, 7.0)
            .iter()
            .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 9, .. })))
    );
}

/// A ready account plus every action emitted after its config reply.
fn ready_engine() -> (
    safe_cex_engine::Engine<safe_cex_venue_okx::Okx>,
    Vec<Action>,
) {
    let mut e = crate::support::engine(crate::support::FixtureOptions {
        key: Some("test-key".into()),
        secret: Some("test-secret".into()),
        passphrase: Some("test-passphrase".into()),
        ..Default::default()
    });
    e.start(0.0);
    let a = take_http(&mut prepared(&mut e, 0.0));
    reply(&mut e, &a, &envelope(MARKET), 1.0);
    // Private startup begins with the catalogue, alongside the ticker seed.
    let out = prepared(&mut e, 1.0);
    let tickers = out
        .iter()
        .find(|a| is_get(a, "/api/v5/market/tickers?instType=SWAP"))
        .unwrap();
    reply(&mut e, tickers, &envelope(TICKER), 2.0);
    let a = out
        .into_iter()
        .find(|a| is_get(a, "/api/v5/account/config"))
        .unwrap();
    reply(
        &mut e,
        &a,
        &envelope(r#"{"posMode":"net_mode","acctLv":"2"}"#),
        3.0,
    );
    let mut seen = prepared(&mut e, 3.0);
    let mut pending: Vec<_> = seen
        .iter()
        .filter(|a| matches!(a, Action::Http(_)))
        .cloned()
        .collect();
    while let Some(a) = pending.pop() {
        let body = if matches!(&a, Action::Http(r) if r.url.contains("account-position-risk")) {
            envelope(r#"{"balData":[{"ccy":"USDT","eq":"5000"}]}"#)
        } else {
            envelope("")
        };
        reply(&mut e, &a, &body, 3.0);
        let out = prepared(&mut e, 3.0);
        pending.extend(out.iter().filter(|a| matches!(a, Action::Http(_))).cloned());
        seen.extend(out);
    }
    (e, seen)
}

/// Fires the 2s account tick until the periodic config read is sent.
fn tick_until_config(
    e: &mut safe_cex_engine::Engine<safe_cex_venue_okx::Okx>,
    mut out: Vec<Action>,
) -> (Action, f64) {
    let mut now = 3.0;
    loop {
        let tick = out
            .iter()
            .find_map(|a| match a {
                Action::SetTimer { timer, at_ms } if *at_ms == now + 2_000.0 => Some(*timer),
                _ => None,
            })
            .unwrap_or_else(|| panic!("account tick not re-armed at {now}"));
        now += 2_000.0;
        e.handle(now, Event::TimerFired { timer: tick });
        out = prepared(e, now);
        if let Some(config) = out.iter().find(|a| is_get(a, "/api/v5/account/config")) {
            assert!(now >= 30_000.0, "polled too early at {now}");
            return (config.clone(), now);
        }
        assert!(now < 40_000.0, "config never re-read");
    }
}

#[test]
fn account_config_is_reread_and_a_mode_change_in_the_okx_app_is_applied() {
    let (mut e, out) = ready_engine();
    assert!(!e.store().hedged());
    let (config, now) = tick_until_config(&mut e, out);
    reply(
        &mut e,
        &config,
        &envelope(r#"{"posMode":"long_short_mode","acctLv":"2"}"#),
        now,
    );
    assert!(e.store().hedged());
    assert!(
        prepared(&mut e, now)
            .iter()
            .any(|a| is_get(a, "/api/v5/account/positions?instType=SWAP"))
    );
}

#[test]
fn config_read_sent_before_our_mode_write_cannot_revert_it() {
    let (mut e, out) = ready_engine();
    let (config, now) = tick_until_config(&mut e, out);
    e.handle(
        now,
        Event::Command(Command::SetPositionMode {
            command: 11,
            hedged: true,
            symbol: None,
        }),
    );
    let write = prepared(&mut e, now)
        .into_iter()
        .find(|a| matches!(a, Action::Http(r) if r.url.ends_with("/api/v5/account/set-position-mode")))
        .expect("mode write");
    reply(
        &mut e,
        &write,
        &envelope(r#"{"posMode":"long_short_mode"}"#),
        now + 1.0,
    );
    assert!(e.store().hedged());
    reply(
        &mut e,
        &config,
        &envelope(r#"{"posMode":"net_mode","acctLv":"2"}"#),
        now + 2.0,
    );
    assert!(
        e.store().hedged(),
        "stale poll reverted the confirmed mode write"
    );
}

#[test]
fn unparseable_polled_account_level_keeps_the_known_level() {
    let (mut e, out) = ready_engine();
    let (config, now) = tick_until_config(&mut e, out);
    reply(
        &mut e,
        &config,
        &envelope(r#"{"posMode":"net_mode","acctLv":""}"#),
        now,
    );
    assert!(
        !prepared(&mut e, now)
            .iter()
            .any(|a| is_get(a, "/api/v5/account/positions?instType=SWAP")),
        "unchanged mode and unknown level must not reconcile"
    );
}

#[test]
fn leverage_read_survives_unrelated_private_pushes() {
    let mut e = trading_engine();
    e.handle(
        5.0,
        Event::Command(Command::FetchLeverage {
            command: 12,
            symbol: "BTCUSDT".into(),
        }),
    );
    let request = prepared(&mut e, 5.0)
        .into_iter()
        .find(|a| is_get(a, "mgnMode=cross"))
        .unwrap();
    // Any account write bumps the account revision without touching leverage.
    e.handle(
        5.5,
        Event::Command(Command::SetPositionMode {
            command: 13,
            hedged: true,
            symbol: None,
        }),
    );
    let write = prepared(&mut e, 5.5)
        .into_iter()
        .find(|a| matches!(a, Action::Http(r) if r.url.ends_with("set-position-mode")))
        .unwrap();
    reply(
        &mut e,
        &write,
        &envelope(r#"{"posMode":"long_short_mode"}"#),
        5.6,
    );
    prepared(&mut e, 5.6);
    reply(
        &mut e,
        &request,
        &envelope(r#"{"instId":"BTC-USDT-SWAP","mgnMode":"cross","lever":"30"}"#),
        6.0,
    );
    assert!(prepared(&mut e, 6.0).iter().any(|a| matches!(a, Action::Notice(Notice::CommandDone { command: 12, result: s }) if s.contains("30.0"))));
    assert_eq!(e.store().setting("BTCUSDT").unwrap().leverage, 30.0);
}

#[test]
fn trading_failures_are_classified_without_changing_their_text() {
    use safe_cex_engine::{ErrorKind, Outcome};
    let limit = r#"{"symbol":"BTCUSDT","type":"limit","side":"buy","amount":1,"price":38000}"#;
    for (body, kind, outcome, code) in [
        (
            r#"{"code":"1","msg":"","data":[{"ordId":"","sCode":"51008","sMsg":"Insufficient margin"}]}"#,
            Some(ErrorKind::Rejected),
            Some(Outcome::Rejected),
            Some("51008"),
        ),
        (
            r#"{"code":"50011","msg":"Too Many Requests","data":[]}"#,
            Some(ErrorKind::RateLimited),
            Some(Outcome::Rejected),
            Some("50011"),
        ),
        (
            r#"{"code":"0","msg":"","data":[{"ordId":"","sCode":"50004","sMsg":"timeout"}]}"#,
            // An unproven acknowledgement is the engine's fixed Internal/Unknown.
            Some(ErrorKind::Internal),
            Some(Outcome::Unknown),
            None,
        ),
        (
            r#"{"code":"1","msg":"","data":[{"ordId":"","sCode":"50026","sMsg":"System error"}]}"#,
            // A non-51xxx row proves no refusal (dazzy3, #2117 Option B): Unknown,
            // which stops the chain and reconciles, rather than a definitive kind.
            None,
            Some(Outcome::Unknown),
            Some("50026"),
        ),
        (
            r#"{"code":"50013","msg":"Systems are busy","data":[]}"#,
            // Busy proves nothing: Unknown, with no refusal kind.
            None,
            Some(Outcome::Unknown),
            Some("50013"),
        ),
    ] {
        let mut e = trading_engine();
        let out = place(&mut e, vec![order(limit)]);
        let request = out.iter().find(|a| matches!(a, Action::Http(_))).unwrap();
        reply(&mut e, request, body, 6.0);
        let failure = e
            .drain_actions()
            .find_map(|a| match a {
                Action::Notice(Notice::CommandFailed { error, .. }) => Some(error),
                _ => None,
            })
            .unwrap_or_else(|| panic!("{body}"));
        assert_eq!(
            (failure.kind, failure.outcome, failure.venue_code.as_deref()),
            (kind, outcome, code),
            "{body}: {}",
            failure.message
        );
    }
}

#[test]
fn a_partially_accepted_batch_is_not_reported_as_rejected() {
    let mut e = trading_engine();
    let out = place(
        &mut e,
        vec![
            order(r#"{"symbol":"BTCUSDT","type":"limit","side":"buy","amount":2,"price":38000}"#),
            order(r#"{"symbol":"BTCUSDT","type":"limit","side":"sell","amount":2,"price":42000}"#),
        ],
    );
    let request = out.iter().find(|a| matches!(a, Action::Http(_))).unwrap();
    reply(
        &mut e,
        request,
        r#"{"code":"2","data":[{"ordId":"accepted","sCode":"0"},{"sCode":"51008","sMsg":"Insufficient margin"}]}"#,
        6.0,
    );
    let failure = e
        .drain_actions()
        .find_map(|a| match a {
            Action::Notice(Notice::CommandFailed { error, .. }) => Some(error),
            _ => None,
        })
        .expect("failed command");
    assert_eq!(failure.outcome, None, "one order is live: {failure:?}");
}

#[test]
fn order_read_with_a_rejected_row_does_not_clear_an_uncertain_placement() {
    use safe_cex_engine::Response;
    for malformed in [true, false] {
        let mut e = trading_engine();
        let out = place(
            &mut e,
            vec![order(
                r#"{"symbol":"BTCUSDT","type":"limit","side":"buy","amount":2,"price":38000}"#,
            )],
        );
        let Action::Http(write) = http(&out).clone() else {
            unreachable!()
        };
        e.handle(
            6.0,
            Event::HttpResponse {
                request: write.id,
                response: Response {
                    status: 504,
                    headers: "",
                    body: "gateway timeout",
                },
            },
        );
        // The write's own reconciliation predates its failure; fail it so the retry reads after.
        let read = prepared(&mut e, 6.0)
            .into_iter()
            .find(|a| matches!(a, Action::Http(r) if r.url.contains("orders-pending")))
            .expect("an uncertain placement must reconcile");
        let Action::Http(read) = read else {
            unreachable!()
        };
        e.handle(
            7.0,
            Event::HttpResponse {
                request: read.id,
                response: Response {
                    status: 500,
                    headers: "",
                    body: "internal error",
                },
            },
        );
        let retry = prepared(&mut e, 7.0)
            .into_iter()
            .find_map(|a| match a {
                Action::SetTimer {
                    timer,
                    at_ms: 5_007.0,
                } => Some(timer),
                _ => None,
            })
            .expect("retry timer");
        e.handle(5_007.0, Event::TimerFired { timer: retry });
        let mut reads: Vec<_> = prepared(&mut e, 5_007.0)
            .into_iter()
            .filter(|a| matches!(a, Action::Http(r) if r.url.contains("/trade/orders")))
            .collect();
        assert!(!reads.is_empty(), "the retry reads orders again");
        let mut first = malformed;
        while let Some(read) = reads.pop() {
            let body = if std::mem::take(&mut first) {
                envelope(r#"{"instId":"BTC-USDT-SWAP","ordId":"maybe","sz":4}"#)
            } else {
                envelope("")
            };
            reply(&mut e, &read, &body, 5_008.0);
            reads.extend(
                prepared(&mut e, 5_008.0)
                    .into_iter()
                    .filter(|a| matches!(a, Action::Http(r) if r.url.contains("/trade/orders"))),
            );
        }
        e.handle(
            5_009.0,
            Event::Command(Command::CancelSymbolOrders {
                command: 2,
                symbol: "BTCUSDT".into(),
            }),
        );
        let refused = prepared(&mut e, 5_009.0).iter().any(|a| {
            matches!(
                a,
                Action::Notice(Notice::CommandFailed { command: 2, error })
                    if error.legacy_text().contains("uncertain")
            )
        });
        assert_eq!(refused, malformed);
        assert!(e.store().loaded().orders);
    }
}

#[test]
fn conditional_zero_price_executes_at_market_once_triggered() {
    let mut e = trading_engine();
    let out = place(
        &mut e,
        vec![order(
            r#"{"symbol":"BTCUSDT","type":"conditional","side":"buy","amount":1,"triggerPrice":41000,"price":0}"#,
        )],
    );
    let request = http(&out);
    assert!(matches!(request, Action::Http(r) if r.url.ends_with("/order-algo")));
    assert!(
        body(request).contains(r#""orderPx":"-1""#),
        "{}",
        body(request)
    );
}

/// Every terminal published for `command`: `Ok(result)` or `Err(error)`.
fn terminals(out: &[Action], command: u32) -> Vec<Result<String, safe_cex_engine::VenueError>> {
    out.iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::CommandDone { command: c, result }) if *c == command => {
                Some(Ok(result.clone()))
            }
            Action::Notice(Notice::CommandFailed { command: c, error }) if *c == command => {
                Some(Err(error.clone()))
            }
            _ => None,
        })
        .collect()
}

fn sends_io(out: &[Action]) -> bool {
    out.iter()
        .any(|a| matches!(a, Action::Http(_) | Action::ManagedHttp(_)))
}

fn is_leverage_read(action: &Action) -> bool {
    is_get(
        action,
        "/api/v5/account/leverage-info?instId=BTC-USDT-SWAP&mgnMode=cross",
    )
}

fn fetch_leverage(
    e: &mut safe_cex_engine::Engine<safe_cex_venue_okx::Okx>,
    command: u32,
    symbol: &str,
    now: f64,
) -> Vec<Action> {
    e.handle(
        now,
        Event::Command(Command::FetchLeverage {
            command,
            symbol: symbol.into(),
        }),
    );
    prepared(e, now)
}

/// Fires, in time order, every timer armed by `out` or by the timers it
/// fires, up to and including `until`; returns all actions they produce.
fn fire_armed(
    e: &mut safe_cex_engine::Engine<safe_cex_venue_okx::Okx>,
    out: &[Action],
    until: f64,
) -> Vec<Action> {
    fn track(timers: &mut Vec<(safe_cex_engine::TimerId, f64)>, out: &[Action]) {
        for action in out {
            match action {
                Action::SetTimer { timer, at_ms } => timers.push((*timer, *at_ms)),
                Action::ClearTimer { timer } => timers.retain(|(t, _)| t != timer),
                _ => {}
            }
        }
    }
    let mut timers = Vec::new();
    track(&mut timers, out);
    let mut seen = Vec::new();
    for _ in 0..10_000 {
        timers.sort_by(|a, b| a.1.total_cmp(&b.1));
        let Some(&(timer, at)) = timers.first() else {
            return seen;
        };
        if at > until {
            return seen;
        }
        timers.remove(0);
        e.handle(at, Event::TimerFired { timer });
        let out = prepared(e, at);
        track(&mut timers, &out);
        seen.extend(out);
    }
    panic!("timer chain did not settle by {until}");
}

#[test]
fn fetch_leverage_unanswered_read_fails_at_shared_deadline() {
    let mut e = trading_engine();
    let before = e.store().setting("BTCUSDT").map(|s| s.leverage);
    let out = fetch_leverage(&mut e, 7, "BTCUSDT", 5.0);
    let request = out
        .iter()
        .find(|a| is_leverage_read(a))
        .cloned()
        .expect("leverage-info read");
    let mut seen = out.clone();
    seen.extend(fire_armed(&mut e, &out, 5.0 + 20_000.0));
    let failed = terminals(&seen, 7);
    assert_eq!(
        failed.len(),
        1,
        "an unanswered leverage read needs exactly one terminal by the 20 s shared deadline"
    );
    let error = failed[0].as_ref().expect_err("deadline is a failure");
    assert_eq!(error.kind, Some(safe_cex_engine::ErrorKind::Timeout));
    reply(
        &mut e,
        &request,
        &envelope(r#"{"instId":"BTC-USDT-SWAP","mgnMode":"cross","posSide":"","lever":"25"}"#),
        20_006.0,
    );
    let late = prepared(&mut e, 20_006.0);
    assert!(terminals(&late, 7).is_empty(), "no second terminal");
    assert_eq!(
        e.store().setting("BTCUSDT").map(|s| s.leverage),
        before,
        "a late reply cannot publish leverage after the deadline"
    );
}

#[test]
fn leverage_reads_beyond_shared_capacity_are_refused_before_io() {
    use safe_cex_engine::request_context::Error;
    let mut e = trading_engine();
    let mut reads = Vec::new();
    for command in 1_000..1_256 {
        let out = fetch_leverage(&mut e, command, "BTCUSDT", 5.0);
        let read: Vec<_> = out.into_iter().filter(is_leverage_read).collect();
        assert_eq!(read.len(), 1, "read {command} is admitted and sent");
        reads.extend(read);
    }
    let out = fetch_leverage(&mut e, 1_256, "BTCUSDT", 5.0);
    assert!(
        !sends_io(&out),
        "the read beyond shared capacity must be refused before any HTTP"
    );
    let failed = terminals(&out, 1_256);
    assert_eq!(failed.len(), 1, "one typed capacity failure");
    let error = failed[0].as_ref().expect_err("capacity is a failure");
    let expected = Error::Capacity.venue_error();
    assert_eq!(error.message, expected.message);
    assert_eq!(error.kind, expected.kind);
    reply(
        &mut e,
        &reads[0],
        &envelope(r#"{"instId":"BTC-USDT-SWAP","mgnMode":"cross","posSide":"","lever":"25"}"#),
        6.0,
    );
    assert_eq!(terminals(&prepared(&mut e, 6.0), 1_000).len(), 1);
    let out = fetch_leverage(&mut e, 1_257, "BTCUSDT", 7.0);
    assert_eq!(
        out.iter().filter(|a| is_leverage_read(a)).count(),
        1,
        "an answered read frees capacity for the next"
    );
}

#[test]
fn fetch_leverage_entry_refusals_keep_text_without_http() {
    let mut e = trading_engine();
    let out = fetch_leverage(&mut e, 9, "NOPE", 5.0);
    assert!(!sends_io(&out));
    let failed = terminals(&out, 9);
    assert_eq!(failed.len(), 1);
    assert_eq!(
        failed[0].as_ref().unwrap_err().message,
        "unknown market NOPE"
    );

    let (mut e, _, _) = public(crate::support::FixtureOptions::default());
    let out = fetch_leverage(&mut e, 10, "BTCUSDT", 5.0);
    assert!(!sends_io(&out));
    let failed = terminals(&out, 10);
    assert_eq!(failed.len(), 1);
    assert_eq!(
        failed[0].as_ref().unwrap_err().message,
        "OKX leverage requires credentials"
    );
}

fn header<'a>(action: &'a Action, name: &str) -> Option<&'a str> {
    let Action::Http(r) = action else {
        return None;
    };
    r.headers
        .iter()
        .find(|(key, _)| *key == name)
        .map(|(_, value)| value.as_str())
}

#[test]
fn leverage_read_uses_the_read_key_and_writes_keep_the_primary() {
    let mut e = trading_engine_with(
        crate::support::FixtureOptions {
            key: Some("primary-key".into()),
            secret: Some("primary-secret".into()),
            passphrase: Some("primary-pass".into()),
            read_credentials: Some(safe_cex_engine::ReadCredentials {
                key: "read-key".into(),
                secret: "read-secret".into(),
                passphrase: Some("read-pass".into()),
            }),
            ..Default::default()
        },
        |_| {},
    );
    let out = fetch_leverage(&mut e, 7, "BTCUSDT", 5.0);
    let read = out
        .iter()
        .find(|a| is_leverage_read(a))
        .expect("leverage-info read");
    assert_eq!(header(read, "OK-ACCESS-KEY"), Some("read-key"));
    assert_eq!(header(read, "OK-ACCESS-PASSPHRASE"), Some("read-pass"));
    e.handle(
        6.0,
        Event::Command(Command::PlaceOrders {
            command: 8,
            orders: vec![order(
                r#"{"symbol":"BTCUSDT","type":"limit","side":"buy","amount":1,"price":38000}"#,
            )],
        }),
    );
    let out = prepared(&mut e, 6.0);
    let write = out
        .iter()
        .find(|a| matches!(a, Action::Http(r) if r.method == safe_cex_engine::Method::Post))
        .expect("placement write");
    assert_eq!(header(write, "OK-ACCESS-KEY"), Some("primary-key"));
    assert_eq!(header(write, "OK-ACCESS-PASSPHRASE"), Some("primary-pass"));
}

/// The catalogue is loaded and the private config read is still unanswered.
fn unready_engine() -> (safe_cex_engine::Engine<safe_cex_venue_okx::Okx>, Action) {
    let mut e = crate::support::engine(crate::support::FixtureOptions {
        key: Some("test-key".into()),
        secret: Some("test-secret".into()),
        passphrase: Some("test-passphrase".into()),
        ..Default::default()
    });
    e.start(0.0);
    let a = take_http(&mut prepared(&mut e, 0.0));
    reply(&mut e, &a, &envelope(MARKET), 1.0);
    let mut out = prepared(&mut e, 1.0);
    let a = take_http(&mut out);
    reply(&mut e, &a, &envelope(TICKER), 2.0);
    out.extend(prepared(&mut e, 2.0));
    let config = out
        .into_iter()
        .find(|a| is_get(a, "/api/v5/account/config"))
        .unwrap();
    (e, config)
}

fn finish_startup(
    e: &mut safe_cex_engine::Engine<safe_cex_venue_okx::Okx>,
    config: &Action,
    now: f64,
) {
    reply(
        e,
        config,
        &envelope(r#"{"posMode":"net_mode","acctLv":"2"}"#),
        now,
    );
    let mut pending: Vec<_> = prepared(e, now)
        .into_iter()
        .filter(|a| matches!(a, Action::Http(_)))
        .collect();
    while let Some(a) = pending.pop() {
        let body = if matches!(&a, Action::Http(r) if r.url.contains("account-position-risk")) {
            envelope(r#"{"balData":[{"ccy":"USDT","eq":"5000"}]}"#)
        } else {
            envelope("")
        };
        reply(e, &a, &body, now);
        pending.extend(
            prepared(e, now)
                .into_iter()
                .filter(|a| matches!(a, Action::Http(_))),
        );
    }
}

const LIMIT: &str = r#"{"symbol":"BTCUSDT","type":"limit","side":"buy","amount":1,"price":38000}"#;

/// A same-symbol placement still writes: no residual order lock or pending
/// placement survives the refusal.
fn assert_places(e: &mut safe_cex_engine::Engine<safe_cex_venue_okx::Okx>, command: u32, now: f64) {
    e.handle(
        now,
        Event::Command(Command::PlaceOrders {
            command,
            orders: vec![order(LIMIT)],
        }),
    );
    let out = prepared(e, now);
    assert!(
        out.iter().any(
            |a| matches!(a, Action::Http(r) if r.method == safe_cex_engine::Method::Post && r.url.contains("/api/v5/trade/"))
        ),
        "placement {command} writes after the refusal"
    );
    assert!(terminals(&out, command).is_empty());
}

fn refused(out: &[Action], command: u32, message: &str, kind: Option<safe_cex_engine::ErrorKind>) {
    assert!(!sends_io(out), "{message}: refused before any HTTP");
    let failed = terminals(out, command);
    assert_eq!(failed.len(), 1, "{message}: exactly one terminal");
    let error = failed[0].as_ref().expect_err(message);
    assert_eq!(error.message, message);
    assert_eq!(error.kind, kind, "{message}");
    assert_eq!(
        error.outcome,
        Some(safe_cex_engine::Outcome::NotSent),
        "{message}"
    );
}

#[test]
fn trade_entry_refusals_keep_text_kind_and_not_sent() {
    use safe_cex_engine::ErrorKind;
    // Unsupported: SetMarginType.
    let mut e = trading_engine();
    e.handle(
        5.0,
        Event::Command(Command::SetMarginType {
            command: 20,
            symbol: "BTCUSDT".into(),
            isolated: true,
        }),
    );
    let out = prepared(&mut e, 5.0);
    refused(
        &out,
        20,
        "Only cross mode is supported",
        Some(ErrorKind::Unsupported),
    );
    assert_places(&mut e, 21, 6.0);

    // Invalid input: a same-symbol placement rejected by planning.
    let mut e = trading_engine();
    e.handle(
        5.0,
        Event::Command(Command::PlaceOrders {
            command: 22,
            orders: vec![order(
                r#"{"symbol":"BTCUSDT","type":"limit","side":"buy","amount":1,"price":38000,"bybitSlippageToleranceTicks":2}"#,
            )],
        }),
    );
    let out = prepared(&mut e, 5.0);
    refused(
        &out,
        22,
        "Bybit execution controls are supported only on Bybit",
        Some(ErrorKind::InvalidInput),
    );
    assert_places(&mut e, 23, 6.0);

    // Not ready: the account config is still unanswered.
    let (mut e, config) = unready_engine();
    e.handle(
        3.0,
        Event::Command(Command::PlaceOrders {
            command: 24,
            orders: vec![order(LIMIT)],
        }),
    );
    let out = prepared(&mut e, 3.0);
    refused(&out, 24, "OKX account is not ready", None);
    finish_startup(&mut e, &config, 4.0);
    assert_places(&mut e, 25, 5.0);

    // Already running: a duplicate active ID never reaches the venue, so it
    // publishes no venue terminal and no write; the original keeps its lock.
    let mut e = trading_engine();
    e.handle(
        5.0,
        Event::Command(Command::PlaceOrders {
            command: 26,
            orders: vec![order(LIMIT)],
        }),
    );
    let first = prepared(&mut e, 5.0);
    let write = first
        .iter()
        .find(|a| matches!(a, Action::Http(r) if r.method == safe_cex_engine::Method::Post))
        .expect("first placement writes")
        .clone();
    for duplicate in [
        Command::PlaceOrders {
            command: 26,
            orders: vec![order(LIMIT)],
        },
        Command::SetMarginType {
            command: 26,
            symbol: "BTCUSDT".into(),
            isolated: true,
        },
    ] {
        e.handle(5.5, Event::Command(duplicate));
        let out = prepared(&mut e, 5.5);
        assert!(!sends_io(&out), "duplicate active ID sends nothing");
        assert!(
            terminals(&out, 26).is_empty(),
            "duplicate active ID cannot complete the original"
        );
    }
    reply(
        &mut e,
        &write,
        &envelope(r#"{"ordId":"accepted","clOrdId":"","sCode":"0","sMsg":""}"#),
        6.0,
    );
    assert_eq!(terminals(&prepared(&mut e, 6.0), 26).len(), 1);
}
