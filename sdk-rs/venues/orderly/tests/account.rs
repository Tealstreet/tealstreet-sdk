mod support;
use safe_cex_engine::{Action, Event, SocketId};
use support::*;

/// Native cadence of the venue's recurring account poll; policy scales it.
const POLL_MS: f64 = 2_000.0;

#[test]
fn trailing_display_uses_order_side_callback_value_or_rate_and_explicit_trigger() {
    for (side, fields, expected) in [
        (
            "SELL",
            r#""extreme_price":50000,"callback_value":500"#,
            49500.0,
        ),
        (
            "BUY",
            r#""extreme_price":50000,"callback_value":500"#,
            50500.0,
        ),
        (
            "SELL",
            r#""extremePrice":50000,"callbackRate":0.01"#,
            49500.0,
        ),
        (
            "BUY",
            r#""extremePrice":50000,"callbackRate":0.01"#,
            50500.0,
        ),
        (
            "BUY",
            r#""extreme_price":50000,"callback_value":500,"callback_rate":0.02"#,
            50500.0,
        ),
        (
            "SELL",
            r#""extreme_price":50000,"callback_value":500,"trigger_price":49000"#,
            49000.0,
        ),
        (
            "SELL",
            r#""extreme_price":50000,"callback_value":500,"is_activated":false"#,
            0.0,
        ),
        ("BUY", r#""activatedPrice":50000,"callbackValue":500"#, 0.0),
    ] {
        let algos = format!(
            r#"[{{"algo_order_id":301,"root_algo_order_id":301,"symbol":"PERP_BTC_USDC","algo_type":"TRAILING_STOP","algo_status":"NEW","side":"{side}","type":"MARKET","quantity":1,"margin_mode":"CROSS",{fields}}}]"#
        );
        let (mut e, socket) = ready_with(ORDER, &algos, POSITIONS);
        let trailing = e.store().orders().iter().find(|o| o.id == "301").unwrap();
        assert_eq!(trailing.price, expected, "{side} {fields}");
        // The stream uses camelCase, independently of REST's snake_case fields.
        let report = format!(
            r#"{{"topic":"algoexecutionreportv2","ts":3000,"data":{{"algoOrderId":301,"rootAlgoOrderId":301,"symbol":"PERP_BTC_USDC","algoType":"TRAILING_STOP","algoStatus":"NEW","side":"{side}","type":"MARKET","quantity":1,"marginMode":"CROSS","extremePrice":60000,"callbackValue":1000,"isActivated":true}}}}"#
        );
        ws(&mut e, socket, &report);
        let trailing = e.store().orders().iter().find(|o| o.id == "301").unwrap();
        assert_eq!(
            trailing.price,
            if side == "BUY" { 61000.0 } else { 59000.0 }
        );
    }
}

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
    // The provider signs at release: the stamp is the release clock (here the
    // fixtures' 2000), and the textsign starts with that same stamp.
    for r in requests(&mut e) {
        assert!(
            r.headers
                .iter()
                .any(|(k, v)| *k == "orderly-key" && v.starts_with("ed25519:"))
        );
        assert!(
            r.headers
                .iter()
                .any(|(k, v)| *k == "orderly-timestamp" && v == "2000")
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
    let mut e = engine(options());
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
fn steady_fills_cannot_starve_the_rest_only_balance() {
    let (mut e, s, mut tick) = ready_ticking(ORDER, "[]", POSITIONS);
    let mut read = requests(&mut e)
        .into_iter()
        .find(|r| r.url.ends_with("/v1/positions"))
        .unwrap();
    let prior = e.store().balance().clone();
    let stale =
        r#"{"success":true,"data":{"rows":[],"total_collateral_value":99,"free_collateral":1}}"#;
    for i in 0..3u32 {
        ws(
            &mut e,
            s,
            &format!(
                r#"{{"topic":"executionreport","ts":2000,"data":{{"orderId":9007199254740993,"symbol":"PERP_BTC_USDC","type":"LIMIT","side":"BUY","status":"PARTIAL_FILLED","price":50000,"quantity":1,"totalExecutedQuantity":0.{n},"executedQuantity":0.1,"executedPrice":50000,"tradeId":{t},"seq":{t}}}}}"#,
                n = i + 1,
                t = 60 + i
            ),
        );
        actions(&mut e);
        reply(&mut e, &read, 200, stale);
        let rereads: Vec<_> = requests(&mut e)
            .into_iter()
            .filter(|r| r.url.ends_with("/v1/positions"))
            .collect();
        // Positions keep their stream-maintained rows; only the balance commits.
        assert_eq!(e.store().positions().len(), 2);
        if i < 2 {
            assert_eq!(e.store().balance(), &prior);
            assert!(rereads.is_empty());
            e.handle(3000.0, Event::TimerFired { timer: tick });
            let mut next = None;
            for a in actions(&mut e) {
                match a {
                    Action::Http(r) if r.url.ends_with("/v1/positions") => next = Some(r),
                    Action::SetTimer { timer, .. } => tick = timer,
                    _ => {}
                }
            }
            read = next.unwrap();
        } else {
            assert_eq!(e.store().balance().total, 99.0);
            assert_eq!(e.store().balance().free, 1.0);
            assert_eq!(rereads.len(), 1);
        }
    }
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
        let mut e = engine(options());
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

fn private_connecting() -> (Driver, SocketId, Clock) {
    let mut e = engine(options());
    let mut clock = Clock::new(1000.0);
    e.start(1000.0);
    for _ in 0..10 {
        for action in clock.drain(&mut e) {
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
                    return (e, socket, clock);
                }
                _ => {}
            }
        }
    }
    panic!("private socket never connected");
}

fn closed(out: &[(f64, Action)], socket: SocketId) -> bool {
    out.iter()
        .any(|(_, a)| matches!(a, Action::WsClose { socket: s } if *s == socket))
}

fn private_connect(out: &[(f64, Action)]) -> Option<(f64, SocketId)> {
    out.iter().find_map(|(at, a)| match a {
        Action::WsConnect { socket, url, .. } if url.contains("private") => Some((*at, *socket)),
        _ => None,
    })
}

#[test]
fn private_initial_and_reconnect_handshakes_time_out_without_open_callbacks() {
    let (mut e, old, mut clock) = private_connecting();
    assert!(!closed(&clock.advance(&mut e, 10_999.0), old));
    assert!(closed(&clock.advance(&mut e, 11_000.0), old));
    let (reconnected, fresh) = private_connect(&clock.advance(&mut e, 12_000.0)).unwrap();
    assert!((11_750.0..=12_000.0).contains(&reconnected));
    assert_ne!(fresh, old);
    e.handle(clock.now, Event::WsOpened { socket: old });
    assert!(
        actions(&mut e).is_empty(),
        "late old opens cannot authenticate or replace the fresh deadline"
    );
    assert!(!closed(
        &clock.advance(&mut e, reconnected + 9_999.0),
        fresh
    ));
    assert!(closed(
        &clock.advance(&mut e, reconnected + 10_000.0),
        fresh
    ));
    let failed = clock.now;
    assert!(
        private_connect(&clock.advance(&mut e, failed + 1_499.0)).is_none(),
        "consecutive handshake failures back off"
    );
    assert!(private_connect(&clock.advance(&mut e, failed + 2_000.0)).is_some());
}

#[test]
fn private_open_replaces_handshake_timer_and_duplicate_open_cannot_resend_auth() {
    let (mut e, socket, mut clock) = private_connecting();
    clock.advance(&mut e, 2000.0);
    let out = clock.handle(&mut e, Event::WsOpened { socket });
    assert_eq!(
        out.iter()
            .filter(
                |a| matches!(a, Action::WsSend { text, .. } if text.contains("\"event\":\"auth\""))
            )
            .count(),
        1
    );
    clock.advance(&mut e, 3000.0);
    let out = clock.handle(
        &mut e,
        Event::WsMessage {
            socket,
            text: r#"{"event":"auth","success":true}"#,
        },
    );
    assert_eq!(
        out.iter()
            .filter(|a| matches!(a, Action::WsSend { text, .. } if text.contains("subscribe")))
            .count(),
        3
    );
    clock.advance(&mut e, 4000.0);
    assert!(clock.handle(&mut e, Event::WsOpened { socket }).is_empty());
    assert!(
        !closed(&clock.advance(&mut e, 11_500.0), socket),
        "the satisfied handshake deadline cannot close the authenticated socket"
    );
    let out = clock.handle(
        &mut e,
        Event::WsMessage {
            socket,
            text: r#"{"event":"auth","success":true}"#,
        },
    );
    assert!(
        out.is_empty(),
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
    assert!(e.store().loaded().orders);
    assert!(
        !e.store()
            .orders()
            .iter()
            .any(|o| o.id == "10" || o.id == "11"),
        "unknown working descendant must skip its whole root"
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

// Drive the actual account loop through slow/fast (1s floor) updates, a parked
// stale callback, and a resumed read. Other timers are deliberately not fired.
fn policy_poll_read<V: safe_cex_engine::Venue>(
    engine: &mut safe_cex_engine::Engine<V>,
    now: f64,
    endpoint: &str,
) -> (f64, Vec<safe_cex_engine::HttpRequest>) {
    use safe_cex_engine::{Action, Event};
    engine.drain_actions().for_each(drop);
    engine.handle(
        now,
        Event::PollScaleChanged {
            scale: 60_000.0 / POLL_MS,
        },
    );
    let slow: Vec<_> = engine
        .drain_actions()
        .filter_map(|a| match a {
            Action::SetTimer { timer, at_ms } => Some((timer, at_ms)),
            Action::Http(_) => panic!("policy updates must not issue account reads"),
            _ => None,
        })
        .collect();
    assert_eq!(slow.len(), 1, "exactly one recurring account poll");
    assert_eq!(slow[0].1, now + 60_000.0);
    engine.handle(
        now + 1.0,
        Event::PollScaleChanged {
            scale: 1000.0 / POLL_MS,
        },
    );
    let fast: Vec<_> = engine
        .drain_actions()
        .filter_map(|a| match a {
            Action::SetTimer { timer, at_ms } => Some((timer, at_ms)),
            _ => None,
        })
        .collect();
    assert_eq!(fast.len(), 1);
    assert_eq!(fast[0].1, now + 1001.0);
    engine.handle(
        now + 2.0,
        Event::PollScaleChanged {
            scale: f64::INFINITY,
        },
    );
    let paused: Vec<_> = engine.drain_actions().collect();
    assert!(
        paused
            .iter()
            .any(|a| matches!(a, Action::ClearTimer { timer } if *timer == fast[0].0))
    );
    assert!(!paused.iter().any(|a| matches!(a, Action::SetTimer { .. })));
    engine.handle(now + 1001.0, Event::TimerFired { timer: fast[0].0 });
    assert!(!engine.drain_actions().any(|a| matches!(a, Action::Http(_))));
    engine.handle(
        now + 90_000.0,
        Event::PollScaleChanged {
            scale: 1000.0 / POLL_MS,
        },
    );
    let resumed: Vec<_> = engine
        .drain_actions()
        .filter_map(|a| match a {
            Action::SetTimer { timer, at_ms } => Some((timer, at_ms)),
            _ => None,
        })
        .collect();
    assert_eq!(resumed.len(), 1);
    assert_eq!(resumed[0].1, now + 91_000.0);
    engine.handle(
        now + 91_000.0,
        Event::TimerFired {
            timer: resumed[0].0,
        },
    );
    // Signed reads are released through the provider's dispatch.
    let reads: Vec<_> = safe_cex_engine::test_support::prepared_actions(engine, now + 91_000.0)
        .into_iter()
        .filter_map(|a| match a {
            Action::Http(request) => Some(request),
            _ => None,
        })
        .collect();
    assert!(
        reads.iter().any(|r| r.url.contains(endpoint)),
        "actual resumed account HTTP: {reads:?}"
    );
    let completed_at = now + 91_001.0;
    engine.handle(
        completed_at,
        Event::PollScaleChanged {
            scale: f64::INFINITY,
        },
    );
    assert!(
        !engine
            .drain_actions()
            .any(|a| matches!(a, Action::SetTimer { .. }))
    );
    (completed_at, reads)
}

fn policy_poll_after_completion<V: safe_cex_engine::Venue>(
    engine: &mut safe_cex_engine::Engine<V>,
    now: f64,
) {
    use safe_cex_engine::{Action, Event};
    // A response may schedule an ordinary repair/deadline, but the parked poll
    // itself must still be registered and adopt this newest policy.
    engine.drain_actions().for_each(drop);
    engine.handle(
        now,
        Event::PollScaleChanged {
            scale: 30_000.0 / POLL_MS,
        },
    );
    let times: Vec<_> = engine
        .drain_actions()
        .filter_map(|a| match a {
            Action::SetTimer { at_ms, .. } => Some(at_ms),
            _ => None,
        })
        .collect();
    assert_eq!(times, vec![now + 30_000.0]);
}

#[test]
fn recurring_account_poll_obeys_live_policy_and_completion() {
    let (mut e, _) = ready();
    // Authentication admits a fresh snapshot independently of recurring polling.
    for _ in 0..12 {
        let initial = requests(&mut e);
        if initial.is_empty() {
            break;
        }
        for r in initial {
            let body = if r.url.contains("/positions") {
                POSITIONS
            } else {
                r#"{"success":true,"data":{"rows":[],"meta":{"total":0}}}"#
            };
            e.handle(
                2000.0,
                Event::HttpResponse {
                    request: r.id,
                    response: safe_cex_engine::Response {
                        status: 200,
                        headers: "",
                        body,
                    },
                },
            );
        }
    }
    let engine = &mut e;
    let (now, mut reads) = policy_poll_read(engine, 2000.0, "/v1/positions");
    for _ in 0..12 {
        if reads.is_empty() {
            break;
        }
        for r in reads {
            let body = if r.url.contains("/positions") {
                POSITIONS
            } else {
                r#"{"success":true,"data":{"rows":[],"meta":{"total":0}}}"#
            };
            engine.handle(
                now,
                safe_cex_engine::Event::HttpResponse {
                    request: r.id,
                    response: safe_cex_engine::Response {
                        status: 200,
                        headers: "",
                        body,
                    },
                },
            );
        }
        reads = engine
            .drain_actions()
            .filter_map(|a| match a {
                safe_cex_engine::Action::Http(r) => Some(r),
                _ => None,
            })
            .collect();
    }
    assert!(reads.is_empty(), "account cycle must complete");
    policy_poll_after_completion(engine, now);
}

#[test]
fn host_poll_scale_cannot_drive_account_reads_below_one_second() {
    use safe_cex_engine::Action;
    let (mut e, _) = ready();
    for _ in 0..12 {
        let initial = requests(&mut e);
        if initial.is_empty() {
            break;
        }
        for r in initial {
            let body = if r.url.contains("/positions") {
                POSITIONS
            } else {
                r#"{"success":true,"data":{"rows":[],"meta":{"total":0}}}"#
            };
            e.handle(
                2000.0,
                Event::HttpResponse {
                    request: r.id,
                    response: safe_cex_engine::Response {
                        status: 200,
                        headers: "",
                        body,
                    },
                },
            );
        }
    }
    e.drain_actions().for_each(drop);
    e.handle(2000.0, Event::PollScaleChanged { scale: 0.05 });
    let armed: Vec<_> = e
        .drain_actions()
        .filter_map(|a| match a {
            Action::SetTimer { at_ms, .. } => Some(at_ms),
            _ => None,
        })
        .collect();
    assert_eq!(armed, vec![3000.0], "a 100ms scaled cadence is held at 1s");
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
        Action::Notice(safe_cex_engine::Notice::Event(safe_cex_engine::LogEvent { message, .. })) if message.contains("position mark unavailable"))));
}

#[test]
fn owned_position_outside_the_catalogue_is_skipped_and_values_the_balance_only_with_its_pnl() {
    for marked in [true, false] {
        let mut body = POSITIONS.replace("PERP_BTC_USDC", "PERP_MISSING_USDC");
        if !marked {
            body = body.replace(r#""mark_price":51000,"#, "");
        }
        let (e, _) = ready_with(ORDER, "[]", &body);
        assert!(e.store().loaded().positions);
        assert!(e.store().positions().is_empty());
        assert_eq!(e.store().loaded().balance, marked);
        if marked {
            let balance = e.store().balance();
            assert_eq!(
                (balance.upnl, balance.total, balance.free),
                (1500.0, 8500.0, 7000.0)
            );
        }
    }
}

#[test]
fn unknown_working_order_is_skipped_with_a_warning_and_terminal_and_spot_rows_silently() {
    for (symbol, status, warned) in [
        ("PERP_MISSING_USDC", "NEW", true),
        ("PERP_MISSING_USDC", "FILLED", false),
        ("SPOT_MISSING_USDC", "NEW", false),
    ] {
        let order = ORDER
            .replace("PERP_BTC_USDC", symbol)
            .replace("NEW", status);
        let (e, _, logs) = ready_with_logs(&order, "[]", POSITIONS);
        assert!(e.store().loaded().orders, "{symbol} {status}");
        assert!(e.store().orders().is_empty(), "{symbol} {status}");
        assert_eq!(
            logs.iter()
                .any(|m| m == "Orderly skipped order 9007199254740993 on an unknown market"),
            warned,
            "{symbol} {status}: {logs:?}"
        );
    }
}

#[test]
fn bad_snapshot_rows_keep_their_stored_copies_until_a_valid_snapshot_omits_them() {
    let (mut e, s) = ready();
    let order = |id: u32, symbol: &str, price: &str| {
        format!(
            r#"{{"order_id":{id},"symbol":"{symbol}","type":"LIMIT","side":"BUY","status":"NEW","price":{price},"quantity":1,"total_executed_quantity":0,"executed":0,"visible_quantity":1,"reduce_only":false,"margin_mode":"CROSS"}}"#
        )
    };
    let position = |symbol: &str, qty: &str| {
        format!(
            r#"{{"symbol":"{symbol}","position_qty":{qty},"average_open_price":50000,"mark_price":51000,"margin_mode":"CROSS","leverage":10}}"#
        )
    };
    // Bootstrap leaves its polls pending; later rounds reconnect and re-authenticate to poll.
    let mut socket = s;
    let mut resync = |e: &mut Driver, orders: &[String], positions: &[String], collateral: u32| {
        let mut pending = requests(e);
        if pending.is_empty() {
            e.handle(3000.0, Event::WsClosed { socket, code: 1006 });
            let retry = actions(e)
                .into_iter()
                .find_map(|a| match a {
                    Action::SetTimer { timer, .. } => Some(timer),
                    _ => None,
                })
                .unwrap();
            e.handle(4000.0, Event::TimerFired { timer: retry });
            socket = actions(e)
                .into_iter()
                .find_map(|a| match a {
                    Action::WsConnect { socket, url, .. } if url.contains("private") => {
                        Some(socket)
                    }
                    _ => None,
                })
                .unwrap();
            e.handle(4000.0, Event::WsOpened { socket });
            actions(e);
            e.handle(
                4000.0,
                Event::WsMessage {
                    socket,
                    text: r#"{"event":"auth","success":true}"#,
                },
            );
            pending = requests(e);
        }
        let orders = format!(
            r#"{{"success":true,"data":{{"rows":[{}],"meta":{{"total":{}}}}}}}"#,
            orders.join(","),
            orders.len()
        );
        let positions = format!(
            r#"{{"success":true,"data":{{"rows":[{}],"total_collateral_value":{collateral},"free_collateral":7000}}}}"#,
            positions.join(",")
        );
        for r in pending {
            let body = if r.url.contains("/v1/positions") {
                positions.as_str()
            } else if r.url.contains("/v1/algo/orders") {
                r#"{"success":true,"data":{"rows":[],"meta":{"total":0}}}"#
            } else {
                orders.as_str()
            };
            reply(e, &r, 200, body);
        }
        actions(e)
            .into_iter()
            .filter_map(|a| match a {
                Action::Notice(safe_cex_engine::Notice::Event(safe_cex_engine::LogEvent {
                    message,
                    ..
                })) => Some(message),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    let size = |e: &Driver, symbol: &str| {
        e.store()
            .positions()
            .iter()
            .find(|p| p.symbol == symbol)
            .map(|p| p.contracts)
    };
    let price = |e: &Driver, id: &str| e.store().order(id).map(|o| o.price);
    resync(
        &mut e,
        &[
            order(1001, "PERP_BTC_USDC", "50000"),
            order(1002, "PERP_BTC_USDC", "49000"),
        ],
        &[
            position("PERP_BTC_USDC", "1"),
            position("PERP_ETH_USDC", "2"),
        ],
        10000,
    );
    assert_eq!(price(&e, "1002"), Some(49000.0));
    assert_eq!(size(&e, "ETHUSDC"), Some(2.0));
    let balance = e.store().balance().clone();
    let logs = resync(
        &mut e,
        &[
            order(1001, "PERP_BTC_USDC", "50100"),
            order(1002, "PERP_BTC_USDC", "\"bad\""),
            order(1003, "PERP_NOPE_USDC", "1"),
        ],
        &[
            position("PERP_BTC_USDC", "3"),
            position("PERP_ETH_USDC", "\"bad\""),
            position("PERP_NOPE_USDC_broker", "1"),
            position("PERP_ZZZ_USDC", "0"),
        ],
        14000,
    );
    assert_eq!(price(&e, "1001"), Some(50100.0));
    assert_eq!(price(&e, "1002"), Some(49000.0));
    assert_eq!(price(&e, "1003"), None);
    assert_eq!(size(&e, "BTCUSDC"), Some(3.0));
    assert_eq!(size(&e, "ETHUSDC"), Some(2.0));
    // BTC 3000 + retained ETH copy 2000 + unknown-market NOPE row 1000.
    let partial = e.store().balance();
    assert_eq!(
        (partial.upnl, partial.total, partial.free, partial.used),
        (6000.0, 8000.0, 7000.0, 1000.0)
    );
    for expected in [
        "Orderly skipped unmappable order 1002",
        "Orderly skipped order 1003 on an unknown market",
        "Orderly skipped unmappable position ETHUSDC",
        "Orderly skipped unmappable position NOPEUSDC_broker",
    ] {
        assert!(logs.iter().any(|m| m == expected), "{expected}: {logs:?}");
    }
    assert!(!logs.iter().any(|m| m.contains("bad") || m.contains("ZZZ")));
    resync(
        &mut e,
        &[order(1001, "PERP_BTC_USDC", "50100")],
        &[position("PERP_BTC_USDC", "3")],
        12000,
    );
    assert_eq!(price(&e, "1002"), None);
    assert_eq!(size(&e, "ETHUSDC"), None);
    assert_eq!(e.store().positions().len(), 1);
    assert_ne!(e.store().balance(), &balance);
}

#[test]
fn skipped_position_rows_keep_the_balance_updating() {
    let positions = |rows: &str| {
        format!(
            r#"{{"success":true,"data":{{"rows":[{rows}],"total_collateral_value":9000,"free_collateral":8000}}}}"#
        )
    };
    // A flat row that fails mapping closes its symbol, whatever failed.
    for flat in [
        r#"{"symbol":"PERP_BTC_USDC","position_qty":0,"margin_mode":"WEIRD"}"#,
        r#"{"symbol":"PERP_BTC_USDC","position_qty":0,"leverage":"bad"}"#,
    ] {
        let (mut e, _) = ready();
        assert_eq!(e.store().positions().len(), 2);
        let poll = requests(&mut e)
            .into_iter()
            .find(|r| r.url.contains("/v1/positions"))
            .unwrap();
        reply(&mut e, &poll, 200, &positions(flat));
        assert!(e.store().positions().is_empty(), "{flat}");
        let balance = e.store().balance();
        assert_eq!(
            (balance.upnl, balance.total, balance.free),
            (0.0, 9000.0, 8000.0),
            "{flat}"
        );
    }
    // A stale read still values collateral when the skipped row has a stored copy.
    let (mut e, s, mut tick) = ready_ticking(ORDER, "[]", POSITIONS);
    let mut poll = requests(&mut e)
        .into_iter()
        .find(|r| r.url.ends_with("/v1/positions"))
        .unwrap();
    // The guard admits a stale read after two discarded ones.
    for seq in 0..3u64 {
        ws(
            &mut e,
            s,
            &format!(
                r#"{{"topic":"position","ts":{},"data":{{"positions":[{{"symbol":"PERP_BTC_USDC","positionQty":2,"averageOpenPrice":50000,"markPrice":51000,"marginMode":"CROSS","leverage":10,"seq":{}}}]}}}}"#,
                2000 + seq,
                9007199254740993 + seq
            ),
        );
        reply(
            &mut e,
            &poll,
            200,
            &positions(
                r#"{"symbol":"PERP_BTC_USDC","position_qty":1,"average_open_price":50000,"mark_price":51000,"margin_mode":"WEIRD"}"#,
            ),
        );
        if seq < 2 {
            e.handle(3000.0, Event::TimerFired { timer: tick });
            let mut next = None;
            for a in actions(&mut e) {
                match a {
                    Action::Http(r) if r.url.ends_with("/v1/positions") => next = Some(r),
                    Action::SetTimer { timer, .. } => tick = timer,
                    _ => {}
                }
            }
            poll = next.unwrap();
        }
    }
    assert_eq!(e.store().position("BTCUSDC").unwrap().contracts, 2.0);
    let balance = e.store().balance();
    assert_eq!(
        (balance.upnl, balance.total, balance.free),
        (2500.0, 6500.0, 8000.0)
    );
}

fn skipped_rows(out: &[Action]) -> usize {
    out.iter()
        .filter(|a| {
            matches!(a, Action::Notice(safe_cex_engine::Notice::Event(event))
            if event.code == safe_cex_engine::log::code::ROW_SKIPPED)
        })
        .count()
}

#[test]
fn ws_admission_positions_isolates_bad_rows_in_every_array_slot() {
    for bad in [
        r#"{"symbol":"PERP_BTC_USDC","positionQty":"broken","marginMode":"ISOLATED","seq":100}"#,
        r#"{"symbol":"PERP_BTC_USDC","positionQty":3,"marginMode":"INVALID","seq":100}"#,
        r#"{"positionQty":3,"marginMode":"CROSS","seq":100}"#,
    ] {
        for slot in 0..3 {
            let (mut e, socket) = ready();
            actions(&mut e);
            let isolated = e
                .store()
                .positions()
                .iter()
                .find(|p| p.id == "BTCUSDC:isolated")
                .unwrap()
                .clone();
            let mut rows = vec![
                r#"{"symbol":"PERP_BTC_USDC","positionQty":2,"averageOpenPrice":50000,"markPrice":51000,"marginMode":"CROSS","seq":10}"#,
                r#"{"symbol":"PERP_ETH_USDC","positionQty":3,"averageOpenPrice":2000,"markPrice":2100,"marginMode":"CROSS","seq":10}"#,
            ];
            rows.insert(slot, bad);
            ws(
                &mut e,
                socket,
                &format!(
                    r#"{{"topic":"position","ts":3000,"data":{{"positions":[{}]}}}}"#,
                    rows.join(",")
                ),
            );
            assert_eq!(
                e.store()
                    .positions()
                    .iter()
                    .find(|p| p.id == "ETHUSDC")
                    .map(|p| p.contracts),
                Some(3.0),
                "slot{slot}: {bad}"
            );
            assert_eq!(
                e.store()
                    .positions()
                    .iter()
                    .find(|p| p.id == "BTCUSDC")
                    .unwrap()
                    .contracts,
                2.0
            );
            assert_eq!(
                e.store()
                    .positions()
                    .iter()
                    .find(|p| p.id == "BTCUSDC:isolated")
                    .unwrap(),
                &isolated
            );
            assert_eq!(skipped_rows(&actions(&mut e)), 1);
            assert!(fill_amounts(&mut e).is_empty());
        }
    }
}

#[test]
fn ws_admission_executions_isolates_bad_rows_in_every_array_slot() {
    for algo in [false, true] {
        let topic = if algo {
            "algoexecutionreportv2"
        } else {
            "executionreport"
        };
        let id = if algo { "algoOrderId" } else { "orderId" };
        let fields = if algo {
            r#", "algoType":"STOP_LOSS","triggerPrice":49000"#
        } else {
            ""
        };
        let good = |n, price| {
            format!(
                r#"{{"{id}":{n},"symbol":"PERP_BTC_USDC","type":"LIMIT","side":"BUY","status":"NEW","price":{price},"quantity":1,"seq":10{fields}}}"#
            )
        };
        for bad in [
            format!(r#"{{"{id}":3,"quantity":"broken"}}"#),
            good(3, 50000).replace("BUY", "INVALID"),
        ] {
            for slot in 0..3 {
                let (mut e, socket) = ready();
                actions(&mut e);
                let mut rows = vec![good(1, 50001), good(2, 50002)];
                rows.insert(slot, bad.clone());
                ws(
                    &mut e,
                    socket,
                    &format!(
                        r#"{{"topic":"{topic}","ts":3000,"data":[{}]}}"#,
                        rows.join(",")
                    ),
                );
                for n in ["1", "2"] {
                    assert!(
                        e.store().orders().iter().any(|o| o.id == n),
                        "{topic} slot{slot}: {bad}"
                    );
                }
                assert!(!e.store().orders().iter().any(|o| o.id == "3"));
                assert_eq!(skipped_rows(&actions(&mut e)), 1);
                assert!(fill_amounts(&mut e).is_empty());
            }
        }
    }
}

#[test]
fn ws_admission_invalid_terminal_root_cannot_poison_child_or_lower_sequence() {
    let algos = r#"[{"algo_order_id":100,"root_algo_order_id":100,"symbol":"PERP_BTC_USDC","algo_type":"POSITIONAL_TP_SL","algo_status":"NEW","margin_mode":"CROSS","child_orders":[{"algo_order_id":101,"root_algo_order_id":100,"algo_type":"STOP_LOSS","side":"SELL","algo_status":"NEW","trigger_price":49000,"quantity":1},{"algo_order_id":102,"root_algo_order_id":100,"algo_type":"TAKE_PROFIT","side":"SELL","algo_status":"NEW","trigger_price":55000,"quantity":1}]}]"#;
    let (mut e, socket) = ready_with(ORDER, algos, POSITIONS);
    actions(&mut e);
    let before = e.store().orders().to_vec();
    ws(
        &mut e,
        socket,
        r#"{"topic":"algoexecutionreportv2","ts":9000,"data":{"algoOrderId":101,"rootAlgoOrderId":100,"symbol":"PERP_BTC_USDC","algoType":"STOP_LOSS","algoStatus":"CANCELLED","rootAlgoStatus":"CANCELLED","side":"INVALID","seq":100}}"#,
    );
    assert_eq!(
        e.store().orders(),
        before.as_slice(),
        "rejected leaf changed its root siblings"
    );
    assert_eq!(skipped_rows(&actions(&mut e)), 1);
    ws(
        &mut e,
        socket,
        r#"{"topic":"algoexecutionreportv2","ts":3000,"data":{"algoOrderId":102,"rootAlgoOrderId":100,"symbol":"PERP_BTC_USDC","algoType":"TAKE_PROFIT","algoStatus":"NEW","side":"SELL","triggerPrice":56000,"quantity":1,"seq":10}}"#,
    );
    let peer = e.store().orders().iter().find(|o| o.id == "102").unwrap();
    assert_eq!(peer.status, safe_cex_engine::model::OrderStatus::Open);
    assert_eq!(
        peer.price, 56000.0,
        "rejected root installed a terminal tombstone"
    );
    assert!(fill_amounts(&mut e).is_empty());
}

#[test]
fn ws_admission_rejected_execution_price_does_not_consume_cumulative_fill() {
    let (mut e, socket) = ready();
    actions(&mut e);
    e.take_frame(2000.0);
    ws(
        &mut e,
        socket,
        r#"{"topic":"executionreport","ts":3000,"data":{"orderId":9007199254740993,"symbol":"PERP_BTC_USDC","type":"LIMIT","side":"BUY","status":"PARTIAL_FILLED","price":50000,"quantity":1,"totalExecutedQuantity":0.25,"averageExecutedPrice":50000,"executedPrice":-1,"seq":10}}"#,
    );
    assert!(fill_amounts(&mut e).is_empty());
    ws(
        &mut e,
        socket,
        r#"{"topic":"executionreport","ts":3001,"data":{"orderId":9007199254740993,"symbol":"PERP_BTC_USDC","type":"LIMIT","side":"BUY","status":"PARTIAL_FILLED","price":50000,"quantity":1,"totalExecutedQuantity":0.25,"averageExecutedPrice":50000,"seq":11}}"#,
    );
    assert_eq!(
        fill_amounts(&mut e),
        [0.25],
        "rejected price consumed cumulative fill watermark"
    );
}

#[test]
fn ws_admission_same_id_valid_and_invalid_rows_follow_native_order() {
    for bad_first in [false, true] {
        let (mut e, socket) = ready();
        actions(&mut e);
        let good = r#"{"orderId":9007199254740993,"symbol":"PERP_BTC_USDC","type":"LIMIT","side":"BUY","status":"NEW","price":51000,"quantity":1,"seq":10}"#;
        let bad = good.replace("BUY", "INVALID").replace("51000", "52000");
        let rows = if bad_first {
            format!("{bad},{good}")
        } else {
            format!("{good},{bad}")
        };
        ws(
            &mut e,
            socket,
            &format!(r#"{{"topic":"executionreport","ts":3000,"data":[{rows}]}}"#),
        );
        assert_eq!(e.store().orders()[0].price, 51000.0);
        assert_eq!(skipped_rows(&actions(&mut e)), 1);
        assert!(fill_amounts(&mut e).is_empty());
    }
}

#[test]
fn ws_admission_structural_errors_are_inert_before_the_first_row() {
    let good = r#"{"orderId":9007199254740993,"symbol":"PERP_BTC_USDC","type":"LIMIT","side":"BUY","status":"NEW","price":51000,"quantity":1,"seq":10}"#;
    let position = r#"{"symbol":"PERP_BTC_USDC","positionQty":3,"marginMode":"CROSS","seq":10}"#;
    for frame in [
        format!(r#"{{"topic":"executionreport","ts":3000,"data":[{good},"#),
        format!(r#"{{"topic":"executionreport","topic":"executionreport","data":[{good}]}}"#),
        format!(
            r#"{{"topic":"position","ts":3000,"data":{{"positions":[{position}],"positions":[]}}}}"#
        ),
        format!(r#"{{"topic":"position","ts":3000,"data":{{"positions":{{"row":{position}}}}}}}"#),
        r#"{"topic":"executionreport","data":7}"#.into(),
    ] {
        let (mut e, socket) = ready();
        actions(&mut e);
        let orders = e.store().orders().to_vec();
        let positions = e.store().positions().to_vec();
        ws(&mut e, socket, &frame);
        assert_eq!(e.store().orders(), orders.as_slice(), "{frame}");
        assert_eq!(e.store().positions(), positions.as_slice(), "{frame}");
        assert_eq!(
            skipped_rows(&actions(&mut e)),
            0,
            "structural failure became a row diagnostic"
        );
        assert!(fill_amounts(&mut e).is_empty());
    }
}

#[test]
fn ws_admission_bad_nested_root_does_not_commit_its_valid_child() {
    let (mut e, socket) = ready();
    actions(&mut e);
    ws(
        &mut e,
        socket,
        r#"{"topic":"algoexecutionreportv2","ts":3000,"data":{"algoOrderId":100,"rootAlgoOrderId":100,"symbol":"PERP_BTC_USDC","algoType":"POSITIONAL_TP_SL","algoStatus":"NEW","childOrders":[{"algoOrderId":101,"rootAlgoOrderId":100,"algoType":"STOP_LOSS","side":"SELL","algoStatus":"NEW","triggerPrice":49000,"quantity":1},{"algoOrderId":102,"rootAlgoOrderId":100,"algoType":"TAKE_PROFIT","side":"INVALID","algoStatus":"NEW","triggerPrice":55000,"quantity":1}]}}"#,
    );
    assert!(
        !e.store()
            .orders()
            .iter()
            .any(|o| o.id == "101" || o.id == "102")
    );
    assert_eq!(skipped_rows(&actions(&mut e)), 1);
    assert!(fill_amounts(&mut e).is_empty());
}

#[test]
fn ws_admission_all_supplied_execution_economics_validate_before_terminal_effects() {
    for field in [
        "\"executedQuantity\":-0.25",
        "\"totalExecutedQuantity\":-0.25",
        "\"executedPrice\":-1",
        "\"averageExecutedPrice\":-1",
    ] {
        let algos = r#"[{"algo_order_id":100,"root_algo_order_id":100,"symbol":"PERP_BTC_USDC","algo_type":"POSITIONAL_TP_SL","algo_status":"NEW","child_orders":[{"algo_order_id":101,"root_algo_order_id":100,"algo_type":"STOP_LOSS","side":"SELL","algo_status":"NEW","trigger_price":49000,"quantity":1}]}]"#;
        let (mut e, socket) = ready_with(ORDER, algos, POSITIONS);
        actions(&mut e);
        let before = e.store().orders().to_vec();
        ws(
            &mut e,
            socket,
            &format!(
                r#"{{"topic":"algoexecutionreportv2","ts":9000,"data":{{"algoOrderId":100,"rootAlgoOrderId":100,"symbol":"PERP_BTC_USDC","algoType":"POSITIONAL_TP_SL","algoStatus":"CANCELLED","seq":100,{field}}}}}"#
            ),
        );
        assert_eq!(e.store().orders(), before.as_slice(), "{field}");
        assert_eq!(skipped_rows(&actions(&mut e)), 1);
        assert!(fill_amounts(&mut e).is_empty());
    }
}

#[test]
fn ws_admission_root_only_symbol_must_match_its_cached_descendants() {
    let algos = r#"[{"algo_order_id":100,"root_algo_order_id":100,"symbol":"PERP_BTC_USDC","algo_type":"POSITIONAL_TP_SL","algo_status":"NEW","child_orders":[{"algo_order_id":101,"root_algo_order_id":100,"algo_type":"STOP_LOSS","side":"SELL","algo_status":"NEW","trigger_price":49000,"quantity":1}]}]"#;
    for symbol in ["PERP_ETH_USDC", "broken"] {
        let (mut e, socket) = ready_with(ORDER, algos, POSITIONS);
        actions(&mut e);
        let before = e.store().orders().to_vec();
        ws(
            &mut e,
            socket,
            &format!(
                r#"{{"topic":"algoexecutionreportv2","ts":9000,"data":{{"algoOrderId":100,"rootAlgoOrderId":100,"symbol":"{symbol}","algoType":"POSITIONAL_TP_SL","algoStatus":"CANCELLED","seq":100}}}}"#
            ),
        );
        assert_eq!(
            e.store().orders(),
            before.as_slice(),
            "wrong-symbol root canceled cached descendants"
        );
        assert_eq!(skipped_rows(&actions(&mut e)), 1);
        // Missing optional symbol remains a valid native root-only control.
        ws(
            &mut e,
            socket,
            r#"{"topic":"algoexecutionreportv2","ts":3000,"data":{"algoOrderId":100,"rootAlgoOrderId":100,"algoType":"POSITIONAL_TP_SL","algoStatus":"CANCELLED","seq":10}}"#,
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
}

#[test]
fn ws_admission_nested_child_cannot_change_its_inherited_root() {
    let algos = r#"[{"algo_order_id":200,"root_algo_order_id":200,"symbol":"PERP_BTC_USDC","algo_type":"POSITIONAL_TP_SL","algo_status":"NEW","child_orders":[{"algo_order_id":201,"root_algo_order_id":200,"algo_type":"STOP_LOSS","side":"SELL","algo_status":"NEW","trigger_price":49000,"quantity":1}]}]"#;
    let (mut e, socket) = ready_with(ORDER, algos, POSITIONS);
    actions(&mut e);
    let before = e.store().orders().to_vec();
    ws(
        &mut e,
        socket,
        r#"{"topic":"algoexecutionreportv2","ts":9000,"data":{"algoOrderId":100,"rootAlgoOrderId":100,"symbol":"PERP_BTC_USDC","algoType":"POSITIONAL_TP_SL","algoStatus":"NEW","seq":100,"childOrders":[{"algoOrderId":201,"algoType":"STOP_LOSS","side":"SELL","algoStatus":"NEW","triggerPrice":48000,"quantity":1}]}}"#,
    );
    assert_eq!(
        e.store().orders(),
        before.as_slice(),
        "cached child moved to an inherited foreign root"
    );
    assert_eq!(skipped_rows(&actions(&mut e)), 1);
}

#[test]
fn ws_admission_late_unique_fill_survives_the_order_display_fence() {
    let (mut e, socket) = ready();
    actions(&mut e);
    e.take_frame(2000.0);
    ws(
        &mut e,
        socket,
        r#"{"topic":"executionreport","ts":4000,"data":{"orderId":9007199254740993,"symbol":"PERP_BTC_USDC","type":"LIMIT","side":"BUY","status":"NEW","price":51000,"quantity":1,"seq":20}}"#,
    );
    let late = r#"{"topic":"executionreport","ts":3000,"data":{"orderId":9007199254740993,"symbol":"PERP_BTC_USDC","type":"LIMIT","side":"BUY","status":"PARTIAL_FILLED","price":50000,"quantity":1,"totalExecutedQuantity":0.25,"executedQuantity":0.25,"executedPrice":50001,"tradeId":12,"seq":10}}"#;
    ws(&mut e, socket, late);
    assert_eq!(fill_amounts(&mut e), [0.25]);
    assert_eq!(e.store().orders()[0].price, 51000.0);
    assert_eq!(e.store().orders()[0].filled, 0.0);
    ws(&mut e, socket, late);
    assert!(fill_amounts(&mut e).is_empty());
}

#[test]
fn ws_admission_sparse_terminal_retains_its_separate_execution_stamp_gate() {
    let algos = r#"[{"algo_order_id":101,"root_algo_order_id":100,"symbol":"PERP_BTC_USDC","algo_type":"STOP_LOSS","side":"SELL","algo_status":"NEW","trigger_price":49000,"quantity":1}]"#;
    let (mut e, socket) = ready_with(ORDER, algos, POSITIONS);
    actions(&mut e);
    e.take_frame(2000.0);
    ws(
        &mut e,
        socket,
        r#"{"topic":"algoexecutionreportv2","ts":4000,"data":{"algoOrderId":101,"rootAlgoOrderId":100,"symbol":"PERP_BTC_USDC","algoType":"STOP_LOSS","side":"SELL","algoStatus":"NEW","triggerPrice":50000,"quantity":1,"seq":20}}"#,
    );
    ws(
        &mut e,
        socket,
        r#"{"topic":"algoexecutionreportv2","ts":3000,"data":{"algoOrderId":101,"rootAlgoOrderId":100,"symbol":"PERP_BTC_USDC","algoType":"STOP_LOSS","side":"SELL","algoStatus":"FILLED","totalExecutedQuantity":0.25,"executedQuantity":0.25,"executedPrice":49000,"seq":10}}"#,
    );
    let order = e.store().orders().iter().find(|o| o.id == "101").unwrap();
    assert_eq!(order.status, safe_cex_engine::model::OrderStatus::Open);
    assert_eq!(order.price, 50000.0);
    assert!(fill_amounts(&mut e).is_empty());
}
