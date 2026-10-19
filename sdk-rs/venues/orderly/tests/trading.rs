mod support;
use safe_cex_engine::model::PlaceOrderOpts;
use safe_cex_engine::{Action, Command, Event, Notice};
use support::*;
fn place(e: &mut Driver, command: u32, json: &str) {
    let orders: Vec<PlaceOrderOpts> = serde_json::from_str(json).unwrap();
    e.handle(
        2000.0,
        Event::Command(Command::PlaceOrders { command, orders }),
    );
}
fn done(out: Vec<Action>, id: u32) -> Result<String, String> {
    out.into_iter()
        .find_map(|a| {
            if let Action::Notice(Notice::CommandDone { command, result }) = a {
                (command == id).then_some(result)
            } else {
                None
            }
        })
        .expect("command completion")
}
fn body(r: &safe_cex_engine::HttpRequest) -> serde_json::Value {
    serde_json::from_str(r.body.as_deref().unwrap()).unwrap()
}
const ALGO: &str = r#"[{"algo_order_id":100,"root_algo_order_id":100,"symbol":"PERP_BTC_USDC","algo_type":"POSITIONAL_TP_SL","algo_status":"NEW","margin_mode":"CROSS","child_orders":[{"algo_order_id":101,"root_algo_order_id":100,"algo_type":"STOP_LOSS","side":"SELL","algo_status":"NEW","trigger_price":49000,"quantity":0},{"algo_order_id":102,"root_algo_order_id":100,"algo_type":"TAKE_PROFIT","side":"SELL","algo_status":"NEW","trigger_price":55000,"quantity":0}]}]"#;

