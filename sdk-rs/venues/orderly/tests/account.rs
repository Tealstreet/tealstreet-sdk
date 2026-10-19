mod support;
use safe_cex_engine::{Action, Engine};
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
