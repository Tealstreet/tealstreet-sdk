mod support;
use safe_cex_engine::SocketId;
use support::*;

fn fresh() -> (Driver, SocketId) {
    let (mut engine, socket) = ready();
    actions(&mut engine);
    engine.take_frame(1000.0);
    (engine, socket)
}

/// Decode public Fill price and amount, rather than inspecting private totals.
fn fills(engine: &mut Driver) -> Vec<(f64, f64)> {
    let Some(bytes) = engine.take_frame(3000.0) else {
        return vec![];
    };
    let mut at = 16;
    for _ in 0..u16::from_le_bytes(bytes[6..8].try_into().unwrap()) {
        let kind = u16::from_le_bytes(bytes[at..at + 2].try_into().unwrap());
        let len = u32::from_le_bytes(bytes[at + 4..at + 8].try_into().unwrap()) as usize;
        let start = at + 8;
        if kind == 11 {
            let count = u32::from_le_bytes(bytes[start..start + 4].try_into().unwrap()) as usize;
            let data = (start + 4 + count * 12 + 7) & !7;
            return (0..count)
                .map(|i| {
                    let offset = data + i * 16;
                    (
                        f64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap()),
                        f64::from_le_bytes(bytes[offset + 8..offset + 16].try_into().unwrap()),
                    )
                })
                .collect();
        }
        at = start + len;
    }
    vec![]
}

fn report(engine: &mut Driver, socket: SocketId, seq: u64, fields: &str) -> Vec<(f64, f64)> {
    ws(
        engine,
        socket,
        &format!(
            r#"{{"topic":"executionreport","ts":3000,"data":{{"orderId":9007199254740993,"symbol":"PERP_BTC_USDC","type":"LIMIT","side":"BUY","status":"PARTIAL_FILLED","price":50000,"quantity":100,"seq":{seq},{fields}}}}}"#
        ),
    );
    fills(engine)
}

fn missing_quote_recovery(average: &str) {
    let (mut engine, socket) = fresh();
    assert_eq!(
        report(
            &mut engine,
            socket,
            1,
            &format!(
                r#""totalExecutedQuantity":1,"executedQuantity":1,"executedPrice":100,"tradeId":1{average}"#
            )
        ),
        [(100.0, 1.0)]
    );
    assert_eq!(
        report(
            &mut engine,
            socket,
            2,
            r#""totalExecutedQuantity":2,"averageExecutedPrice":100,"tradeId":2"#
        ),
        [(100.0, 1.0)],
        "the previous uniquely delivered whole interval already proves quote100"
    );
}

#[test]
fn missing_average_does_not_poison_next_cumulative_fill_price() {
    missing_quote_recovery("");
}