#[test]
fn caller_id_is_exact_and_documented_tif_is_native() {
    let (mut e, _) = ready();
    requests(&mut e);
    place(
        &mut e,
        1,
        r#"[{"symbol":"BTCUSDC","type":"limit","side":"buy","amount":1,"price":50000,"clientOrderId":"caller-unchanged","timeInForce":"PostOnly"}]"#,
    );
    let r = requests(&mut e).pop().unwrap();
    let payload = body(&r);
    assert_eq!(payload["client_order_id"], "caller-unchanged");
    assert_eq!(payload["order_type"], "POST_ONLY");
    assert_eq!(payload["order_tag"], "TEALSTREET");
    assert_eq!(payload["margin_mode"], "CROSS");
    reply(
        &mut e,
        &r,
        200,
        r#"{"success":true,"data":{"order_id":9007199254740995}}"#,
    );
    assert_eq!(done(actions(&mut e), 1).unwrap(), r#"["9007199254740995"]"#);
}
#[test]
fn full_batch_fails_before_any_write_for_invalid_later_id_and_bracket_source() {
    let (mut e, _) = ready();
    requests(&mut e);
    for json in [
        r#"[{"symbol":"BTCUSDC","type":"market","side":"buy","amount":1},{"symbol":"BTCUSDC","type":"limit","side":"buy","amount":1,"price":50000,"clientOrderId":"-bad"}]"#,
        r#"[{"symbol":"BTCUSDC","type":"market","side":"buy","amount":1,"stopLoss":49000,"slTriggerType":"last_price"}]"#,
        r#"[{"symbol":"BTCUSDC","type":"market","side":"buy","amount":1,"clientOrderId":"same"},{"symbol":"BTCUSDC","type":"market","side":"buy","amount":1,"clientOrderId":"same"}]"#,
    ] {
        place(&mut e, 10, json);
        let out = actions(&mut e);
        assert!(!out.iter().any(|a| matches!(a, Action::Http(_))));
        assert!(done(out, 10).is_err());
    }
}
#[test]
fn split_quantities_preserve_exact_amount_and_report_partial_acceptance() {
    let (mut e, _) = ready();
    requests(&mut e);
    place(
        &mut e,
        1,
        r#"[{"symbol":"BTCUSDC","type":"market","side":"buy","amount":41}]"#,
    );
    let r = requests(&mut e).pop().unwrap();
    assert_eq!(body(&r)["order_quantity"], 20.0);
    reply(
        &mut e,
        &r,
        200,
        r#"{"success":true,"data":{"order_id":41}}"#,
    );
    let r = requests(&mut e).pop().unwrap();
    assert_eq!(body(&r)["order_quantity"], 20.0);
    e.handle(
        2000.0,
        Event::HttpFailed {
            request: r.id,
            error: "response lost",
        },
    );
    let error = done(actions(&mut e), 1).unwrap_err();
    assert!(error.contains("partialOrderIds"));
    assert!(error.contains("41"));
    assert!(requests(&mut e).is_empty());
}
#[test]
fn brackets_follow_entry_and_accept_documented_root_id_shape() {
    let (mut e, s) = ready();
    requests(&mut e);
    place(
        &mut e,
        1,
        r#"[{"symbol":"BTCUSDC","type":"market","side":"buy","amount":1,"stopLoss":49000,"takeProfit":55000}]"#,
    );
    let r = requests(&mut e).pop().unwrap();
    assert!(r.url.ends_with("/v1/order"));
    reply(
        &mut e,
        &r,
        200,
        r#"{"success":true,"data":{"order_id":50}}"#,
    );
    let r = requests(&mut e).pop().unwrap();
    let b = body(&r);
    assert_eq!(b["child_orders"][0]["side"], "SELL");
    assert_eq!(b["child_orders"].as_array().unwrap().len(), 2);
    assert!(b.get("brokerId").is_some());
    reply(
        &mut e,
        &r,
        200,
        r#"{"success":true,"data":{"order_id":51}}"#,
    );
    ws(
        &mut e,
        s,
        r#"{"topic":"algoexecutionreportv2","ts":3000,"data":{"algoOrderId":510,"rootAlgoOrderId":51,"symbol":"PERP_BTC_USDC","algoType":"STOP_LOSS","algoStatus":"NEW","triggerPrice":49000,"side":"SELL","quantity":0,"seq":1}}"#,
    );
    assert!(
        !actions(&mut e)
            .iter()
            .any(|a| matches!(a, Action::Notice(Notice::CommandDone { command: 1, .. })))
    );
    ws(
        &mut e,
        s,
        r#"{"topic":"algoexecutionreportv2","ts":3001,"data":{"algoOrderId":511,"rootAlgoOrderId":51,"symbol":"PERP_BTC_USDC","algoType":"TAKE_PROFIT","algoStatus":"NEW","triggerPrice":55000,"side":"SELL","quantity":0,"seq":2}}"#,
    );
    assert_eq!(done(actions(&mut e), 1).unwrap(), r#"["50","51"]"#);
}
#[test]
fn replacing_one_stop_preserves_opposite_sibling_once_after_confirmed_cancel() {
    let (mut e, s) = ready_with(ORDER, ALGO, POSITIONS);
    requests(&mut e);
    place(
        &mut e,
        1,
        r#"[{"symbol":"BTCUSDC","type":"stop_market","side":"sell","amount":1,"price":48000}]"#,
    );
    let r = requests(&mut e).pop().unwrap();
    assert_eq!(r.method, safe_cex_engine::Method::Delete);
    assert!(r.url.contains("/v1/algo/order?order_id=100"));
    reply(
        &mut e,
        &r,
        200,
        r#"{"success":true,"data":{"status":"CANCEL_SENT"}}"#,
    );
    assert!(
        !requests(&mut e)
            .iter()
            .any(|r| r.method == safe_cex_engine::Method::Post)
    );
    ws(
        &mut e,
        s,
        r#"{"topic":"algoexecutionreportv2","ts":3000,"data":{"algoOrderId":100,"rootAlgoOrderId":100,"symbol":"PERP_BTC_USDC","algoType":"POSITIONAL_TP_SL","algoStatus":"CANCELLED","seq":5}}"#,
    );
    let writes: Vec<_> = requests(&mut e)
        .into_iter()
        .filter(|r| r.method == safe_cex_engine::Method::Post)
        .collect();
    assert_eq!(writes.len(), 1);
    let b = body(&writes[0]);
    assert_eq!(b["child_orders"][0]["trigger_price"], "48000");
    assert_eq!(b["child_orders"][1]["trigger_price"], "55000");
}
#[test]
fn cancelling_both_children_sends_one_parent_delete_and_never_repairs() {
    let (mut e, _) = ready_with(ORDER, ALGO, POSITIONS);
    requests(&mut e);
    e.handle(
        2000.0,
        Event::Command(Command::CancelOrders {
            command: 1,
            ids: vec!["101".into(), "102".into()],
        }),
    );
    let all = requests(&mut e);
    assert_eq!(all.len(), 1);
    reply(
        &mut e,
        &all[0],
        200,
        r#"{"success":true,"data":{"status":"CANCEL_SENT"}}"#,
    );
    assert!(done(actions(&mut e), 1).is_ok());
    assert!(requests(&mut e).is_empty());
}
#[test]
fn bulk_cancel_attempts_both_families_after_a_failure() {
    let (mut e, _) = ready_with(ORDER, ALGO, POSITIONS);
    requests(&mut e);
    e.handle(
        2000.0,
        Event::Command(Command::CancelSymbolOrders {
            command: 1,
            symbol: "BTCUSDC".into(),
        }),
    );
    let r = requests(&mut e).pop().unwrap();
    assert!(r.url.contains("/v1/orders?"));
    reply(
        &mut e,
        &r,
        503,
        r#"{"success":false,"message":"unavailable"}"#,
    );
    let r = requests(&mut e).pop().unwrap();
    assert!(r.url.contains("/v1/algo/orders?"));
    reply(
        &mut e,
        &r,
        200,
        r#"{"success":true,"data":{"status":"CANCEL_ALL_SENT"}}"#,
    );
    assert!(done(actions(&mut e), 1).is_err());
}
#[test]
fn strict_native_amend_preserves_identity_and_never_replaces() {
    let (mut e, _) = ready();
    requests(&mut e);
    e.handle(
        2000.0,
        Event::Command(Command::AmendOrder {
            command: 1,
            id: "9007199254740993".into(),
            price: Some(50001.0),
            amount: None,
        }),
    );
    let r = requests(&mut e).pop().unwrap();
    assert_eq!(r.method, safe_cex_engine::Method::Put);
    let b = body(&r);
    assert_eq!(b["order_id"], "9007199254740993");
    assert_eq!(b["order_price"], 50001.0);
    assert!(b.get("client_order_id").is_none());
    reply(
        &mut e,
        &r,
        200,
        r#"{"success":true,"data":{"status":"EDIT_SENT"}}"#,
    );
    assert_eq!(done(actions(&mut e), 1).unwrap(), r#"["9007199254740993"]"#);
    e.handle(
        2000.0,
        Event::Command(Command::AmendOrder {
            command: 2,
            id: "9007199254740993".into(),
            price: Some(50002.0),
            amount: None,
        }),
    );
    assert!(done(actions(&mut e), 2).is_err());
}
#[test]
fn margin_selector_keeps_existing_positions_and_targets_symbol_leverage() {
    let (mut e, _) = ready();
    requests(&mut e);
    e.handle(
        2000.0,
        Event::Command(Command::SetMarginType {
            command: 1,
            symbol: "BTCUSDC".into(),
            isolated: true,
        }),
    );
    let r = requests(&mut e).pop().unwrap();
    reply(&mut e, &r, 200, r#"{"success":true}"#);
    assert!(done(actions(&mut e), 1).is_ok());
    assert_eq!(e.store().positions().len(), 2);
    e.handle(
        2000.0,
        Event::Command(Command::SetLeverage {
            command: 2,
            symbol: "BTCUSDC".into(),
            leverage: 1000.0,
        }),
    );
    let r = requests(&mut e).pop().unwrap();
    assert_eq!(body(&r)["margin_mode"], "ISOLATED");
    assert_eq!(body(&r)["leverage"], 100.0);
    reply(&mut e, &r, 200, r#"{"success":true}"#);
    assert!(done(actions(&mut e), 2).is_ok());
    assert_eq!(
        e.store()
            .positions()
            .iter()
            .find(|p| p.id == "BTCUSDC")
            .unwrap()
            .leverage,
        10.0
    );
    assert_eq!(
        e.store()
            .positions()
            .iter()
            .find(|p| p.id == "BTCUSDC:isolated")
            .unwrap()
            .leverage,
        100.0
    );
}

#[test]
fn queued_second_stop_waits_for_first_pair_then_cancels_it_before_replacement() {
    let (mut e, s) = ready();
    requests(&mut e);
    place(
        &mut e,
        1,
        r#"[{"symbol":"BTCUSDC","type":"stop_market","side":"sell","amount":1,"price":49000}]"#,
    );
    let first = requests(&mut e).pop().unwrap();
    place(
        &mut e,
        2,
        r#"[{"symbol":"BTCUSDC","type":"stop_market","side":"sell","amount":1,"price":48000}]"#,
    );
    assert!(requests(&mut e).is_empty());
    reply(
        &mut e,
        &first,
        200,
        r#"{"success":true,"data":{"order_id":100}}"#,
    );
    assert!(
        !requests(&mut e)
            .iter()
            .any(|r| r.method == safe_cex_engine::Method::Post)
    );
    ws(
        &mut e,
        s,
        r#"{"topic":"algoexecutionreportv2","ts":3000,"data":{"algoOrderId":101,"rootAlgoOrderId":100,"symbol":"PERP_BTC_USDC","algoType":"STOP_LOSS","algoStatus":"NEW","side":"SELL","triggerPrice":49000,"quantity":0,"seq":1}}"#,
    );
    let out = actions(&mut e);
    assert!(out.iter().any(|a| matches!(
        a,
        Action::Notice(Notice::CommandDone {
            command: 1,
            result: Ok(_)
        })
    )));
    let writes: Vec<_> = out
        .into_iter()
        .filter_map(|a| {
            if let Action::Http(r) = a {
                Some(r)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(writes.len(), 1);
    assert_eq!(writes[0].method, safe_cex_engine::Method::Delete);
    assert!(writes[0].url.contains("order_id=100"));
}

#[test]
fn duplicate_bracket_scopes_and_ambiguous_id_lengths_reject_before_writes() {
    let (mut e, _) = ready();
    requests(&mut e);
    place(
        &mut e,
        1,
        r#"[{"symbol":"BTCUSDC","type":"market","side":"buy","amount":1,"stopLoss":49000},{"symbol":"BTCUSDC","type":"market","side":"buy","amount":1,"takeProfit":55000}]"#,
    );
    let out = actions(&mut e);
    assert!(!out.iter().any(|a| matches!(a, Action::Http(_))));
    assert!(done(out, 1).is_err());
    place(
        &mut e,
        2,
        &format!(
            r#"[{{"symbol":"BTCUSDC","type":"market","side":"buy","amount":1,"clientOrderId":"{}"}}]"#,
            "😀".repeat(19)
        ),
    );
    let out = actions(&mut e);
    assert!(!out.iter().any(|a| matches!(a, Action::Http(_))));
    assert!(done(out, 2).is_err());
}

#[test]
fn global_cross_leverage_is_clamped_for_each_symbol() {
    let (mut e, _) = ready();
    requests(&mut e);
    e.handle(
        2000.0,
        Event::Command(Command::SetLeverage {
            command: 1,
            symbol: String::new(),
            leverage: 100.0,
        }),
    );
    let r = requests(&mut e).pop().unwrap();
    reply(&mut e, &r, 200, r#"{"success":true}"#);
    assert!(done(actions(&mut e), 1).is_ok());
    assert_eq!(e.store().setting("BTCUSDC").unwrap().leverage, 100.0);
    assert_eq!(e.store().setting("ETHUSDC").unwrap().leverage, 20.0);
    e.handle(
        2000.0,
        Event::Command(Command::SetMarginType {
            command: 2,
            symbol: "ETHUSDC".into(),
            isolated: false,
        }),
    );
    let r = requests(&mut e).pop().unwrap();
    reply(&mut e, &r, 200, r#"{"success":true}"#);
    assert!(done(actions(&mut e), 2).is_ok());
    assert_eq!(e.store().setting("ETHUSDC").unwrap().leverage, 50.0);
}

#[test]
fn bulk_cancel_preempts_unconfirmed_pair_and_discards_queued_placements() {
    let (mut e, _) = ready();
    requests(&mut e);
    place(
        &mut e,
        1,
        r#"[{"symbol":"BTCUSDC","type":"stop_market","side":"sell","amount":1,"price":49000}]"#,
    );
    let r = requests(&mut e).pop().unwrap();
    reply(
        &mut e,
        &r,
        200,
        r#"{"success":true,"data":{"order_id":100}}"#,
    );
    actions(&mut e);
    place(
        &mut e,
        2,
        r#"[{"symbol":"BTCUSDC","type":"market","side":"buy","amount":1}]"#,
    );
    e.handle(
        2000.0,
        Event::Command(Command::CancelSymbolOrders {
            command: 3,
            symbol: "BTCUSDC".into(),
        }),
    );
    let out = actions(&mut e);
    assert!(out.iter().any(|a| matches!(
        a,
        Action::Notice(Notice::CommandDone {
            command: 2,
            result: Err(_)
        })
    )));
    let writes: Vec<_> = out
        .into_iter()
        .filter_map(|a| {
            if let Action::Http(r) = a {
                Some(r)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(writes.len(), 1);
    assert_eq!(writes[0].method, safe_cex_engine::Method::Delete);
    reply(
        &mut e,
        &writes[0],
        200,
        r#"{"success":true,"data":{"status":"CANCEL_ALL_SENT"}}"#,
    );
    let r = requests(&mut e).pop().unwrap();
    reply(
        &mut e,
        &r,
        200,
        r#"{"success":true,"data":{"status":"CANCEL_ALL_SENT"}}"#,
    );
    assert!(done(actions(&mut e), 3).is_ok());
    place(
        &mut e,
        4,
        r#"[{"symbol":"BTCUSDC","type":"stop_market","side":"sell","amount":1,"price":48000}]"#,
    );
    let out = actions(&mut e);
    assert!(!out.iter().any(|a| matches!(a, Action::Http(_))));
    assert!(done(out, 4).is_err());
}

#[test]
fn delayed_placement_ack_cannot_resurrect_a_terminal_order_after_snapshot_pruning() {
    let (mut e, s) = ready();
    for r in requests(&mut e) {
        reply(&mut e, &r, 503, r#"{"success":false,"message":"busy"}"#);
    }
    actions(&mut e);
    place(
        &mut e,
        1,
        r#"[{"symbol":"BTCUSDC","type":"limit","side":"buy","amount":1,"price":50000}]"#,
    );
    let placed = requests(&mut e).pop().unwrap();
    ws(
        &mut e,
        s,
        r#"{"topic":"executionreport","ts":3000,"data":{"orderId":555,"symbol":"PERP_BTC_USDC","type":"LIMIT","side":"BUY","status":"FILLED","price":50000,"quantity":1,"totalExecutedQuantity":1,"executedQuantity":1,"executedPrice":50000,"tradeId":555,"seq":1}}"#,
    );
    e.handle(
        3000.0,
        Event::WsClosed {
            socket: s,
            code: 1006,
        },
    );
    let timer = actions(&mut e)
        .into_iter()
        .find_map(|a| {
            if let Action::SetTimer { timer, .. } = a {
                Some(timer)
            } else {
                None
            }
        })
        .unwrap();
    e.handle(4000.0, Event::TimerFired { timer });
    let socket = actions(&mut e)
        .into_iter()
        .find_map(|a| {
            if let Action::WsConnect { socket, .. } = a {
                Some(socket)
            } else {
                None
            }
        })
        .unwrap();
    e.handle(4000.0, Event::WsOpened { socket });
    actions(&mut e);
    ws(&mut e, socket, r#"{"event":"auth","success":true}"#);
    for r in requests(&mut e) {
        if r.url.contains("/v1/orders?") {
            reply(
                &mut e,
                &r,
                200,
                r#"{"success":true,"data":{"rows":[],"meta":{"total":0}}}"#,
            );
        }
    }
    assert!(!e.store().orders().iter().any(|o| o.id == "555"));
    reply(
        &mut e,
        &placed,
        200,
        r#"{"success":true,"data":{"order_id":555}}"#,
    );
    assert!(!e.store().orders().iter().any(|o| o.id == "555"));
    assert!(done(actions(&mut e), 1).is_ok());
}

#[test]
fn lost_pair_response_blocks_queued_protection_until_fresh_reconciliation() {
    let (mut e, socket) = ready();
    let held = requests(&mut e);
    let stop =
        r#"[{"symbol":"BTCUSDC","type":"stop_market","side":"sell","amount":1,"price":49000}]"#;
    place(&mut e, 1, stop);
    let r = requests(&mut e).pop().unwrap();
    place(&mut e, 2, stop);
    e.handle(
        2000.0,
        Event::HttpFailed {
            request: r.id,
            error: "response lost",
        },
    );
    let out = actions(&mut e);
    assert!(
        !out.iter()
            .any(|a| matches!(a, Action::Http(r) if r.method == safe_cex_engine::Method::Post))
    );
    assert!(done(out.clone(), 1).is_err());
    assert!(done(out, 2).is_err());
    place(&mut e, 3, stop);
    let out = actions(&mut e);
    assert!(!out.iter().any(|a| matches!(a, Action::Http(_))));
    assert!(done(out, 3).is_err());
    // The held pre-write read is successful but cannot establish recovery.
    for r in held {
        reply(
            &mut e,
            &r,
            200,
            r#"{"success":true,"data":{"rows":[],"meta":{"total":0}}}"#,
        );
    }
    actions(&mut e);
    place(&mut e, 4, stop);
    assert!(done(actions(&mut e), 4).is_err());
    e.handle(3000.0, Event::WsClosed { socket, code: 1006 });
    let timer = actions(&mut e)
        .into_iter()
        .find_map(|a| {
            if let Action::SetTimer { timer, .. } = a {
                Some(timer)
            } else {
                None
            }
        })
        .unwrap();
    e.handle(4000.0, Event::TimerFired { timer });
    let socket = actions(&mut e)
        .into_iter()
        .find_map(|a| {
            if let Action::WsConnect { socket, .. } = a {
                Some(socket)
            } else {
                None
            }
        })
        .unwrap();
    e.handle(4000.0, Event::WsOpened { socket });
    actions(&mut e);
    ws(&mut e, socket, r#"{"event":"auth","success":true}"#);
    for r in requests(&mut e) {
        if r.url.contains("/v1/algo/orders?") {
            reply(
                &mut e,
                &r,
                200,
                r#"{"success":true,"data":{"rows":[],"meta":{"total":0}}}"#,
            );
        }
    }
    actions(&mut e);
    place(&mut e, 5, stop);
    assert!(
        requests(&mut e)
            .iter()
            .any(|r| r.method == safe_cex_engine::Method::Post)
    );
}

#[test]
fn bulk_cancel_during_pair_post_does_not_wait_for_pair_children() {
    let (mut e, _) = ready();
    requests(&mut e);
    place(
        &mut e,
        1,
        r#"[{"symbol":"BTCUSDC","type":"stop_market","side":"sell","amount":1,"price":49000}]"#,
    );
    let pair = requests(&mut e).pop().unwrap();
    e.handle(
        2000.0,
        Event::Command(Command::CancelSymbolOrders {
            command: 2,
            symbol: "BTCUSDC".into(),
        }),
    );
    assert!(requests(&mut e).is_empty());
    reply(
        &mut e,
        &pair,
        200,
        r#"{"success":true,"data":{"order_id":100}}"#,
    );
    let out = actions(&mut e);
    assert!(
        out.iter()
            .any(|a| matches!(a, Action::Http(r) if r.method == safe_cex_engine::Method::Delete))
    );
    assert!(done(out, 1).is_err());
}
