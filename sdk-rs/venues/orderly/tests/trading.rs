mod support;
use safe_cex_engine::model::{BybitBbo, PlaceOrderOpts};
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
        .find_map(|a| match a {
            Action::Notice(Notice::CommandDone { command, result }) => {
                (command == id).then_some(Ok(result))
            }
            Action::Notice(Notice::CommandFailed { command, error }) => {
                (command == id).then(|| Err(error.legacy_text()))
            }
            _ => None,
        })
        .expect("command completion")
}

fn body(r: &safe_cex_engine::HttpRequest) -> serde_json::Value {
    serde_json::from_str(r.body.as_deref().unwrap()).unwrap()
}
const ALGO: &str = r#"[{"algo_order_id":100,"root_algo_order_id":100,"symbol":"PERP_BTC_USDC","algo_type":"POSITIONAL_TP_SL","algo_status":"NEW","margin_mode":"CROSS","child_orders":[{"algo_order_id":101,"root_algo_order_id":100,"algo_type":"STOP_LOSS","side":"SELL","algo_status":"NEW","trigger_price":49000,"type":"CLOSE_POSITION","reduce_only":true,"quantity":0},{"algo_order_id":102,"root_algo_order_id":100,"algo_type":"TAKE_PROFIT","side":"SELL","algo_status":"NEW","trigger_price":55000,"type":"CLOSE_POSITION","reduce_only":true,"quantity":0}]}]"#;

#[test]
fn receipts_report_actual_caller_id_before_write_and_preserve_absence() {
    for supplied in [false, true] {
        let (mut e, _) = ready();
        actions(&mut e);
        let mut order: PlaceOrderOpts = serde_json::from_str(
            r#"{"symbol":"BTCUSDC","type":"limit","side":"buy","amount":1,"price":50000}"#,
        )
        .unwrap();
        order.client_order_id = supplied.then(|| "caller-opaque-9007199254740995".into());
        e.handle(
            2000.0,
            Event::Command(Command::PlaceOrders {
                command: 71,
                orders: vec![order],
            }),
        );
        let out = actions(&mut e);
        let http = out
            .iter()
            .position(|a| matches!(a, Action::Http(_)))
            .unwrap();
        let receipts: Vec<_> = out
            .iter()
            .enumerate()
            .filter_map(|(i, action)| {
                if let Action::Notice(Notice::CommandProgress {
                    command: 71,
                    receipts,
                }) = action
                {
                    assert!(i < http);
                    Some(receipts)
                } else {
                    None
                }
            })
            .flatten()
            .collect();
        let Action::Http(request) = &out[http] else {
            unreachable!()
        };
        let payload = body(request);
        if supplied {
            assert_eq!(receipts.len(), 1);
            assert_eq!(receipts[0].symbol, "BTCUSDC");
            assert_eq!(
                receipts[0].kind,
                safe_cex_engine::OrderIdentityKind::ClientOrderId
            );
            assert_eq!(payload["client_order_id"], receipts[0].identity);
        } else {
            assert!(receipts.is_empty());
            assert!(payload.get("client_order_id").is_none());
        }
    }
}

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

fn rejects_bybit_control_without_batch_effects(bad: PlaceOrderOpts) {
    for busy in [false, true] {
        let (mut e, _) = ready();
        requests(&mut e);
        let good: PlaceOrderOpts = serde_json::from_str(
            r#"{"symbol":"BTCUSDC","type":"limit","side":"buy","amount":1,"price":50000}"#,
        )
        .unwrap();
        if busy {
            e.handle(
                2000.0,
                Event::Command(Command::PlaceOrders {
                    command: 1,
                    orders: vec![good.clone()],
                }),
            );
            assert_eq!(
                requests(&mut e).len(),
                1,
                "valid order must reach native HTTP"
            );
        }
        let before = e.snapshot_frame(2000.0).to_vec();
        e.take_frame(2000.0);
        e.handle(
            2000.0,
            Event::Command(Command::PlaceOrders {
                command: 10,
                orders: vec![good, bad.clone()],
            }),
        );
        let out = actions(&mut e);
        assert!(
            out.iter().all(|action| matches!(
                action,
                Action::Notice(Notice::CommandFailed { command: 10, error })
                    if error.message.contains("Bybit BBO and TickSize controls")
                        && error.outcome == Some(safe_cex_engine::Outcome::NotSent)
            )),
            "unexpected batch effects (busy={busy}): {out:?}"
        );
        assert_eq!(
            out.len(),
            1,
            "batch must reject immediately and exactly once"
        );
        assert_eq!(e.snapshot_frame(2000.0), before);
        assert!(
            e.take_frame(2000.0).is_none(),
            "rejection dirtied the store"
        );
    }
}

#[test]
fn bybit_bbo_on_later_batch_item_rejects_before_any_read_write_or_mutation() {
    let mut bad: PlaceOrderOpts = serde_json::from_str(
        r#"{"symbol":"BTCUSDC","type":"limit","side":"buy","amount":1,"price":50000}"#,
    )
    .unwrap();
    bad.bybit_bbo = Some(BybitBbo {
        side_type: "Queue".into(),
        level: 1.0,
    });
    rejects_bybit_control_without_batch_effects(bad);
}