#[test]
fn zero_average_does_not_poison_next_cumulative_fill_price() {
    missing_quote_recovery(r#", "averageExecutedPrice":0"#);
}

#[test]
fn arbitrary_last_keeps_native_quote_delta_quotient() {
    let (mut engine, socket) = fresh();
    assert_eq!(
        report(
            &mut engine,
            socket,
            1,
            r#""totalExecutedQuantity":1,"averageExecutedPrice":100"#
        ),
        [(100.0, 1.0)]
    );
    assert_eq!(
        report(
            &mut engine,
            socket,
            2,
            r#""totalExecutedQuantity":2,"averageExecutedPrice":110,"executedQuantity":0.25"#
        ),
        [(480.0, 0.25)]
    );
}

#[test]
fn quiet_last_zero_observes_forward_average_without_delivery() {
    let (mut engine, socket) = fresh();
    assert!(report(&mut engine, socket, 1, r#""totalExecutedQuantity":2,"averageExecutedPrice":110,"executedQuantity":0,"executedPrice":0"#).is_empty());
    assert_eq!(
        report(
            &mut engine,
            socket,
            2,
            r#""totalExecutedQuantity":3,"averageExecutedPrice":110"#
        ),
        [(110.0, 1.0)]
    );
}

#[test]
fn equal_regressed_and_absent_cumulative_allow_last_without_replacing_quote() {
    for cumulative in [
        r#""totalExecutedQuantity":1,"#,
        r#""totalExecutedQuantity":0.5,"#,
        "",
    ] {
        let (mut engine, socket) = fresh();
        assert_eq!(
            report(
                &mut engine,
                socket,
                1,
                r#""totalExecutedQuantity":1,"averageExecutedPrice":100"#
            ),
            [(100.0, 1.0)]
        );
        assert_eq!(
            report(
                &mut engine,
                socket,
                2,
                &format!(
                    r#"{cumulative}"executedQuantity":0.25,"executedPrice":90,"averageExecutedPrice":1,"tradeId":2"#
                )
            ),
            [(90.0, 0.25)]
        );
        assert_eq!(
            report(
                &mut engine,
                socket,
                3,
                r#""totalExecutedQuantity":2,"averageExecutedPrice":100,"tradeId":3"#
            ),
            [(100.0, 1.0)]
        );
    }
}

#[test]
fn direct_zero_does_not_fall_through_to_average_or_consume_progress() {
    let (mut engine, socket) = fresh();
    assert!(report(&mut engine, socket, 1, r#""totalExecutedQuantity":1,"executedQuantity":1,"executedPrice":0,"averageExecutedPrice":100,"tradeId":1"#).is_empty());
    assert_eq!(
        report(
            &mut engine,
            socket,
            2,
            r#""totalExecutedQuantity":1,"averageExecutedPrice":100,"tradeId":1"#
        ),
        [(100.0, 1.0)]
    );
}

#[test]
fn blind_gap_does_not_extrapolate_direct_last_over_missing_history() {
    let (mut engine, socket) = fresh();
    assert_eq!(
        report(
            &mut engine,
            socket,
            1,
            r#""totalExecutedQuantity":10,"executedQuantity":1,"executedPrice":100"#
        ),
        [(100.0, 1.0)]
    );
    assert_eq!(
        report(
            &mut engine,
            socket,
            2,
            r#""totalExecutedQuantity":11,"averageExecutedPrice":100"#
        ),
        [(1100.0, 1.0)]
    );
}

#[test]
fn duplicate_higher_endpoint_observes_but_cannot_infer_undelivered_quote() {
    let (mut engine, socket) = fresh();
    assert_eq!(
        report(
            &mut engine,
            socket,
            1,
            r#""totalExecutedQuantity":1,"executedQuantity":1,"executedPrice":100,"tradeId":7"#
        ),
        [(100.0, 1.0)]
    );
    assert!(
        report(
            &mut engine,
            socket,
            2,
            r#""totalExecutedQuantity":2,"executedQuantity":1,"executedPrice":100,"tradeId":7"#
        )
        .is_empty()
    );
    assert_eq!(
        report(
            &mut engine,
            socket,
            3,
            r#""totalExecutedQuantity":3,"averageExecutedPrice":100,"tradeId":8"#
        ),
        [(300.0, 1.0)]
    );
}

#[test]
fn quiet_missing_quote_cannot_infer_undelivered_interval() {
    let (mut engine, socket) = fresh();
    assert!(
        report(
            &mut engine,
            socket,
            1,
            r#""totalExecutedQuantity":1,"executedQuantity":0,"executedPrice":100"#
        )
        .is_empty()
    );
    assert_eq!(
        report(
            &mut engine,
            socket,
            2,
            r#""totalExecutedQuantity":2,"averageExecutedPrice":100"#
        ),
        [(200.0, 1.0)]
    );
}

#[test]
fn unused_average_product_overflow_still_rejects_before_observation() {
    let (mut engine, socket) = fresh();
    assert!(report(&mut engine, socket, 1, r#""totalExecutedQuantity":2,"executedQuantity":0,"executedPrice":0,"averageExecutedPrice":1.7976931348623157e308,"tradeId":1"#).is_empty());
    assert_eq!(
        report(
            &mut engine,
            socket,
            2,
            r#""totalExecutedQuantity":2,"averageExecutedPrice":100,"tradeId":1"#
        ),
        [(100.0, 2.0)]
    );
}

#[test]
fn inferred_quote_overflow_cannot_reject_finite_direct_fills() {
    let (mut engine, socket) = fresh();
    for seq in [1, 2] {
        assert_eq!(
            report(
                &mut engine,
                socket,
                seq,
                &format!(
                    r#""totalExecutedQuantity":{seq},"executedQuantity":1,"executedPrice":1.7976931348623157e308"#
                )
            ),
            [(f64::MAX, 1.0)]
        );
    }
}

#[test]
fn native_binary_subtraction_is_not_lot_or_decimal_rounded() {
    let (mut engine, socket) = fresh();
    assert_eq!(
        report(
            &mut engine,
            socket,
            1,
            r#""totalExecutedQuantity":0.2,"averageExecutedPrice":100"#
        ),
        [(100.0, 0.2)]
    );
    let amount = 0.3 - 0.2;
    assert_eq!(
        report(
            &mut engine,
            socket,
            2,
            r#""totalExecutedQuantity":0.3,"averageExecutedPrice":100"#
        ),
        [((30.0 - 20.0) / amount, amount)]
    );
}

#[test]
fn duplicate_higher_average_observation_still_commits_before_dedup() {
    let (mut engine, socket) = fresh();
    assert_eq!(
        report(
            &mut engine,
            socket,
            1,
            r#""totalExecutedQuantity":1,"averageExecutedPrice":100,"tradeId":7"#
        ),
        [(100.0, 1.0)]
    );
    assert!(report(&mut engine, socket, 2, r#""totalExecutedQuantity":2,"executedQuantity":1,"executedPrice":100,"averageExecutedPrice":110,"tradeId":7"#).is_empty());
    assert_eq!(
        report(
            &mut engine,
            socket,
            3,
            r#""totalExecutedQuantity":3,"averageExecutedPrice":110,"tradeId":8"#
        ),
        [(110.0, 1.0)]
    );
}

#[test]
fn direct_price_keeps_forward_unused_regressing_average() {
    let (mut engine, socket) = fresh();
    assert_eq!(
        report(
            &mut engine,
            socket,
            1,
            r#""totalExecutedQuantity":1,"averageExecutedPrice":100"#
        ),
        [(100.0, 1.0)]
    );
    assert_eq!(
        report(
            &mut engine,
            socket,
            2,
            r#""totalExecutedQuantity":2,"averageExecutedPrice":1,"executedPrice":90"#
        ),
        [(90.0, 1.0)]
    );
    assert_eq!(
        report(
            &mut engine,
            socket,
            3,
            r#""totalExecutedQuantity":3,"averageExecutedPrice":100"#
        ),
        [(298.0, 1.0)]
    );
}

#[test]
fn exact_binary_growth_does_not_use_epsilon_to_infer_quote() {
    let (mut engine, socket) = fresh();
    assert_eq!(
        report(
            &mut engine,
            socket,
            1,
            r#""totalExecutedQuantity":0.2,"averageExecutedPrice":100"#
        ),
        [(100.0, 0.2)]
    );
    assert_eq!(
        report(
            &mut engine,
            socket,
            2,
            r#""totalExecutedQuantity":0.3,"executedQuantity":0.1,"executedPrice":100"#
        ),
        [(100.0, 0.1)]
    );
    let amount = 0.4 - 0.3;
    assert_eq!(
        report(
            &mut engine,
            socket,
            3,
            r#""totalExecutedQuantity":0.4,"averageExecutedPrice":100"#
        ),
        [(40.0 / amount, amount)]
    );
}

#[test]
fn equal_endpoint_average_cannot_repair_an_unobserved_quote() {
    let (mut engine, socket) = fresh();
    assert_eq!(
        report(
            &mut engine,
            socket,
            1,
            r#""totalExecutedQuantity":10,"executedQuantity":1,"executedPrice":100"#
        ),
        [(100.0, 1.0)]
    );
    assert!(
        report(
            &mut engine,
            socket,
            2,
            r#""totalExecutedQuantity":10,"averageExecutedPrice":100,"executedQuantity":0"#
        )
        .is_empty()
    );
    assert_eq!(
        report(
            &mut engine,
            socket,
            3,
            r#""totalExecutedQuantity":11,"averageExecutedPrice":100"#
        ),
        [(1100.0, 1.0)]
    );
}

#[test]
fn valid_direct_price_does_not_validate_an_unused_overflowing_quotient() {
    let (mut engine, socket) = fresh();
    assert_eq!(
        report(
            &mut engine,
            socket,
            1,
            r#""totalExecutedQuantity":1,"averageExecutedPrice":1.7976931348623157e308"#
        ),
        [(f64::MAX, 1.0)]
    );
    assert_eq!(
        report(
            &mut engine,
            socket,
            2,
            r#""totalExecutedQuantity":2,"averageExecutedPrice":1,"executedQuantity":1e-308,"executedPrice":100"#
        ),
        [(100.0, 1e-308)]
    );
}

#[test]
fn rest_filled_observation_does_not_seed_native_execution_totals() {
    let order = ORDER.replace(
        "\"total_executed_quantity\":0",
        "\"total_executed_quantity\":0.75",
    );
    let (mut engine, socket) = ready_with(&order, "[]", POSITIONS);
    actions(&mut engine);
    engine.take_frame(1000.0);
    assert_eq!(engine.store().orders()[0].filled, 0.75);
    assert_eq!(
        report(
            &mut engine,
            socket,
            1,
            r#""totalExecutedQuantity":1,"averageExecutedPrice":100"#
        ),
        [(100.0, 1.0)]
    );
}

#[test]
fn native_trade_and_match_namespaces_and_late_delivery_are_retained() {
    let (mut engine, socket) = fresh();
    for (seq, fields) in [
        (
            10,
            r#""tradeId":7,"match_id":8,"totalExecutedQuantity":1,"averageExecutedPrice":100"#,
        ),
        (
            9,
            r#""tradeId":8,"match_id":8,"totalExecutedQuantity":2,"executedQuantity":1,"executedPrice":100,"averageExecutedPrice":100"#,
        ),
        (
            8,
            r#""match_id":8,"totalExecutedQuantity":3,"executedQuantity":1,"executedPrice":100,"averageExecutedPrice":100"#,
        ),
    ] {
        assert_eq!(report(&mut engine, socket, seq, fields), [(100.0, 1.0)]);
    }
    assert_eq!(
        engine.store().orders()[0].filled,
        1.0,
        "display fence is separate from unique Fill delivery"
    );
    assert!(report(&mut engine, socket, 7, r#""match_id":8,"totalExecutedQuantity":4,"executedQuantity":1,"executedPrice":100,"averageExecutedPrice":100"#).is_empty());
    assert!(report(&mut engine, socket, 6, r#""tradeId":7,"match_id":9,"totalExecutedQuantity":5,"executedQuantity":1,"executedPrice":100,"averageExecutedPrice":100"#).is_empty());
    assert_eq!(
        report(
            &mut engine,
            socket,
            5,
            r#""totalExecutedQuantity":6,"averageExecutedPrice":100"#
        ),
        [(100.0, 1.0)]
    );
    assert!(
        report(
            &mut engine,
            socket,
            4,
            r#""totalExecutedQuantity":6,"executedQuantity":1,"executedPrice":100"#
        )
        .is_empty()
    );
}

#[test]
fn root_product_overflow_rolls_back_first_child_quote_and_identity() {
    let (mut engine, socket) = fresh();
    let before = engine.store().orders().to_vec();
    let root = |average: &str| {
        format!(
            r#"{{"topic":"algoexecutionreportv2","ts":4000,"data":{{"algoOrderId":100,"rootAlgoOrderId":100,"symbol":"PERP_BTC_USDC","algoType":"POSITIONAL_TP_SL","algoStatus":"NEW","childOrders":[{{"algoOrderId":101,"rootAlgoOrderId":100,"algoType":"STOP_LOSS","side":"SELL","algoStatus":"PARTIAL_FILLED","triggerPrice":49000,"quantity":3,"totalExecutedQuantity":1,"executedQuantity":1,"executedPrice":100,"tradeId":7}},{{"algoOrderId":102,"rootAlgoOrderId":100,"algoType":"TAKE_PROFIT","side":"SELL","algoStatus":"PARTIAL_FILLED","triggerPrice":55000,"quantity":3,"totalExecutedQuantity":2,"executedQuantity":1,"executedPrice":100,"averageExecutedPrice":{average},"tradeId":8}}]}}}}"#
        )
    };
    ws(&mut engine, socket, &root("1.7976931348623157e308"));
    assert!(fills(&mut engine).is_empty());
    assert_eq!(engine.store().orders(), before.as_slice());
    ws(&mut engine, socket, &root("100"));
    assert_eq!(
        fills(&mut engine),
        [(100.0, 1.0), (100.0, 1.0)],
        "invalid root must not consume first child's Fill identity"
    );
    ws(
        &mut engine,
        socket,
        r#"{"topic":"algoexecutionreportv2","ts":5000,"data":{"algoOrderId":101,"rootAlgoOrderId":100,"symbol":"PERP_BTC_USDC","algoType":"STOP_LOSS","side":"SELL","algoStatus":"PARTIAL_FILLED","triggerPrice":49000,"quantity":3,"totalExecutedQuantity":2,"averageExecutedPrice":100,"tradeId":9}}"#,
    );
    assert_eq!(fills(&mut engine), [(100.0, 1.0)]);
}

#[test]
fn native_terminal_fifo_eviction_still_removes_cumulative_quote() {
    let (mut engine, socket) = fresh();
    for id in 1..=4097 {
        ws(
            &mut engine,
            socket,
            &format!(
                r#"{{"topic":"executionreport","ts":3000,"data":{{"orderId":{id},"symbol":"PERP_BTC_USDC","type":"LIMIT","side":"BUY","status":"FILLED","price":100,"quantity":1,"totalExecutedQuantity":1,"averageExecutedPrice":100,"tradeId":1}}}}"#
            ),
        );
        assert_eq!(fills(&mut engine), [(100.0, 1.0)]);
        actions(&mut engine);
    }
    ws(
        &mut engine,
        socket,
        r#"{"topic":"executionreport","ts":4000,"data":{"orderId":1,"symbol":"PERP_BTC_USDC","type":"LIMIT","side":"BUY","status":"FILLED","price":100,"quantity":2,"totalExecutedQuantity":2,"averageExecutedPrice":100,"tradeId":2}}"#,
    );
    assert_eq!(
        fills(&mut engine),
        [(100.0, 2.0)],
        "evicted ID cannot regain its old quote/cumulative anchor"
    );
}
