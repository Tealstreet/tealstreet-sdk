mod support;
use safe_cex_engine::{Action, Engine, Event, SocketId, TimerId};
use safe_cex_venue_orderly::Orderly;
use support::*;

#[test]
fn bootstrap_signed_headers_and_dual_margin_positions() {
    let (mut e, _) = ready();
    assert!(
        e.store().loaded().positions && e.store().loaded().orders && e.store().loaded().balance
    );
    assert_eq!(e.store().positions().len(), 2);
    assert_eq!(e.store().positions()[0].id, "BTCUSDC");
    assert_eq!(e.store().positions()[1].id, "BTCUSDC:isolated");
    assert_eq!(e.store().positions()[1].margin_size, Some(6000.0));
    assert_eq!(e.store().balance().total, 8500.0);
    assert_eq!(e.store().orders()[0].id, "9007199254740993");
    assert_eq!(
        e.store().orders()[0].client_order_id.as_deref(),
        Some("caller-42")
    );
    for r in requests(&mut e) {
        assert!(
            r.headers
                .iter()
                .any(|(k, v)| *k == "orderly-key" && v.starts_with("ed25519:"))
        );
        assert!(
            r.headers
                .iter()
                .any(|(k, v)| *k == "orderly-textsign" && v.starts_with("2000GET/v1/"))
        );
    }
}
#[test]
fn subscribes_only_after_successful_auth_and_paces_failure() {
    let mut e = Engine::new(Orderly::new(options()));
    e.start(1000.0);
    let r = requests(&mut e).pop().unwrap();
    reply(
        &mut e,
        &r,
        503,
        r#"{"success":false,"message":"upstream unavailable"}"#,
    );
    assert!(!e.store().loaded().markets);
    assert!(
        actions(&mut e)
            .iter()
            .any(|a| matches!(a, Action::SetTimer { .. }))
    );
    let (mut e, s) = ready();
    let out = actions(&mut e);
    let topics: Vec<_> = out
        .iter()
        .filter_map(|a| {
            if let Action::WsSend { text, .. } = a {
                Some(text)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(topics.iter().filter(|t| t.contains("subscribe")).count(), 3);
    ws(&mut e, s, r#"{"event":"auth","success":true}"#);
    assert!(
        !actions(&mut e)
            .iter()
            .any(|a| matches!(a,Action::WsSend{text,..}if text.contains("subscribe")))
    );
}
#[test]
fn failed_and_stale_snapshots_preserve_last_good_balance_positions_orders() {
    let (mut e, s) = ready();
    let polls = requests(&mut e);
    let prior = e.store().balance().clone();
    ws(
        &mut e,
        s,
        r#"{"topic":"position","ts":2000,"data":{"positions":[{"symbol":"PERP_BTC_USDC","positionQty":2,"averageOpenPrice":50000,"markPrice":51000,"marginMode":"CROSS","leverage":10,"seq":9007199254740993}]}}"#,
    );
    for r in polls {
        reply(&mut e, &r, 503, r#"{"success":false,"message":"busy"}"#);
    }
    assert_eq!(
        e.store()
            .positions()
            .iter()
            .find(|p| p.id == "BTCUSDC")
            .unwrap()
            .contracts,
        2.0
    );
    assert_eq!(e.store().balance(), &prior);
    assert_eq!(e.store().orders().len(), 1);
}
#[test]
fn duplicate_executions_emit_only_incremental_fill_and_cumulative_order_state() {
    let (mut e, s) = ready();
    actions(&mut e);
    e.take_frame(1000.0);
    let partial = r#"{"topic":"executionreport","ts":2000,"data":{"orderId":9007199254740993,"symbol":"PERP_BTC_USDC","type":"LIMIT","side":"BUY","status":"PARTIAL_FILLED","price":50000,"quantity":1,"totalExecutedQuantity":0.25,"executedQuantity":0.25,"executedPrice":50000,"tradeId":11,"seq":9007199254740993,"marginMode":"CROSS"}}"#;
    ws(&mut e, s, partial);
    ws(&mut e, s, partial);
    assert_eq!(fill_amounts(&mut e), [0.25]);
    assert_eq!(e.store().orders()[0].filled, 0.25);
    ws(
        &mut e,
        s,
        r#"{"topic":"executionreport","ts":2100,"data":{"orderId":9007199254740993,"symbol":"PERP_BTC_USDC","type":"LIMIT","side":"BUY","status":"PARTIALLY_FILLED","price":50000,"quantity":1,"totalExecutedQuantity":0.75,"executedQuantity":0.5,"executedPrice":50001,"tradeId":12,"seq":9007199254740994,"marginMode":"CROSS"}}"#,
    );
    assert_eq!(fill_amounts(&mut e), [0.5]);
    assert_eq!(e.store().orders()[0].remaining, 0.25);
    ws(&mut e, s, partial);
    assert!(fill_amounts(&mut e).is_empty());
    assert_eq!(e.store().orders()[0].filled, 0.75);
}
#[test]
fn position_sequence_and_zero_update_are_scoped_by_margin_mode() {
    let (mut e, s) = ready();
    requests(&mut e);
    for (seq, qty) in [(9007199254740994u64, 0), (9007199254740993, 2)] {
        ws(
            &mut e,
            s,
            &format!(
                r#"{{"topic":"position","ts":2000,"data":{{"positions":[{{"symbol":"PERP_BTC_USDC","positionQty":{qty},"marginMode":"ISOLATED","seq":{seq}}}]}}}}"#
            ),
        );
    }
    assert_eq!(e.store().positions().len(), 1);
    assert_eq!(e.store().positions()[0].id, "BTCUSDC");
}

#[test]
fn successful_rest_snapshot_establishes_sequence_before_delayed_pushes() {
    let positions = POSITIONS.replacen(
        "\"position_qty\":1",
        "\"seq\":200,\"timestamp\":1000,\"position_qty\":1",
        1,
    );
    let order = ORDER.replace(
        "\"order_id\":",
        "\"seq\":300,\"timestamp\":1000,\"order_id\":",
    );
    let (mut e, s) = ready_with(&order, "[]", &positions);
    requests(&mut e);
    ws(
        &mut e,
        s,
        r#"{"topic":"position","ts":3000,"data":{"positions":[{"symbol":"PERP_BTC_USDC","positionQty":99,"marginMode":"CROSS","seq":199,"timestamp":3000,"updated_time":2000}]}}"#,
    );
    ws(
        &mut e,
        s,
        r#"{"topic":"executionreport","ts":3000,"data":{"orderId":9007199254740993,"symbol":"PERP_BTC_USDC","type":"LIMIT","side":"BUY","status":"NEW","quantity":99,"price":50000,"seq":299,"timestamp":3000}}"#,
    );
    assert_eq!(
        e.store()
            .positions()
            .iter()
            .find(|p| p.id == "BTCUSDC")
            .unwrap()
            .contracts,
        1.0
    );
    assert_eq!(e.store().orders()[0].amount, 1.0);
}

#[test]
fn accepted_execution_invalidates_an_already_pending_positions_and_balance_read() {
    let (mut e, s) = ready();
    let polls = requests(&mut e);
    let balance = e.store().balance().clone();
    ws(
        &mut e,
        s,
        r#"{"topic":"executionreport","ts":2000,"data":{"orderId":9007199254740993,"symbol":"PERP_BTC_USDC","type":"LIMIT","side":"BUY","status":"PARTIAL_FILLED","price":50000,"quantity":1,"totalExecutedQuantity":0.25,"executedQuantity":0.25,"executedPrice":50000,"tradeId":51,"seq":4}}"#,
    );
    let r = polls
        .into_iter()
        .find(|r| r.url.ends_with("/v1/positions"))
        .unwrap();
    reply(
        &mut e,
        &r,
        200,
        r#"{"success":true,"data":{"rows":[],"total_collateral_value":99,"free_collateral":1}}"#,
    );
    assert_eq!(e.store().positions().len(), 2);
    assert_eq!(e.store().balance(), &balance);
}

#[test]
fn documented_algo_status_fields_coexist_and_root_rejection_cascades() {
    let algos = r#"[{"algo_order_id":100,"root_algo_order_id":100,"symbol":"PERP_BTC_USDC","algo_type":"POSITIONAL_TP_SL","algo_status":"NEW","root_algo_order_status":"NEW","margin_mode":"CROSS","child_orders":[{"algo_order_id":101,"root_algo_order_id":100,"algo_type":"STOP_LOSS","side":"SELL","status":"NEW","algo_status":"NEW","trigger_price":49000,"quantity":1,"total_executed_quantity":0}]}]"#;
    let (mut e, s) = ready_with(ORDER, algos, POSITIONS);
    requests(&mut e);
    e.take_frame(1000.0);
    ws(
        &mut e,
        s,
        r#"{"topic":"algoexecutionreportv2","ts":2000,"data":{"algoOrderId":101,"rootAlgoOrderId":100,"symbol":"PERP_BTC_USDC","algoType":"STOP_LOSS","side":"SELL","status":"PARTIAL_FILLED","algoStatus":"PARTIAL_FILLED","rootAlgoStatus":"NEW","triggerPrice":49000,"quantity":1,"totalExecutedQuantity":0.25,"executedQuantity":0.25,"executedPrice":48999,"seq":10}}"#,
    );
    assert_eq!(fill_amounts(&mut e), [0.25]);
    assert_eq!(
        e.store()
            .orders()
            .iter()
            .find(|o| o.id == "101")
            .unwrap()
            .filled,
        0.25
    );
    ws(
        &mut e,
        s,
        r#"{"topic":"algoexecutionreportv2","ts":3000,"data":{"algoOrderId":100,"rootAlgoOrderId":100,"symbol":"PERP_BTC_USDC","algoType":"POSITIONAL_TP_SL","status":"REJECTED","algoStatus":"REJECTED","seq":11}}"#,
    );
    assert_eq!(
        e.store()
            .orders()
            .iter()
            .find(|o| o.id == "101")
            .unwrap()
            .status,
        safe_cex_engine::model::OrderStatus::Canceled
    );
}

#[test]
fn root_terminal_before_any_children_blocks_a_held_startup_snapshot() {
    let (mut e, s) = ready();
    let polls = requests(&mut e);
    let r = polls
        .into_iter()
        .find(|r| r.url.contains("/v1/algo/orders"))
        .unwrap();
    ws(
        &mut e,
        s,
        r#"{"topic":"algoexecutionreportv2","ts":2000,"data":{"algoOrderId":100,"rootAlgoOrderId":100,"symbol":"PERP_BTC_USDC","algoType":"POSITIONAL_TP_SL","status":"NEW","algoStatus":"NEW","rootAlgoStatus":"REJECTED","seq":10}}"#,
    );
    reply(
        &mut e,
        &r,
        200,
        r#"{"success":true,"data":{"rows":[{"algo_order_id":100,"root_algo_order_id":100,"symbol":"PERP_BTC_USDC","algo_type":"POSITIONAL_TP_SL","algo_status":"NEW","margin_mode":"CROSS","child_orders":[{"algo_order_id":101,"root_algo_order_id":100,"algo_type":"STOP_LOSS","side":"SELL","algo_status":"NEW","trigger_price":49000,"quantity":1}]}],"meta":{"total":1}}}"#,
    );
    assert!(!e.store().orders().iter().any(|o| o.id == "101"));
}

#[test]
fn only_definitive_account_validation_rejection_deactivates() {
    use safe_cex_engine::Notice;
    for status in [401, 403, 408, 429, 503] {
        let mut e = Engine::new(Orderly::new(options()));
        e.start(1000.0);
        let mut output = vec![];
        for _ in 0..4 {
            let r = requests(&mut e).pop().unwrap();
            if r.url.ends_with("/v1/public/info") {
                reply(&mut e, &r, 200, MARKETS);
            } else if r.url.ends_with("/v1/public/futures") {
                reply(&mut e, &r, 200, TICKERS);
            } else {
                assert!(r.url.ends_with("/v1/client/info"));
                reply(
                    &mut e,
                    &r,
                    status,
                    r#"{"success":false,"message":"validation failed"}"#,
                );
                output = actions(&mut e);
                break;
            }
        }
        assert_eq!(
            output
                .iter()
                .any(|a| matches!(a, Action::Notice(Notice::Deactivate))),
            status == 401 || status == 403
        );
        assert_eq!(
            output.iter().any(|a| matches!(a, Action::SetTimer { .. })),
            status != 401 && status != 403
        );
    }
}

fn private_connecting() -> (Driver, SocketId, TimerId) {
    let mut e = Engine::new(Orderly::new(options()));
    e.start(1000.0);
    for _ in 0..10 {
        let mut private = None;
        for action in actions(&mut e) {
            match action {
                Action::Http(r) => {
                    let body = if r.url.ends_with("/v1/client/info") {
                        r#"{"success":true,"data":{"max_leverage":20}}"#
                    } else if r.url.ends_with("/v1/public/info") {
                        MARKETS
                    } else if r.url.ends_with("/v1/public/futures") {
                        TICKERS
                    } else {
                        assert!(r.url.ends_with("/v1/client/margin_modes"));
                        r#"{"success":true,"data":{"rows":[]}}"#
                    };
                    reply(&mut e, &r, 200, body);
                }
                Action::WsConnect {
                    socket,
                    url,
                    channel,
                    ..
                } if url.contains("private") => {
                    assert_eq!(channel, Some(safe_cex_engine::SocketChannel::Private));
                    private = Some(socket);
                }
                Action::SetTimer { timer, at_ms } if private.is_some() => {
                    assert_eq!(at_ms, 11000.0);
                    return (e, private.unwrap(), timer);
                }
                _ => {}
            }
        }
    }
    panic!("private connect did not arm a handshake timer");
}

#[test]
fn private_initial_and_reconnect_handshakes_time_out_without_open_callbacks() {
    let (mut e, old, handshake) = private_connecting();
    e.handle(11000.0, Event::TimerFired { timer: handshake });
    let out = actions(&mut e);
    assert!(
        out.iter()
            .any(|a| matches!(a, Action::WsClose { socket } if *socket == old))
    );
    let retry = out
        .into_iter()
        .find_map(|a| match a {
            Action::SetTimer {
                timer,
                at_ms: 12000.0,
            } => Some(timer),
            _ => None,
        })
        .unwrap();
    e.handle(12000.0, Event::TimerFired { timer: retry });
    let out = actions(&mut e);
    let fresh = out
        .iter()
        .find_map(|a| match a {
            Action::WsConnect { socket, url, .. } if url.contains("private") => Some(*socket),
            _ => None,
        })
        .unwrap();
    assert_ne!(fresh, old);
    let next_handshake = out
        .into_iter()
        .find_map(|a| match a {
            Action::SetTimer {
                timer,
                at_ms: 22000.0,
            } => Some(timer),
            _ => None,
        })
        .unwrap();
    e.handle(13000.0, Event::WsOpened { socket: old });
    assert!(
        actions(&mut e).is_empty(),
        "late old opens cannot authenticate or replace the fresh timer"
    );
    e.handle(
        22000.0,
        Event::TimerFired {
            timer: next_handshake,
        },
    );
    let out = actions(&mut e);
    assert!(
        out.iter()
            .any(|a| matches!(a, Action::WsClose { socket } if *socket == fresh))
    );
    assert!(
        out.iter()
            .any(|a| matches!(a, Action::SetTimer { at_ms, .. } if *at_ms == 24000.0)),
        "consecutive handshake failures back off"
    );
}

#[test]
fn private_open_replaces_handshake_timer_and_duplicate_open_cannot_resend_auth() {
    let (mut e, socket, handshake) = private_connecting();
    e.handle(2000.0, Event::WsOpened { socket });
    let out = actions(&mut e);
    assert!(
        out.iter()
            .any(|a| matches!(a, Action::ClearTimer { timer } if *timer == handshake))
    );
    assert_eq!(
        out.iter()
            .filter(
                |a| matches!(a, Action::WsSend { text, .. } if text.contains("\"event\":\"auth\""))
            )
            .count(),
        1
    );
    assert!(
        out.iter()
            .any(|a| matches!(a, Action::SetTimer { at_ms, .. } if *at_ms == 12000.0))
    );
    e.handle(
        3000.0,
        Event::WsMessage {
            socket,
            text: r#"{"event":"auth","success":true}"#,
        },
    );
    let out = actions(&mut e);
    assert_eq!(
        out.iter()
            .filter(|a| matches!(a, Action::WsSend { text, .. } if text.contains("subscribe")))
            .count(),
        3
    );
    e.handle(4000.0, Event::WsOpened { socket });
    assert!(actions(&mut e).is_empty());
    e.handle(11000.0, Event::TimerFired { timer: handshake });
    assert!(
        actions(&mut e).is_empty(),
        "cleared handshake callbacks cannot close the authenticated socket"
    );
    e.handle(
        11500.0,
        Event::WsMessage {
            socket,
            text: r#"{"event":"auth","success":true}"#,
        },
    );
    assert!(
        actions(&mut e).is_empty(),
        "duplicate opens must preserve authentication and subscriptions"
    );
}

#[test]
fn documented_native_stop_is_visible_with_exact_algo_cancel_identity() {
    // GET /v1/algo/orders documents STOP as an opening trigger with type MARKET.
    let stop = r#"[{"algo_order_id":168400010,"root_algo_order_id":168400010,"symbol":"PERP_BTC_USDC","algo_type":"STOP","algo_status":"NEW","side":"BUY","type":"MARKET","trigger_price":51000,"trigger_price_type":"MARK_PRICE","quantity":0.1,"reduce_only":false,"margin_mode":"CROSS"}]"#;
    let (e, _) = ready_with(ORDER, stop, POSITIONS);
    assert!(e.store().loaded().orders);
    let order = e
        .store()
        .orders()
        .iter()
        .find(|o| o.id == "168400010")
        .unwrap();
    assert_eq!(order.kind, safe_cex_engine::model::OrderType::Conditional);
    assert_eq!(order.parent_id.as_deref(), Some("168400010"));
    assert_eq!(order.price, 51000.0);
    assert!(!order.reduce_only);
}

#[test]
fn documented_nested_bracket_retains_entry_and_protective_children() {
    let bracket = r#"[{"algo_order_id":200,"root_algo_order_id":200,"symbol":"PERP_BTC_USDC","algo_type":"BRACKET","algo_status":"NEW","side":"BUY","type":"LIMIT","price":50000,"quantity":0.1,"reduce_only":false,"margin_mode":"CROSS","child_orders":[{"algo_order_id":210,"root_algo_order_id":200,"algo_type":"POSITIONAL_TP_SL","algo_status":"NEW","child_orders":[{"algo_order_id":211,"root_algo_order_id":200,"algo_type":"STOP_LOSS","algo_status":"NEW","type":"CLOSE_POSITION","side":"SELL","trigger_price":49000,"quantity":0,"reduce_only":true},{"algo_order_id":212,"root_algo_order_id":200,"algo_type":"TAKE_PROFIT","algo_status":"NEW","type":"CLOSE_POSITION","side":"SELL","trigger_price":55000,"quantity":0,"reduce_only":true}]}]}]"#;
    let (mut e, socket) = ready_with(ORDER, bracket, POSITIONS);
    assert!(e.store().loaded().orders);
    assert_eq!(e.store().orders().len(), 4);
    for (id, kind) in [
        ("200", safe_cex_engine::model::OrderType::Limit),
        ("211", safe_cex_engine::model::OrderType::StopLoss),
        ("212", safe_cex_engine::model::OrderType::TakeProfit),
    ] {
        let order = e.store().orders().iter().find(|o| o.id == id).unwrap();
        assert_eq!(order.kind, kind);
        assert_eq!(order.parent_id.as_deref(), Some("200"));
    }
    // The same nested documented shape must be handled by the stream without a
    // direct-child lookup panic; each child keeps its own status.
    ws(
        &mut e,
        socket,
        &format!(
            r#"{{"topic":"algoexecutionreportv2","ts":3000,"data":{}}}"#,
            bracket.trim_start_matches('[').trim_end_matches(']')
        ),
    );
    assert_eq!(e.store().orders().len(), 4);
}

#[test]
fn native_stop_placeholder_child_does_not_hide_the_working_parent() {
    let stop = r#"[{"algo_order_id":10,"root_algo_order_id":10,"symbol":"PERP_BTC_USDC","algo_type":"STOP","algo_status":"NEW","side":"BUY","type":"MARKET","trigger_price":51000,"quantity":0.1,"reduce_only":false,"child_orders":[{}]}]"#;
    let (e, _) = ready_with(ORDER, stop, POSITIONS);
    assert!(e.store().loaded().orders);
    assert!(e.store().orders().iter().any(|o| o.id == "10"));
    let invalid = stop.replace("[{}]", "[{\"algo_order_id\":11,\"algo_type\":\"UNKNOWN_WORKING_ORDER\",\"symbol\":\"PERP_BTC_USDC\"}]");
    let (e, _) = ready_with(ORDER, &invalid, POSITIONS);
    assert!(
        !e.store().loaded().orders,
        "unknown working descendant must reject the snapshot"
    );
}

#[test]
fn partial_bracket_entry_fill_and_triggerless_leaf_do_not_finalize_live_root_siblings() {
    use safe_cex_engine::model::OrderStatus;
    let bracket = r#"[{"algo_order_id":200,"root_algo_order_id":200,"symbol":"PERP_BTC_USDC","algo_type":"BRACKET","algo_status":"NEW","side":"BUY","type":"LIMIT","price":50000,"quantity":1,"reduce_only":false,"margin_mode":"CROSS","child_orders":[{"algo_order_id":210,"root_algo_order_id":200,"algo_type":"POSITIONAL_TP_SL","algo_status":"NEW","child_orders":[{"algo_order_id":101,"root_algo_order_id":200,"algo_type":"STOP_LOSS","algo_status":"NEW","type":"CLOSE_POSITION","side":"SELL","trigger_price":49000,"quantity":0,"reduce_only":true},{"algo_order_id":102,"root_algo_order_id":200,"algo_type":"TAKE_PROFIT","algo_status":"NEW","type":"CLOSE_POSITION","side":"SELL","trigger_price":55000,"quantity":0,"reduce_only":true}]}]}]"#;
    let (mut e, socket) = ready_with(ORDER, bracket, POSITIONS);
    requests(&mut e);
    ws(
        &mut e,
        socket,
        r#"{"topic":"algoexecutionreportv2","ts":3000,"data":{"algoOrderId":200,"rootAlgoOrderId":200,"symbol":"PERP_BTC_USDC","algoType":"BRACKET","algoStatus":"FILLED","type":"LIMIT","side":"BUY","price":50000,"quantity":1,"seq":1}}"#,
    );
    assert_eq!(
        e.store()
            .orders()
            .iter()
            .find(|o| o.id == "200")
            .unwrap()
            .status,
        OrderStatus::Closed
    );
    for id in ["101", "102"] {
        assert_eq!(
            e.store()
                .orders()
                .iter()
                .find(|o| o.id == id)
                .unwrap()
                .status,
            OrderStatus::Open,
            "entry fill finalized a protective sibling"
        );
    }
    ws(
        &mut e,
        socket,
        r#"{"topic":"algoexecutionreportv2","ts":3001,"data":{"algoOrderId":210,"rootAlgoOrderId":200,"symbol":"PERP_BTC_USDC","algoType":"POSITIONAL_TP_SL","algoStatus":"FILLED","seq":2}}"#,
    );
    assert_eq!(
        e.store()
            .orders()
            .iter()
            .find(|o| o.id == "102")
            .unwrap()
            .status,
        OrderStatus::Open,
        "nested container completion finalized its BRACKET root"
    );
    ws(
        &mut e,
        socket,
        r#"{"topic":"algoexecutionreportv2","ts":3001,"data":{"algoOrderId":101,"rootAlgoOrderId":200,"symbol":"PERP_BTC_USDC","algoType":"STOP_LOSS","algoStatus":"FILLED","side":"SELL","seq":2}}"#,
    );
    assert_eq!(
        e.store()
            .orders()
            .iter()
            .find(|o| o.id == "101")
            .unwrap()
            .status,
        OrderStatus::Closed
    );
    assert_eq!(
        e.store()
            .orders()
            .iter()
            .find(|o| o.id == "102")
            .unwrap()
            .status,
        OrderStatus::Open
    );
    ws(
        &mut e,
        socket,
        r#"{"topic":"algoexecutionreportv2","ts":3002,"data":{"algoOrderId":102,"rootAlgoOrderId":200,"symbol":"PERP_BTC_USDC","algoType":"TAKE_PROFIT","algoStatus":"NEW","type":"CLOSE_POSITION","side":"SELL","triggerPrice":56000,"quantity":0,"seq":3}}"#,
    );
    let take = e.store().orders().iter().find(|o| o.id == "102").unwrap();
    assert_eq!(take.status, OrderStatus::Open);
    assert_eq!(
        take.price, 56000.0,
        "live child was blocked by an incorrect root tombstone"
    );
}

#[test]
fn private_native_mark_works_without_public_tickers_and_missing_marks_preserve_state() {
    let (mut e, socket) = ready();
    e.drain_actions().for_each(drop);
    e.store_mut().set_tickers(vec![]);
    ws(
        &mut e,
        socket,
        r#"{"topic":"position","ts":3000,"data":{"positions":[{"symbol":"PERP_BTC_USDC","positionQty":2,"averageOpenPrice":50000,"markPrice":52000,"marginMode":"CROSS","leverage":10,"seq":9007199254740993}]}}"#,
    );
    let native = e.store().position("BTCUSDC").unwrap();
    assert_eq!(
        (native.contracts, native.notional, native.unrealized_pnl),
        (2.0, 104000.0, 4000.0)
    );
    let before = e.store().positions().to_vec();
    e.drain_actions().for_each(drop);
    ws(
        &mut e,
        socket,
        r#"{"topic":"position","ts":4000,"data":{"positions":[{"symbol":"PERP_BTC_USDC","positionQty":3,"averageOpenPrice":50000,"marginMode":"CROSS","leverage":10,"seq":9007199254740994}]}}"#,
    );
    assert_eq!(e.store().positions(), before);
    assert!(e.drain_actions().any(|action| matches!(action,
        Action::Notice(safe_cex_engine::Notice::Log { message, .. }) if message.contains("position mark unavailable"))));
}

#[test]
fn owned_position_outside_partial_catalogue_is_not_an_empty_success() {
    let body = POSITIONS.replace("PERP_BTC_USDC", "PERP_MISSING_USDC");
    let (e, _) = ready_with(ORDER, "[]", &body);
    assert!(!e.store().loaded().positions);
}