#[test]
fn bybit_ticks_on_later_batch_item_reject_before_any_read_write_or_mutation() {
    for ticks in [0.0, 1.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let mut bad: PlaceOrderOpts = serde_json::from_str(
            r#"{"symbol":"BTCUSDC","type":"limit","side":"buy","amount":1,"price":50000}"#,
        )
        .unwrap();
        bad.bybit_slippage_tolerance_ticks = Some(ticks);
        rejects_bybit_control_without_batch_effects(bad);
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
    assert!(
        requests(&mut e)
            .iter()
            .all(|r| r.method == safe_cex_engine::Method::Get)
    );
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
    let out = actions(&mut e);
    assert!(out.iter().any(|a| matches!(a, Action::Http(r) if r.method == safe_cex_engine::Method::Get && r.url.contains("orders?status=INCOMPLETE"))));
    let r = out
        .into_iter()
        .find_map(|a| match a {
            Action::Http(r) if r.method == safe_cex_engine::Method::Delete => Some(r),
            _ => None,
        })
        .unwrap();
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
            side: None,
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
            result: _
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
            side: None,
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
    assert!(done(out.clone(), 2).is_err());
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

#[test]
fn finite_and_limit_protective_siblings_never_become_full_position_repairs() {
    for algos in [
        ALGO.replace("POSITIONAL_TP_SL", "TP_SL")
            .replace("CLOSE_POSITION", "MARKET")
            .replace("\"quantity\":0", "\"quantity\":0.1"),
        ALGO.replace("CLOSE_POSITION", "LIMIT")
            .replace("\"quantity\":0", "\"price\":48500,\"quantity\":0"),
    ] {
        let (mut e, _) = ready_with(ORDER, &algos, POSITIONS);
        assert!(e.store().loaded().orders);
        requests(&mut e);
        e.handle(
            2000.0,
            Event::Command(Command::CancelOrders {
                command: 1,
                ids: vec!["101".into()],
            }),
        );
        let out = actions(&mut e);
        assert!(
            !out.iter().any(|a| matches!(a, Action::Http(_))),
            "unsafe sibling repair sent a write"
        );
        assert!(done(out, 1).is_err());
    }
}

#[test]
fn bulk_cancel_supersedes_queued_child_cancellation_and_amendment() {
    let (mut e, _) = ready_with(ORDER, ALGO, POSITIONS);
    requests(&mut e);
    place(
        &mut e,
        1,
        r#"[{"symbol":"BTCUSDC","type":"market","side":"buy","amount":1}]"#,
    );
    let pending = requests(&mut e).pop().unwrap();
    e.handle(
        2000.0,
        Event::Command(Command::CancelOrders {
            command: 2,
            ids: vec!["101".into()],
        }),
    );
    e.handle(
        2000.0,
        Event::Command(Command::AmendOrder {
            command: 3,
            id: "102".into(),
            price: Some(56000.0),
            amount: None,
        }),
    );
    e.handle(
        2000.0,
        Event::Command(Command::CancelSymbolOrders {
            command: 4,
            symbol: "BTCUSDC".into(),
        }),
    );
    let out = actions(&mut e);
    for command in [2, 3] {
        assert!(
            done(out.clone(), command).is_err(),
            "queued order mutation was not superseded"
        );
        // Superseded before it was sent.
        assert_eq!(
            outcome(out.clone(), command),
            Some(Some(safe_cex_engine::Outcome::NotSent))
        );
    }
    reply(
        &mut e,
        &pending,
        200,
        r#"{"success":true,"data":{"order_id":400}}"#,
    );
    let normal = requests(&mut e).pop().unwrap();
    reply(
        &mut e,
        &normal,
        200,
        r#"{"success":true,"data":{"status":"CANCEL_ALL_SENT"}}"#,
    );
    let algo = requests(&mut e).pop().unwrap();
    reply(
        &mut e,
        &algo,
        200,
        r#"{"success":true,"data":{"status":"CANCEL_ALL_SENT"}}"#,
    );
    let out = actions(&mut e);
    assert!(done(out.clone(), 4).is_ok());
    assert!(
        !out.iter().any(|a| matches!(a, Action::Http(_))),
        "queued mutation wrote after bulk ACK"
    );
}

#[test]
fn finite_algo_whole_root_cancellation_remains_available_without_repair() {
    let algos = ALGO
        .replace("POSITIONAL_TP_SL", "TP_SL")
        .replace("CLOSE_POSITION", "MARKET")
        .replace("\"quantity\":0", "\"quantity\":0.1");
    let (mut e, _) = ready_with(ORDER, &algos, POSITIONS);
    requests(&mut e);
    e.handle(
        2000.0,
        Event::Command(Command::CancelOrders {
            command: 1,
            ids: vec!["100".into()],
        }),
    );
    let r = requests(&mut e)
        .pop()
        .expect("whole native root cancellation");
    assert_eq!(r.method, safe_cex_engine::Method::Delete);
    assert!(r.url.contains("/v1/algo/order?order_id=100&"));
    reply(
        &mut e,
        &r,
        200,
        r#"{"success":true,"data":{"status":"CANCEL_SENT"}}"#,
    );
    assert!(done(actions(&mut e), 1).is_ok());
    assert!(requests(&mut e).is_empty());
}

#[test]
fn bulk_cancel_keeps_queued_cancellation_in_an_independent_symbol() {
    let algos = ALGO
        .replace("BTC_USDC", "ETH_USDC")
        .replace("CROSS", "ISOLATED");
    let (mut e, _) = ready_with(ORDER, &algos, POSITIONS);
    requests(&mut e);
    place(
        &mut e,
        1,
        r#"[{"symbol":"BTCUSDC","type":"market","side":"buy","amount":1}]"#,
    );
    let pending = requests(&mut e).pop().unwrap();
    e.handle(
        2000.0,
        Event::Command(Command::CancelOrders {
            command: 2,
            ids: vec!["100".into()],
        }),
    );
    e.handle(
        2000.0,
        Event::Command(Command::CancelSymbolOrders {
            command: 3,
            symbol: "BTCUSDC".into(),
        }),
    );
    assert!(
        !actions(&mut e)
            .iter()
            .any(|a| matches!(a, Action::Notice(Notice::CommandDone { command: 2, .. })))
    );
    reply(
        &mut e,
        &pending,
        200,
        r#"{"success":true,"data":{"order_id":400}}"#,
    );
    for _ in 0..2 {
        let r = requests(&mut e).pop().unwrap();
        assert!(r.url.contains("symbol=PERP_BTC_USDC"));
        reply(
            &mut e,
            &r,
            200,
            r#"{"success":true,"data":{"status":"CANCEL_ALL_SENT"}}"#,
        );
    }
    let out = actions(&mut e);
    assert!(done(out.clone(), 3).is_ok());
    assert!(out.iter().any(
        |a| matches!(a, Action::Http(r) if r.url.contains("order_id=100&symbol=PERP_ETH_USDC"))
    ));
}

#[test]
fn rest_proven_positional_sibling_keeps_provenance_on_a_partial_stream_update() {
    let (mut e, socket) = ready_with(ORDER, ALGO, POSITIONS);
    requests(&mut e);
    ws(
        &mut e,
        socket,
        r#"{"topic":"algoexecutionreportv2","ts":3000,"data":{"algoOrderId":102,"rootAlgoOrderId":100,"symbol":"PERP_BTC_USDC","algoType":"TAKE_PROFIT","algoStatus":"NEW","side":"SELL","triggerPrice":56000,"quantity":0,"seq":1}}"#,
    );
    actions(&mut e);
    e.handle(
        3001.0,
        Event::Command(Command::CancelOrders {
            command: 1,
            ids: vec!["101".into()],
        }),
    );
    let r = requests(&mut e)
        .pop()
        .expect("same verified child/root remains repairable");
    assert!(r.url.contains("/v1/algo/order?order_id=100&"));
}

#[test]
fn child_before_pair_ack_gets_typed_creation_provenance_without_trusting_contradictions() {
    for wire_type in ["", ",\"type\":\"LIMIT\""] {
        let (mut e, socket) = ready();
        requests(&mut e);
        place(
            &mut e,
            1,
            r#"[{"symbol":"BTCUSDC","type":"stop_market","side":"sell","amount":1,"price":49000}]"#,
        );
        let pair = requests(&mut e).pop().unwrap();
        place(
            &mut e,
            2,
            r#"[{"symbol":"BTCUSDC","type":"stop_market","side":"sell","amount":1,"price":48000}]"#,
        );
        ws(
            &mut e,
            socket,
            &format!(
                r#"{{"topic":"algoexecutionreportv2","ts":3000,"data":{{"algoOrderId":101,"rootAlgoOrderId":100,"symbol":"PERP_BTC_USDC","algoType":"STOP_LOSS","algoStatus":"NEW","side":"SELL","triggerPrice":49000,"quantity":0,"seq":1{wire_type}}}}}"#
            ),
        );
        actions(&mut e);
        reply(
            &mut e,
            &pair,
            200,
            r#"{"success":true,"data":{"order_id":100}}"#,
        );
        let out = actions(&mut e);
        assert!(done(out.clone(), 1).is_ok());
        if wire_type.is_empty() {
            assert!(out.iter().any(|a| matches!(a, Action::Http(r) if r.method == safe_cex_engine::Method::Delete && r.url.contains("order_id=100"))));
        } else {
            assert!(done(out.clone(), 2).is_err());
            assert!(!out.iter().any(|a| matches!(a, Action::Http(_))));
        }
    }
}

#[test]
fn whole_bracket_root_cancels_live_exits_after_the_entry_fills() {
    let exits = ALGO
        .trim_start_matches('[')
        .trim_end_matches(']')
        .replace("\"root_algo_order_id\":100", "\"root_algo_order_id\":200");
    let bracket = format!(
        r#"[{{"algo_order_id":200,"root_algo_order_id":200,"symbol":"PERP_BTC_USDC","algo_type":"BRACKET","algo_status":"FILLED","side":"BUY","type":"LIMIT","price":50000,"quantity":1,"margin_mode":"CROSS","child_orders":[{exits}]}}]"#
    );
    let (mut e, _) = ready_with(ORDER, &bracket, POSITIONS);
    assert!(e.store().loaded().orders);
    let entry = e.store().orders().iter().find(|o| o.id == "200").unwrap();
    assert_eq!(entry.status, safe_cex_engine::model::OrderStatus::Closed);
    assert!(
        e.store()
            .orders()
            .iter()
            .filter(|o| ["101", "102"].contains(&o.id.as_str()))
            .all(|o| o.status == safe_cex_engine::model::OrderStatus::Open)
    );
    requests(&mut e);
    e.handle(
        3000.0,
        Event::Command(Command::CancelOrders {
            command: 1,
            ids: vec!["200".into()],
        }),
    );
    let r = requests(&mut e)
        .pop()
        .expect("live exits below filled bracket entry");
    assert!(r.url.contains("/v1/algo/order?order_id=200&"));
    assert!(
        e.store()
            .orders()
            .iter()
            .filter(|o| ["101", "102"].contains(&o.id.as_str()))
            .all(|o| o.status == safe_cex_engine::model::OrderStatus::Canceling)
    );
}

#[test]
fn omitted_stream_fields_do_not_erase_prior_type_or_margin_contradictions() {
    for contrary in [
        r#", "type":"LIMIT""#,
        r#", "type":"CLOSE_POSITION", "marginMode":"ISOLATED""#,
    ] {
        let (mut e, socket) = ready();
        requests(&mut e);
        place(
            &mut e,
            1,
            r#"[{"symbol":"BTCUSDC","type":"stop_market","side":"sell","amount":1,"price":49000}]"#,
        );
        let pair = requests(&mut e).pop().unwrap();
        reply(
            &mut e,
            &pair,
            200,
            r#"{"success":true,"data":{"order_id":100}}"#,
        );
        requests(&mut e);
        for (seq, fields) in [(1, contrary), (2, "")] {
            ws(
                &mut e,
                socket,
                &format!(
                    r#"{{"topic":"algoexecutionreportv2","ts":3000,"data":{{"algoOrderId":101,"rootAlgoOrderId":100,"symbol":"PERP_BTC_USDC","algoType":"STOP_LOSS","algoStatus":"NEW","side":"SELL","triggerPrice":49000,"quantity":0,"seq":{seq}{fields}}}}}"#
                ),
            );
            actions(&mut e);
        }
        place(
            &mut e,
            2,
            r#"[{"symbol":"BTCUSDC","type":"stop_market","side":"sell","amount":1,"price":48000}]"#,
        );
        let out = actions(&mut e);
        assert!(done(out.clone(), 2).is_err());
        assert!(
            !out.iter().any(|a| matches!(a, Action::Http(_))),
            "missing fields erased contradictory native evidence"
        );
    }
}

#[test]
fn lost_place_cancel_and_amend_preserve_optimistic_state_and_reconcile_without_replay() {
    use safe_cex_engine::model::OrderStatus;
    for kind in [0, 1, 2] {
        let (mut e, _) = ready();
        requests(&mut e);
        if kind == 0 {
            place(
                &mut e,
                71,
                r#"[{"symbol":"BTCUSDC","type":"limit","side":"buy","amount":1,"price":50000}]"#,
            );
        } else {
            e.handle(
                2000.0,
                Event::Command(if kind == 1 {
                    Command::CancelOrders {
                        command: 71,
                        ids: vec!["9007199254740993".into()],
                    }
                } else {
                    Command::AmendOrder {
                        command: 71,
                        id: "9007199254740993".into(),
                        price: Some(50001.0),
                        amount: None,
                    }
                }),
            );
        }
        let request = requests(&mut e).pop().unwrap();
        let before = e.store().orders().to_vec();
        e.handle(
            2001.0,
            Event::HttpFailed {
                request: request.id,
                error: "timeout after acceptance",
            },
        );
        let out = actions(&mut e);
        assert_eq!(e.store().orders().len(), before.len());
        let expected = [
            OrderStatus::Creating,
            OrderStatus::Canceling,
            OrderStatus::Updating,
        ][kind];
        assert!(e.store().orders().iter().any(|o| o.status == expected));
        assert!(out.iter().any(|a| matches!(a, Action::Http(r) if r.method == safe_cex_engine::Method::Get && r.url.contains("orders?status=INCOMPLETE"))));
        assert!(
            !out.iter()
                .any(|a| matches!(a, Action::Http(r) if r.method != safe_cex_engine::Method::Get))
        );
        assert!(done(out, 71).is_err());
    }
}

#[test]
fn uncertain_placement_is_retired_only_by_a_successful_fresh_order_snapshot() {
    for status in [200, 503] {
        let (mut e, _) = ready();
        requests(&mut e);
        place(
            &mut e,
            81,
            r#"[{"symbol":"BTCUSDC","type":"limit","side":"buy","amount":1,"price":50000}]"#,
        );
        let write = requests(&mut e).pop().unwrap();
        e.handle(
            2001.0,
            Event::HttpFailed {
                request: write.id,
                error: "lost response",
            },
        );
        let fresh = requests(&mut e)
            .into_iter()
            .find(|r| r.url.contains("/v1/orders?"))
            .unwrap();
        assert!(e.store().order("pending:81").is_some());
        let body = format!(
            r#"{{"success":true,"data":{{"rows":[{}],"meta":{{"total":1}}}}}}"#,
            ORDER.replace("9007199254740993", "9007199254740994")
        );
        reply(&mut e, &fresh, status, &body);
        assert_eq!(e.store().order("pending:81").is_some(), status != 200);
        if status == 200 {
            assert!(e.store().order("9007199254740994").is_some());
        }
    }
}

#[test]
fn app_shaped_standalone_stops_accept_the_same_trigger_and_price() {
    for (kind, native_kind, price) in [
        ("stop_market", "STOP_LOSS", 48000),
        ("take_profit_market", "TAKE_PROFIT", 56000),
    ] {
        let (mut e, _) = ready();
        requests(&mut e);
        place(
            &mut e,
            91,
            &format!(
                r#"[{{"symbol":"BTCUSDC","type":"{kind}","side":"sell","amount":1,"price":{price},"triggerPrice":{price},"reduceOnly":true,"close":true,"triggerType":"mark_price"}}]"#
            ),
        );
        let writes = requests(&mut e);
        assert_eq!(writes.len(), 1);
        assert_eq!(writes[0].method, safe_cex_engine::Method::Post);
        assert!(writes[0].url.ends_with("/v1/algo/order"));
        let payload = body(&writes[0]);
        assert_eq!(payload["algo_type"], "POSITIONAL_TP_SL");
        assert_eq!(payload["child_orders"].as_array().unwrap().len(), 1);
        assert_eq!(payload["child_orders"][0]["algo_type"], native_kind);
        assert_eq!(
            payload["child_orders"][0]["trigger_price"],
            price.to_string()
        );
        assert_eq!(payload["child_orders"][0]["type"], "CLOSE_POSITION");
    }
}

#[test]
fn positional_trigger_rejects_different_prices_and_opening_conditionals_before_writes() {
    for order in [
        r#"{"symbol":"BTCUSDC","type":"stop_market","side":"sell","amount":1,"price":48000,"triggerPrice":47999}"#,
        r#"{"symbol":"BTCUSDC","type":"take_profit_market","side":"sell","amount":1,"price":56000,"triggerPrice":56001}"#,
        r#"{"symbol":"BTCUSDC","type":"conditional","side":"sell","amount":1,"price":48000,"triggerPrice":48000}"#,
        r#"{"symbol":"BTCUSDC","type":"limit","side":"sell","amount":1,"price":48000,"triggerPrice":48000}"#,
    ] {
        let (mut e, _) = ready();
        requests(&mut e);
        place(&mut e, 92, &format!("[{order}]"));
        let out = actions(&mut e);
        assert!(!out.iter().any(|action| matches!(action, Action::Http(_))));
        assert!(done(out, 92).is_err());
    }
}

fn unrelated_protection(side: &str, margin: &str) -> String {
    let finite = ALGO
        .replace("POSITIONAL_TP_SL", "TP_SL")
        .replace("CLOSE_POSITION", "MARKET")
        .replace("\"quantity\":0", "\"quantity\":0.1")
        .replace("100", "200")
        .replace("101", "201")
        .replace("102", "202")
        .replace("SELL", side)
        .replace("CROSS", margin);
    let trailing = format!(
        r#"{{"algo_order_id":700,"root_algo_order_id":700,"symbol":"PERP_BTC_USDC","algo_type":"TRAILING_STOP","algo_status":"NEW","margin_mode":"{margin}","side":"{side}","type":"MARKET","reduce_only":true,"quantity":0.25,"trigger_price":50000}}"#
    );
    format!("[{},{}]", &finite[1..finite.len() - 1], trailing)
}

#[test]
fn standalone_sl_tp_leave_same_side_trailing_and_quantity_protections_untouched() {
    for (side, margin) in [("SELL", "CROSS"), ("BUY", "ISOLATED")] {
        for (kind, native_kind, price) in [
            ("stop_market", "STOP_LOSS", 48000),
            ("take_profit_market", "TAKE_PROFIT", 56000),
        ] {
            let algos = unrelated_protection(side, margin);
            let (mut e, _) = ready_with(ORDER, &algos, POSITIONS);
            assert!(e.store().loaded().orders);
            assert!(e.store().order("700").is_some());
            let before = e.store().orders().to_vec();
            requests(&mut e);
            place(
                &mut e,
                93,
                &format!(
                    r#"[{{"symbol":"BTCUSDC","type":"{kind}","side":"{}","amount":1,"price":{price},"triggerPrice":{price},"reduceOnly":true}}]"#,
                    side.to_ascii_lowercase()
                ),
            );
            let writes = requests(&mut e);
            assert_eq!(writes.len(), 1);
            assert_eq!(
                writes[0].method,
                safe_cex_engine::Method::Post,
                "must not cancel unrelated protection"
            );
            assert_eq!(
                body(&writes[0])["child_orders"][0]["algo_type"],
                native_kind
            );
            for old in before {
                let current = e.store().order(&old.id).unwrap();
                assert_eq!(current.status, old.status);
                assert_eq!(current.price, old.price);
            }
        }
    }
}

#[test]
fn replacing_full_position_pair_does_not_cancel_independent_finite_or_trailing_roots() {
    for (side, margin) in [("SELL", "CROSS"), ("BUY", "ISOLATED")] {
        for (kind, price) in [("stop_market", 48000), ("take_profit_market", 56000)] {
            let unrelated = unrelated_protection(side, margin);
            let full = ALGO.replace("SELL", side).replace("CROSS", margin);
            let algos = format!(
                "[{},{}]",
                &full[1..full.len() - 1],
                &unrelated[1..unrelated.len() - 1]
            );
            let (mut e, socket) = ready_with(ORDER, &algos, POSITIONS);
            requests(&mut e);
            place(
                &mut e,
                94,
                &format!(
                    r#"[{{"symbol":"BTCUSDC","type":"{kind}","side":"{}","amount":1,"price":{price},"triggerPrice":{price},"reduceOnly":true}}]"#,
                    side.to_ascii_lowercase()
                ),
            );
            let writes = requests(&mut e);
            assert_eq!(writes.len(), 1);
            assert_eq!(writes[0].method, safe_cex_engine::Method::Delete);
            assert!(writes[0].url.contains("order_id=100&"));
            reply(
                &mut e,
                &writes[0],
                200,
                r#"{"success":true,"data":{"status":"CANCEL_SENT"}}"#,
            );
            requests(&mut e);
            ws(
                &mut e,
                socket,
                r#"{"topic":"algoexecutionreportv2","ts":3000,"data":{"algoOrderId":100,"rootAlgoOrderId":100,"symbol":"PERP_BTC_USDC","algoType":"POSITIONAL_TP_SL","algoStatus":"CANCELLED","seq":5}}"#,
            );
            let writes = requests(&mut e);
            assert_eq!(writes.len(), 1);
            assert_eq!(writes[0].method, safe_cex_engine::Method::Post);
            let children = body(&writes[0])["child_orders"].as_array().unwrap().clone();
            assert_eq!(children.len(), 2);
            assert_eq!(
                children[0]["trigger_price"],
                if kind == "stop_market" {
                    "48000"
                } else {
                    "49000"
                }
            );
            assert_eq!(
                children[1]["trigger_price"],
                if kind == "take_profit_market" {
                    "56000"
                } else {
                    "55000"
                }
            );
            for id in ["201", "202", "700"] {
                assert_eq!(
                    e.store().order(id).unwrap().status,
                    safe_cex_engine::model::OrderStatus::Open
                );
            }
        }
    }
}

#[test]
fn mixed_full_and_quantity_siblings_cannot_be_dropped_by_parent_replacement() {
    let algos = ALGO.replace("\"algo_type\":\"TAKE_PROFIT\",\"side\":\"SELL\",\"algo_status\":\"NEW\",\"trigger_price\":55000,\"type\":\"CLOSE_POSITION\",\"reduce_only\":true,\"quantity\":0", "\"algo_type\":\"TAKE_PROFIT\",\"side\":\"SELL\",\"algo_status\":\"NEW\",\"trigger_price\":55000,\"type\":\"MARKET\",\"reduce_only\":true,\"quantity\":0.1");
    assert_ne!(algos, ALGO);
    let (mut e, _) = ready_with(ORDER, &algos, POSITIONS);
    requests(&mut e);
    place(
        &mut e,
        95,
        r#"[{"symbol":"BTCUSDC","type":"stop_market","side":"sell","amount":1,"price":48000,"triggerPrice":48000}]"#,
    );
    let out = actions(&mut e);
    assert!(!out.iter().any(|action| matches!(action, Action::Http(_))));
    assert!(done(out, 95).is_err());
    assert_eq!(e.store().order("102").unwrap().amount, 0.1);
    assert_eq!(
        e.store().order("102").unwrap().status,
        safe_cex_engine::model::OrderStatus::Open
    );
}

#[test]
fn ambiguous_http_or_success_ack_preserves_pending_writes_and_retires_stale_reads() {
    use safe_cex_engine::model::OrderStatus;
    use safe_cex_engine::{Method, Response};
    for (status, body) in [
        (408, "request timeout"),
        (500, "internal error"),
        (503, "unavailable"),
        (504, "gateway timeout"),
        (200, "truncated"),
        (200, r#"{"success":true,"data":{}}"#),
    ] {
        for operation in [0, 1, 2] {
            let (mut e, _) = ready();
            let held: Vec<_> = requests(&mut e)
                .into_iter()
                .filter(|r| r.url.contains("/v1/orders?"))
                .collect();
            assert!(
                !held.is_empty(),
                "fixture must hold a pre-write authoritative read"
            );
            if operation == 0 {
                place(
                    &mut e,
                    81,
                    r#"[{"symbol":"BTCUSDC","type":"limit","side":"buy","amount":1,"price":50000}]"#,
                );
            } else {
                e.handle(
                    2000.,
                    Event::Command(if operation == 1 {
                        Command::CancelOrders {
                            command: 81,
                            ids: vec!["9007199254740993".into()],
                        }
                    } else {
                        Command::AmendOrder {
                            command: 81,
                            id: "9007199254740993".into(),
                            price: Some(50001.),
                            amount: None,
                        }
                    }),
                );
            }
            let write = requests(&mut e).pop().unwrap();
            let id = if operation == 0 {
                "pending:81"
            } else {
                "9007199254740993"
            };
            e.handle(
                2001.,
                Event::HttpResponse {
                    request: write.id,
                    response: Response {
                        status,
                        headers: "",
                        body,
                    },
                },
            );
            let out = actions(&mut e);
            let expected = [
                OrderStatus::Creating,
                OrderStatus::Canceling,
                OrderStatus::Updating,
            ][operation];
            assert_eq!(
                e.store().order(id).unwrap().status,
                expected,
                "{status}: {body}"
            );
            assert!(
                !out.iter()
                    .any(|a| matches!(a, Action::Http(r) if r.method != Method::Get)),
                "ambiguous write must never retry"
            );
            let fresh = out
                .iter()
                .find_map(|a| match a {
                    Action::Http(r) if r.url.contains("/v1/orders?") => Some(r.clone()),
                    _ => None,
                })
                .unwrap();
            assert!(done(out, 81).is_err());
            for read in held {
                e.handle(
                    2002.,
                    Event::HttpResponse {
                        request: read.id,
                        response: Response {
                            status: 200,
                            headers: "",
                            body: r#"{"success":true,"data":{"rows":[],"meta":{"total":0}}}"#,
                        },
                    },
                );
            }
            assert_eq!(
                e.store().order(id).unwrap().status,
                expected,
                "pre-write read must remain stale"
            );
            e.handle(
                2003.,
                Event::HttpResponse {
                    request: fresh.id,
                    response: Response {
                        status: 200,
                        headers: "",
                        body: r#"{"success":true,"data":{"rows":[],"meta":{"total":0}}}"#,
                    },
                },
            );
            assert!(
                e.store().order(id).is_none(),
                "fresh complete read retires optimistic state"
            );
        }
    }
}

#[test]
fn definite_native_rejection_still_restores_optimistic_order_state() {
    use safe_cex_engine::model::OrderStatus;
    let (mut e, _) = ready();
    requests(&mut e);
    e.handle(
        2000.,
        Event::Command(Command::CancelOrders {
            command: 82,
            ids: vec!["9007199254740993".into()],
        }),
    );
    let write = requests(&mut e).pop().unwrap();
    reply(
        &mut e,
        &write,
        400,
        r#"{"success":false,"code":-1005,"message":"Invalid order id"}"#,
    );
    assert!(done(actions(&mut e), 82).is_err());
    assert_eq!(
        e.store().order("9007199254740993").unwrap().status,
        OrderStatus::Open
    );
}

#[test]
fn invalid_dynamic_leverage_limits_reject_without_native_write() {
    for (min, max) in [
        (30.0, 20.0),
        (f64::NAN, 20.0),
        (1.0, 0.0),
        (1.0, f64::INFINITY),
    ] {
        let (mut e, _) = ready();
        actions(&mut e);
        e.store_mut().update_markets(|market| {
            market.limits.leverage.min = min;
            market.limits.leverage.max = max;
        });
        e.handle(
            2000.0,
            Event::Command(Command::SetLeverage {
                command: 99,
                symbol: "BTCUSDC".into(),
                leverage: 5.0,
                side: None,
            }),
        );
        let out = actions(&mut e);
        assert!(!out.iter().any(|action| matches!(action, Action::Http(_))));
        assert!(done(out, 99).unwrap_err().contains("leverage limits"));
    }
}

#[test]
fn accepted_global_leverage_continues_past_invalid_market_limits() {
    use safe_cex_engine::model::MarginType;
    let (mut e, _) = ready();
    actions(&mut e);
    e.handle(
        2000.,
        Event::Command(Command::SetMarginType {
            command: 100,
            symbol: "ETHUSDC".into(),
            isolated: false,
        }),
    );
    let r = requests(&mut e).pop().unwrap();
    reply(&mut e, &r, 200, r#"{"success":true}"#);
    assert!(done(actions(&mut e), 100).is_ok());
    let mut markets = e.store().markets().to_vec();
    let mut sol = markets[0].clone();
    sol.id = "SOL/USDC:USDC".into();
    sol.symbol = "SOLUSDC".into();
    sol.base = "SOL".into();
    sol.limits.leverage.max = 30.;
    markets[1].limits.leverage.min = 50.;
    markets[1].limits.leverage.max = 10.;
    markets.push(sol);
    e.store_mut().set_markets(markets);
    let positions = e.store().positions().to_vec();
    let cross = positions
        .iter()
        .find(|p| p.margin_type == Some(MarginType::Cross))
        .unwrap()
        .clone();
    let isolated = positions
        .iter()
        .find(|p| p.margin_type == Some(MarginType::Isolated))
        .unwrap()
        .clone();
    let mut rows = vec![isolated.clone()];
    for symbol in ["BTCUSDC", "ETHUSDC", "SOLUSDC"] {
        let mut p = cross.clone();
        p.id = symbol.into();
        p.symbol = symbol.into();
        rows.push(p);
    }
    e.store_mut().set_positions(rows);
    e.handle(
        2000.,
        Event::Command(Command::SetLeverage {
            command: 101,
            symbol: String::new(),
            leverage: 80.,
            side: None,
        }),
    );
    let r = requests(&mut e).pop().unwrap();
    assert_eq!(body(&r)["leverage"], 80.);
    assert!(body(&r).get("symbol").is_none());
    reply(&mut e, &r, 200, r#"{"success":true}"#);
    let out = actions(&mut e);
    assert!(out.iter().any(|a|matches!(a,Action::Notice(safe_cex_engine::Notice::Event(safe_cex_engine::LogEvent { level: safe_cex_engine::Level::Warn, message, .. })) if message.contains("ETHUSDC") && message.contains("confirmed account leverage"))));
    assert!(done(out, 101).is_ok());
    for (symbol, expected) in [("BTCUSDC", 80.), ("ETHUSDC", 80.), ("SOLUSDC", 30.)] {
        assert_eq!(e.store().setting(symbol).unwrap().leverage, expected);
        assert_eq!(
            e.store()
                .positions()
                .iter()
                .find(|p| p.id == symbol)
                .unwrap()
                .leverage,
            expected
        );
    }
    assert_eq!(
        e.store()
            .positions()
            .iter()
            .find(|p| p.id == isolated.id)
            .unwrap()
            .leverage,
        isolated.leverage
    );
}

fn outcome(out: Vec<Action>, id: u32) -> Option<Option<safe_cex_engine::Outcome>> {
    out.into_iter().find_map(|a| match a {
        Action::Notice(Notice::CommandFailed { command, error }) if command == id => {
            Some(error.outcome)
        }
        _ => None,
    })
}

#[test]
fn lost_and_server_failed_writes_are_unknown_but_rejections_are_not() {
    use safe_cex_engine::Outcome;
    let order = r#"[{"symbol":"BTCUSDC","type":"limit","side":"buy","amount":1,"price":50000}]"#;
    let (mut e, _) = ready();
    requests(&mut e);
    place(&mut e, 1, order);
    let r = requests(&mut e).pop().unwrap();
    e.handle(
        2000.0,
        Event::HttpFailed {
            request: r.id,
            error: "response lost",
        },
    );
    assert_eq!(outcome(actions(&mut e), 1), Some(Some(Outcome::Unknown)));

    let (mut e, _) = ready();
    requests(&mut e);
    place(&mut e, 2, order);
    let r = requests(&mut e).pop().unwrap();
    reply(&mut e, &r, 502, "bad gateway");
    assert_eq!(outcome(actions(&mut e), 2), Some(Some(Outcome::Unknown)));

    let (mut e, _) = ready();
    requests(&mut e);
    place(&mut e, 3, order);
    let r = requests(&mut e).pop().unwrap();
    reply(
        &mut e,
        &r,
        400,
        r#"{"success":false,"code":-1103,"message":"price out of range"}"#,
    );
    assert_eq!(outcome(actions(&mut e), 3), Some(Some(Outcome::Rejected)));

    // A server-side code does not prove the order was refused.
    let (mut e, _) = ready();
    requests(&mut e);
    place(&mut e, 4, order);
    let r = requests(&mut e).pop().unwrap();
    reply(
        &mut e,
        &r,
        400,
        r#"{"success":false,"code":-1012,"message":"rpc rejected"}"#,
    );
    let out = actions(&mut e);
    assert_eq!(outcome(out.clone(), 4), Some(Some(Outcome::Unknown)));
    // Reconciled like a lost write: account state is re-read.
    assert!(
        out.iter()
            .any(|a| matches!(a, Action::Http(r) if r.method == safe_cex_engine::Method::Get))
    );
}

#[test]
#[ignore = "original RED: P0 write adoption waits for signer and Plan seams"]
fn p0_write_timeout_code_remains_unknown_and_reconciles() {
    let (mut e, _) = ready();
    actions(&mut e);
    place(
        &mut e,
        701,
        r#"[{"symbol":"BTCUSDC","type":"limit","side":"buy","amount":1,"price":50000}]"#,
    );
    let request = requests(&mut e).pop().unwrap();
    reply(
        &mut e,
        &request,
        200,
        r#"{"success":false,"code":-1007,"message":"timeout"}"#,
    );
    let out = actions(&mut e);
    let failures: Vec<_> = out
        .iter()
        .filter_map(|action| match action {
            Action::Notice(Notice::CommandFailed {
                command: 701,
                error,
            }) => Some(error),
            _ => None,
        })
        .collect();
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0].venue_code.as_deref(), Some("-1007"));
    assert_eq!(failures[0].outcome, Some(safe_cex_engine::Outcome::Unknown));
    for path in ["/v1/positions", "/v1/orders?", "/v1/algo/orders?"] {
        assert!(
            out.iter()
                .any(|action| matches!(action, Action::Http(request)
            if request.url.contains(path))),
            "missing reconcile {path}"
        );
    }
    assert!(
        !out.iter()
            .any(|action| matches!(action, Action::Http(request)
        if request.method == safe_cex_engine::Method::Post))
    );
}

#[test]
#[ignore = "original RED: P0 write adoption waits for signer and Plan seams"]
fn p0_write_5xx_preserves_native_code_without_proving_refusal() {
    let (mut e, _) = ready();
    actions(&mut e);
    place(
        &mut e,
        702,
        r#"[{"symbol":"BTCUSDC","type":"limit","side":"buy","amount":1,"price":50000}]"#,
    );
    let request = requests(&mut e).pop().unwrap();
    reply(
        &mut e,
        &request,
        503,
        r#"{"success":false,"code":-1001,"message":"auth-looking upstream failure"}"#,
    );
    let out = actions(&mut e);
    let failures: Vec<_> = out
        .iter()
        .filter_map(|action| match action {
            Action::Notice(Notice::CommandFailed {
                command: 702,
                error,
            }) => Some(error),
            _ => None,
        })
        .collect();
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0].venue_code.as_deref(), Some("-1001"));
    assert_eq!(failures[0].outcome, Some(safe_cex_engine::Outcome::Unknown));
    assert!(
        !out.iter()
            .any(|action| matches!(action, Action::Http(request)
        if request.method == safe_cex_engine::Method::Post))
    );
}

#[test]
#[ignore = "original RED: Info auth classification waits for managed account adoption"]
fn p0_write_info_non_auth_4xx_does_not_deactivate() {
    let mut e = safe_cex_engine::test_support::seeded_engine(safe_cex_venue_orderly::Orderly::new(
        options(),
    ));
    e.start(1000.0);
    let mut delivered_info = false;
    for _ in 0..4 {
        let request = requests(&mut e).pop().expect("native startup request");
        if request.url.ends_with("/v1/public/info") {
            reply(&mut e, &request, 200, MARKETS);
        } else if request.url.ends_with("/v1/public/futures") {
            reply(&mut e, &request, 200, TICKERS);
        } else {
            assert!(request.url.ends_with("/v1/client/info"));
            delivered_info = true;
            reply(
                &mut e,
                &request,
                400,
                r#"{"success":false,"code":-1004,"message":"invalid request parameter"}"#,
            );
            let out = actions(&mut e);
            assert!(
                !out.iter()
                    .any(|action| matches!(action, Action::Notice(Notice::Deactivate))),
                "an unrelated request error does not prove invalid credentials"
            );
            assert!(!out.iter().any(
                |action| matches!(action, Action::Notice(Notice::Event(event))
                if event.code == safe_cex_engine::log::code::AUTH_REJECTED)
            ));
            assert!(
                out.iter()
                    .any(|action| matches!(action, Action::SetTimer { .. })),
                "native account read recovery must remain armed"
            );
            break;
        }
    }
    assert!(
        delivered_info,
        "Info failure exercised, not just public discovery"
    );
}
