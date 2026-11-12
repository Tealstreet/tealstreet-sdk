use super::*;
const ENGINE_UNKNOWN_TEXT: &str = "The outcome is unknown. Check Open Orders";

fn writes(actions: &[Action]) -> Vec<HttpRequest> {
    actions
        .iter()
        .filter_map(|action| match action {
            Action::Http(request) if request.method == Method::Post => Some(request.clone()),
            _ => None,
        })
        .collect()
}

fn ack(e: &mut Engine<BingxSpot>, actions: &[Action], id: &str) -> (Value, Vec<Action>) {
    let requests = writes(actions);
    assert_eq!(requests.len(), 1);
    let request = &requests[0];
    assert!(request.url.ends_with("/spot/v1/trade/order"));
    let body: Value = serde_json::from_str(request.body.as_deref().unwrap()).unwrap();
    reply(
        e,
        request,
        200,
        &json!({"code":0,"data":{
            "orderId":id,"symbol":"BTC-USDT","side":body["side"],
            "type":body["type"],"status":"NEW"
        }})
        .to_string(),
    );
    (body, collect(e))
}

fn failure(actions: &[Action]) -> &safe_cex_engine::VenueError {
    let errors = actions
        .iter()
        .filter_map(|action| match action {
            Action::Notice(Notice::CommandFailed { command: 70, error }) => Some(error),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(errors.len(), 1, "one command completion");
    errors[0]
}

fn unknown_outcome_and_journal(actions: &[Action], accepted: &[&str]) {
    // The terminal's classification and native IDs are the engine journal's
    // public projection. They are separate from the diagnostic below.
    let error = failure(actions);
    assert_eq!(error.outcome, Some(Outcome::Unknown));
    assert!(
        error.message.contains(ENGINE_UNKNOWN_TEXT),
        "{}",
        error.message
    );
    assert_eq!(
        error
            .partial
            .as_ref()
            .map(|p| p.order_ids.iter().map(String::as_str).collect::<Vec<_>>())
            .unwrap_or_default(),
        accepted,
        "journal retains only proven native IDs"
    );
}

fn unknown_event(actions: &[Action]) -> &safe_cex_engine::LogEvent {
    let events = actions
        .iter()
        .filter_map(|action| match action {
            Action::Notice(Notice::Event(event)) if event.code == "order.outcome_unknown" => {
                Some(event)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        events.len(),
        1,
        "one independent Unknown diagnostic; observed codes: {:?}",
        actions
            .iter()
            .filter_map(|a| match a {
                Action::Notice(Notice::Event(e)) => Some(e.code),
                _ => None,
            })
            .collect::<Vec<_>>()
    );
    let event = events[0];
    assert_eq!(event.level, safe_cex_engine::Level::Error);
    assert_eq!(event.message, ENGINE_UNKNOWN_TEXT);
    event
}

fn unknown_telemetry(actions: &[Action], operation: &str) {
    assert_eq!(
        unknown_event(actions).fields,
        vec![
            ("command", 70u32.into()),
            ("outcome", "unknown".into()),
            ("operation", operation.into()),
        ]
    );
}

fn engine_unknown_telemetry(
    actions: &[Action],
    request: safe_cex_engine::RequestId,
    status: Option<u16>,
) {
    let mut fields = vec![
        ("command", 70u32.into()),
        ("outcome", "unknown".into()),
        ("request", request.0.to_string().into()),
    ];
    if let Some(status) = status {
        fields.push(("status", u32::from(status).into()));
    }
    assert_eq!(unknown_event(actions).fields, fields);
}

fn late_unknown_ack_is_inert(e: &mut Engine<BingxSpot>, request: &HttpRequest) {
    reply(
        e,
        request,
        200,
        r#"{"code":0,"data":{"orderId":"late","symbol":"BTC-USDT","side":"BUY","type":"LIMIT","status":"NEW"}}"#,
    );
    e.handle(
        NOW,
        Event::HttpFailed {
            request: request.id,
            error: "late duplicate failure",
        },
    );
    let late = collect(e);
    assert!(writes(&late).is_empty());
    assert!(!late.iter().any(|a| matches!(
        a,
        Action::Notice(
            Notice::CommandDone { command: 70, .. } | Notice::CommandFailed { command: 70, .. }
        )
    )));
    assert!(!late.iter().any(|a| matches!(a,
        Action::Notice(Notice::Event(event)) if event.code == "order.outcome_unknown"
    )));
}

fn partial_row(row: Value, quantity: f64, price: Option<f64>) {
    let (mut e, _) = if row["type"] == "market" && row["side"] == "sell" {
        seed_with_fixtures(true, MARKETS, r#"{"code":0,"data":[]}"#)
    } else {
        seed(true)
    };
    let actions = submit_batch(
        &mut e,
        vec![
            batch_order(0.002, 41000., "prefix"),
            row,
            batch_order(0.003, 41000., "suffix"),
        ],
    );
    let (_, actions) = ack(&mut e, &actions, "1001");
    let middle = writes(&actions);
    assert_eq!(middle.len(), 1, "TS sends this numeric payload");
    let body: Value = serde_json::from_str(middle[0].body.as_deref().unwrap()).unwrap();
    assert_eq!(body["quantity"], quantity);
    let sum: f64 = middle
        .iter()
        .map(|request| {
            let body: Value = serde_json::from_str(request.body.as_deref().unwrap()).unwrap();
            body["quantity"].as_f64().unwrap()
        })
        .sum();
    assert_eq!(sum, quantity);
    assert_eq!(body.get("price").and_then(Value::as_f64), price);
    reply(
        &mut e,
        &middle[0],
        200,
        r#"{"code":100440,"msg":"fixture refusal"}"#,
    );
    let actions = collect(&mut e);
    let (body, actions) = ack(&mut e, &actions, "1003");
    assert_eq!(body["quantity"], 0.003);
    assert!(writes(&actions).is_empty());
    let error = failure(&actions);
    assert_eq!(error.venue_code.as_deref(), Some("100440"));
    assert_eq!(error.outcome, None);
    assert_eq!(error.partial.as_ref().unwrap().order_ids, ["1001", "1003"]);
    assert!(!actions.iter().any(|a| matches!(a,
        Action::Notice(Notice::Event(event)) if event.code == "order.outcome_unknown"
    )));
}

// TS: packages/safe-cex/src/__tests__/reference/bingx/bingx.exchange.ts:1485-1542,1854-1889.
// KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: bingx-spot | quantity-minimum-and-notional
#[test]
fn below_minimum_and_notional_rows_follow_the_prefix_and_refusal_continues() {
    partial_row(batch_order(0.00013, 41000., "small"), 0.00013, Some(41000.));
    partial_row(batch_order(0.002, 2000., "cheap"), 0.002, Some(2000.));
    partial_row(
        batch_order(0.002, 1_000_000_000., "expensive"),
        0.002,
        Some(1_000_000_000.),
    );
}

// TS: packages/safe-cex/src/__tests__/reference/bingx/bingx.exchange.ts:1485-1487,1854-1889.
// KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: bingx-spot | quantity-normalization
#[test]
fn rounded_quantity_is_sent_without_a_conservation_gate() {
    // KNOWN-ISSUE: legacy TS under-allocates (sum 0.002 of requested 0.002004);
    // kept for exact TS parity per Sam's decision.
    partial_row(
        batch_order(0.002004, 41000., "rounded"),
        0.002,
        Some(41000.),
    );
    partial_row(batch_order(0.000006, 41000., "tiny"), 0.00001, Some(41000.));
}

// TS: packages/safe-cex/src/__tests__/reference/bingx/bingx.exchange.ts:1491-1492,1854-1889.
// KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: bingx-spot | limit-price-normalization
#[test]
fn rounded_and_negative_limit_prices_are_sent() {
    partial_row(batch_order(0.002, 41000.005, "tick"), 0.002, Some(41000.));
    partial_row(batch_order(0.002, -1., "negative"), 0.002, Some(-1.));
}

// TS: packages/safe-cex/src/__tests__/reference/bingx/bingx.exchange.ts:1489-1492,1854-1889.
// KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: bingx-spot | market-sell-without-notional-reference
#[test]
fn market_sell_does_not_need_a_notional_reference() {
    partial_row(
        json!({"symbol":"BTCUSDT","type":"market","side":"sell",
        "amount":0.002,"clientOrderId":"unpriced_sell"}),
        0.002,
        None,
    );
}

// TS: packages/safe-cex/src/__tests__/reference/bingx/bingx.exchange.ts:1544-1564,1854-1889.
// KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: bingx-spot | max-quantity-lots-and-tail
#[test]
fn lot_tail_below_native_minimum_is_sent_and_the_next_input_follows_its_refusal() {
    let (mut e, _) = seed(true);
    let split = json!({"symbol":"BTCUSDT","type":"limit","side":"buy",
        "amount":18.26639,"price":1});
    let mut actions = submit_batch(&mut e, vec![split, batch_order(0.002, 41000., "later")]);
    let mut clients = std::collections::HashSet::new();
    for (index, quantity) in [9.13319, 9.13319].into_iter().enumerate() {
        let (body, next) = ack(&mut e, &actions, &format!("100{index}"));
        assert_eq!(body["quantity"], quantity);
        assert!(clients.insert(body["newClientOrderId"].as_str().unwrap().to_owned()));
        actions = next;
    }
    let tail = writes(&actions);
    assert_eq!(tail.len(), 1);
    let body: Value = serde_json::from_str(tail[0].body.as_deref().unwrap()).unwrap();
    assert_eq!(body["quantity"], 0.00001);
    assert!(clients.insert(body["newClientOrderId"].as_str().unwrap().to_owned()));
    reply(
        &mut e,
        &tail[0],
        200,
        r#"{"code":100440,"msg":"tail too small"}"#,
    );
    let actions = collect(&mut e);
    let (_, actions) = ack(&mut e, &actions, "1003");
    assert_eq!(
        failure(&actions).partial.as_ref().unwrap().order_ids,
        ["1000", "1001", "1003"]
    );
}

// TS: packages/safe-cex/src/__tests__/reference/bingx/bingx.exchange.ts:1447-1452,1461-1464.
#[test]
fn missing_market_is_a_formatter_failure_before_any_write() {
    let (mut e, _) = seed(true);
    let mut missing = batch_order(0.002, 41000., "missing");
    missing["symbol"] = json!("MISSINGUSDT");
    let actions = submit_batch(&mut e, vec![batch_order(0.002, 41000., "prefix"), missing]);
    assert!(
        actions
            .iter()
            .all(|action| !matches!(action, Action::Http(_)))
    );
    assert_eq!(failure(&actions).outcome, Some(Outcome::NotSent));
}

// PARITY-EXCEPTION: stop-after-UNKNOWN (Sean/daxx, 2026-11-08); TS continues at
// packages/safe-cex/src/__tests__/reference/bingx/bingx.exchange.ts:1871-1889.
#[test]
fn unknown_middle_payload_halts_with_prefix_ids_and_a_terminal_event() {
    unknown_middle_payload(&[0, 1]);
}

// PARITY-EXCEPTION: stop-after-UNKNOWN (Sean/daxx, 2026-11-08); TS continues at
// packages/safe-cex/src/__tests__/reference/bingx/bingx.exchange.ts:1871-1889.
#[test]
fn venue_decoded_unknown_middle_emits_a_diagnostic_and_journal() {
    unknown_middle_payload(&[2, 3]);
}

fn unknown_middle_payload(modes: &[u8]) {
    let paths = [
        (
            "quantity-minimum",
            batch_order(0.00013, 41000., "small"),
            false,
        ),
        ("notional", batch_order(0.002, 2000., "cheap"), false),
        (
            "quantity-normalization",
            batch_order(0.002004, 41000., "rounded"),
            false,
        ),
        (
            "price-normalization",
            batch_order(0.002, 41000.005, "tick"),
            false,
        ),
        (
            "market-sell-reference",
            json!({"symbol":"BTCUSDT","type":"market","side":"sell","amount":0.002,"clientOrderId":"market"}),
            false,
        ),
        ("serial-batch", batch_order(0.002, 41000., "middle"), false),
        (
            "max-quantity-lots",
            json!({"symbol":"BTCUSDT","type":"limit","side":"buy","amount":18.26639,"price":1}),
            true,
        ),
    ];
    for (path, row, split) in paths {
        for &mode in modes {
            let (mut e, _) = if path == "market-sell-reference" {
                seed_with_fixtures(true, MARKETS, r#"{"code":0,"data":[]}"#)
            } else {
                seed(true)
            };
            let actions = submit_batch(
                &mut e,
                vec![
                    batch_order(0.002, 41000., "prefix"),
                    row.clone(),
                    batch_order(0.003, 41000., "suffix"),
                ],
            );
            let (_, mut actions) = ack(&mut e, &actions, "1001");
            let mut accepted = vec!["1001"];
            if split {
                (_, actions) = ack(&mut e, &actions, "1002");
                accepted.push("1002");
            }
            let requests = writes(&actions);
            assert_eq!(requests.len(), 1, "{path}");
            if mode == 0 {
                e.handle(
                    NOW,
                    Event::HttpFailed {
                        request: requests[0].id,
                        error: "fixture connection lost",
                    },
                );
            } else {
                reply(
                    &mut e,
                    &requests[0],
                    if mode == 1 { 503 } else { 200 },
                    if mode == 1 {
                        "upstream unavailable"
                    } else if mode == 2 {
                        "malformed"
                    } else {
                        r#"{"code":100500,"msg":"unlisted native refusal"}"#
                    },
                );
            }
            let actions = collect(&mut e);
            assert!(
                writes(&actions).is_empty(),
                "{path}: Unknown cannot release the suffix"
            );
            unknown_outcome_and_journal(&actions, &accepted);
            if mode >= 2 {
                unknown_telemetry(&actions, "place");
            } else {
                engine_unknown_telemetry(&actions, requests[0].id, (mode == 1).then_some(503));
            }
            late_unknown_ack_is_inert(&mut e, &requests[0]);
        }
    }
}

fn cancels(e: &mut Engine<BingxSpot>) -> Vec<Action> {
    e.handle(
        NOW,
        Event::Command(Command::CancelOrdersWithSymbols {
            command: 70,
            orders: serde_json::from_value(json!([
                {"id":"1001","symbol":"BTCUSDT"},
                {"id":"1002","symbol":"BTCUSDT"}
            ]))
            .unwrap(),
        }),
    );
    collect(e)
}

// TS: packages/safe-cex/src/__tests__/reference/bingx/bingx.exchange.ts:940-962.
// KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: bingx-spot | independent-cancellation-continuation
#[test]
fn cancellation_continues_after_a_definite_refusal() {
    let (mut e, _) = seed(true);
    let first = writes(&cancels(&mut e));
    assert_eq!(first.len(), 1);
    reply(
        &mut e,
        &first[0],
        200,
        r#"{"code":100421,"msg":"definite refusal"}"#,
    );
    let second = writes(&collect(&mut e));
    assert_eq!(second.len(), 1);
    let body: Value = serde_json::from_str(second[0].body.as_deref().unwrap()).unwrap();
    assert_eq!(body["orderId"], "1002");
    reply(
        &mut e,
        &second[0],
        200,
        r#"{"code":0,"data":{"orderId":"1002","symbol":"BTC-USDT","status":"CANCELED"}}"#,
    );
    let actions = collect(&mut e);
    assert!(writes(&actions).is_empty());
    assert_eq!(failure(&actions).outcome, None);
    assert!(!actions.iter().any(|a| matches!(a,
        Action::Notice(Notice::Event(event)) if event.code == "order.outcome_unknown"
    )));
}

#[test]
fn spot_followup_empty_refusal_data_preserves_classification_and_continuation() {
    // TS: reference/bingx/bingx.exchange.ts:1854-1889 continues after native refusal.
    for data in [
        None,
        Some(json!(null)),
        Some(json!({})),
        Some(json!([])),
        Some(json!({"orders":[]})),
    ] {
        let (mut e, _) = seed(true);
        let actions = submit_batch(
            &mut e,
            [0.002, 0.003, 0.004]
                .into_iter()
                .enumerate()
                .map(|(i, amount)| batch_order(amount, 41000., &format!("row_{i}")))
                .collect(),
        );
        let (_, actions) = ack(&mut e, &actions, "1001");
        let middle = writes(&actions);
        assert_eq!(middle.len(), 1);
        let mut refusal = json!({"code":100440,"msg":"fixture known refusal"});
        if let Some(data) = data {
            refusal["data"] = data;
        }
        reply(&mut e, &middle[0], 200, &refusal.to_string());
        let actions = collect(&mut e);
        assert_eq!(
            writes(&actions).len(),
            1,
            "definite refusal continues: {refusal}"
        );
        assert!(!actions.iter().any(|a| matches!(a,
            Action::Notice(Notice::Event(event)) if event.code == "order.outcome_unknown"
        )));
        let (body, actions) = ack(&mut e, &actions, "1003");
        assert_eq!(body["quantity"], 0.004);
        let error = failure(&actions);
        assert_eq!(error.kind, Some(safe_cex_engine::ErrorKind::InvalidInput));
        assert_eq!(error.venue_code.as_deref(), Some("100440"));
        assert_eq!(
            error.venue_message.as_deref(),
            Some("fixture known refusal")
        );
        assert_eq!(error.outcome, None);
        assert_eq!(error.partial.as_ref().unwrap().order_ids, ["1001", "1003"]);
        assert!(writes(&actions).is_empty());
        let actions = submit_batch(&mut e, vec![batch_order(0.002, 41000., "fresh")]);
        let (_, actions) = ack(&mut e, &actions, "1004");
        assert_eq!(done(&actions, 70), json!(["1004"]));
    }
}

#[test]
fn spot_followup_cancel_empty_refusal_data_continues() {
    for data in [json!({}), json!([]), json!({"orders": []})] {
        let (mut e, _) = seed(true);
        let first = writes(&cancels(&mut e));
        reply(
            &mut e,
            &first[0],
            200,
            &json!({
                "code":100421,"msg":"definite refusal","data":data
            })
            .to_string(),
        );
        let second = writes(&collect(&mut e));
        assert_eq!(second.len(), 1);
        let body: Value = serde_json::from_str(second[0].body.as_deref().unwrap()).unwrap();
        assert_eq!(body["orderId"], "1002");
        reply(
            &mut e,
            &second[0],
            200,
            r#"{"code":0,"data":{"orderId":"1002","symbol":"BTC-USDT","status":"CANCELED"}}"#,
        );
        let actions = collect(&mut e);
        assert_eq!(failure(&actions).venue_code.as_deref(), Some("100421"));
        assert_eq!(failure(&actions).outcome, None);
        assert!(!actions.iter().any(|a| matches!(a,
            Action::Notice(Notice::Event(event)) if event.code == "order.outcome_unknown"
        )));
    }
}

#[test]
fn spot_followup_effect_claims_and_empty_success_stay_unknown() {
    for body in [
        json!({"code":100440,"data":{"orderId":"1002","symbol":"BTC-USDT","status":"NEW","side":"BUY","type":"LIMIT"}}),
        json!({"code":100440,"data":{"orderId":"1002"}}),
        json!({"code":100440,"data":{"order":{"orderId":"1002"}}}),
        json!({"code":100440,"data":{"orders":[{"orderId":"1002"}]}}),
        json!({"code":0,"data":{}}),
        json!({"code":0,"data":{"orders":[]}}),
    ] {
        assert_indexed_spot_unknown(body);
    }
}

#[test]
fn spot_followup_incomplete_execution_evidence_stays_unknown() {
    for data in [
        json!({"symbol":"BTC-USDT","side":"BUY","type":"LIMIT",
            "status":"FILLED","executedQty":"0.003","origQty":"0.003"}),
        json!({"executedQty":"0.003","cummulativeQuoteQty":"123"}),
        json!({"status":"FILLED"}),
        json!({"status":"PARTIALLY_FILLED","executedQty":0.001}),
        json!({"executedQty":0.003}),
        json!({"cummulativeQuoteQty":123}),
        json!({"orders":[],"executedQty":"0.003"}),
    ] {
        for null_id in [false, true] {
            let mut data = data.clone();
            if null_id {
                data["orderId"] = Value::Null;
            }
            assert_indexed_spot_unknown(json!({"code":100440,"data":data}));
        }
    }
}

#[test]
fn spot_followup_nonempty_unrecognized_data_stays_unknown() {
    for data in [
        json!({"unrecognizedExecutionField":"execution evidence"}),
        json!([null, null, []]),
        json!({"orderId":null,"orders":[],"fills":[{"quantity":"0.003"}]}),
    ] {
        assert_indexed_spot_unknown(json!({"code":100440,"data":data}));
    }
}

fn assert_indexed_spot_unknown(body: Value) {
    let (mut e, _) = seed(true);
    let actions = submit_batch(
        &mut e,
        vec![
            batch_order(0.002, 41000., "prefix"),
            batch_order(0.003, 41000., "middle"),
            batch_order(0.004, 41000., "suffix"),
        ],
    );
    let (_, actions) = ack(&mut e, &actions, "1001");
    let middle = writes(&actions);
    reply(&mut e, &middle[0], 200, &body.to_string());
    let actions = collect(&mut e);
    assert!(
        writes(&actions).is_empty(),
        "Unknown must halt before suffix: {body}"
    );
    unknown_outcome_and_journal(&actions, &["1001"]);
    let error = failure(&actions);
    let elements = error.elements.as_ref().expect("indexed Spot outcomes");
    assert_eq!(elements.len(), 3);
    for (index, client) in ["prefix", "middle", "suffix"].iter().enumerate() {
        assert_eq!(elements[index].element.index, index);
        assert_eq!(
            elements[index].element.client_order_id.as_deref(),
            Some(*client)
        );
    }
    assert!(matches!(&elements[0].result,
            safe_cex_engine::write_plan::ElementResult::Placed { order_ids }
                if order_ids == &["1001"]));
    assert!(matches!(&elements[1].result,
            safe_cex_engine::write_plan::ElementResult::Unknown { error }
                if error.outcome == Some(Outcome::Unknown)
                    && error.message == ENGINE_UNKNOWN_TEXT));
    assert!(matches!(
        &elements[2].result,
        safe_cex_engine::write_plan::ElementResult::NotSent { error: None }
    ));
    let partial = error.partial.as_ref().unwrap();
    assert_eq!(partial.failed_index, Some(1));
    assert_eq!(partial.total, Some(3));
    unknown_telemetry(&actions, "place");
    late_unknown_ack_is_inert(&mut e, &middle[0]);
}

#[test]
fn spot_unknown_journal_keeps_an_earlier_refusal_and_later_failed_index() {
    let (mut e, _) = seed(true);
    let actions = submit_batch(
        &mut e,
        vec![
            batch_order(0.002, 41000., "prefix"),
            batch_order(0.003, 41000., "refused"),
            batch_order(0.004, 41000., "unknown"),
            batch_order(0.005, 41000., "suffix"),
        ],
    );
    let (_, actions) = ack(&mut e, &actions, "1001");
    reply(
        &mut e,
        &writes(&actions)[0],
        200,
        r#"{"code":100440,"data":{}}"#,
    );
    let actions = collect(&mut e);
    reply(&mut e, &writes(&actions)[0], 200, r#"{"code":0,"data":{}}"#);
    let actions = collect(&mut e);
    assert!(writes(&actions).is_empty());
    unknown_outcome_and_journal(&actions, &["1001"]);
    unknown_telemetry(&actions, "place");
    let error = failure(&actions);
    assert_eq!(error.venue_code.as_deref(), Some("100440"));
    let partial = error.partial.as_ref().unwrap();
    assert_eq!(partial.total, Some(4));
    assert_eq!(partial.failed_index, Some(2));
    let elements = error.elements.as_ref().unwrap();
    assert_eq!(elements.len(), 4);
    assert!(matches!(&elements[1].result,
        safe_cex_engine::write_plan::ElementResult::NotSent { error: Some(error) }
            if error.outcome == Some(Outcome::Rejected)));
    assert!(matches!(
        &elements[2].result,
        safe_cex_engine::write_plan::ElementResult::Unknown { .. }
    ));
    assert!(matches!(
        &elements[3].result,
        safe_cex_engine::write_plan::ElementResult::NotSent { error: None }
    ));
}

// PARITY-EXCEPTION: stop-after-UNKNOWN (Sean/daxx, 2026-11-08); TS continues at
// packages/safe-cex/src/__tests__/reference/bingx/bingx.exchange.ts:940-962.
#[test]
fn cancellation_unknown_halts_the_action() {
    cancellation_unknown(&[0, 1]);
}

#[test]
fn engine_sealed_cancel_all_reports_only_bounded_facts_once() {
    for status in [None, Some(408), Some(429), Some(500), Some(503)] {
        let (mut e, _) = seed(true);
        e.handle(
            NOW,
            Event::Command(Command::CancelSymbolOrders {
                command: 70,
                symbol: "BTCUSDT".into(),
            }),
        );
        let r = request(&collect(&mut e), "/trade/cancelOpenOrders");
        if let Some(status) = status {
            reply(
                &mut e,
                &r,
                status,
                "fixture-private-response-not-for-telemetry",
            );
        } else {
            e.handle(
                NOW,
                Event::HttpFailed {
                    request: r.id,
                    error: "fixture-private-error-not-for-telemetry",
                },
            );
        }
        let actions = collect(&mut e);
        assert!(writes(&actions).is_empty());
        unknown_outcome_and_journal(&actions, &[]);
        engine_unknown_telemetry(&actions, r.id, status);
        late_unknown_ack_is_inert(&mut e, &r);
    }
}

#[test]
fn read_failures_do_not_emit_order_unknown_diagnostics() {
    for status in [None, Some(503)] {
        let (mut e, _) = seed(true);
        e.handle(
            NOW,
            Event::Command(Command::Venue {
                command: 70,
                action: safe_cex_engine::VenueAction::RefreshOrders { symbols: None },
            }),
        );
        let r = request(&collect(&mut e), "/trade/openOrders");
        if let Some(status) = status {
            reply(&mut e, &r, status, "fixture-read-failed");
        } else {
            e.handle(
                NOW,
                Event::HttpFailed {
                    request: r.id,
                    error: "fixture-read-failed",
                },
            );
        }
        let actions = collect(&mut e);
        assert!(writes(&actions).is_empty());
        assert!(!actions.iter().any(|action| matches!(action,
            Action::Notice(Notice::Event(event)) if event.code == "order.outcome_unknown"
        )));
    }
}

// PARITY-EXCEPTION: stop-after-UNKNOWN (Sean/daxx, 2026-11-08); TS continues at
// packages/safe-cex/src/__tests__/reference/bingx/bingx.exchange.ts:940-962.
#[test]
fn venue_decoded_unknown_cancel_emits_a_diagnostic_and_journal() {
    cancellation_unknown(&[2, 3]);
}

fn cancellation_unknown(modes: &[u8]) {
    for &mode in modes {
        let (mut e, _) = seed(true);
        let first = writes(&cancels(&mut e));
        assert_eq!(first.len(), 1);
        if mode == 0 {
            e.handle(
                NOW,
                Event::HttpFailed {
                    request: first[0].id,
                    error: "lost cancel",
                },
            );
        } else {
            reply(
                &mut e,
                &first[0],
                if mode == 1 { 503 } else { 200 },
                match mode {
                    1 => "upstream unavailable",
                    2 => "malformed",
                    _ => r#"{"code":100500,"msg":"unlisted native refusal"}"#,
                },
            );
        }
        let actions = collect(&mut e);
        assert!(writes(&actions).is_empty());
        unknown_outcome_and_journal(&actions, &[]);
        if mode >= 2 {
            unknown_telemetry(&actions, "cancel");
        } else {
            engine_unknown_telemetry(&actions, first[0].id, (mode == 1).then_some(503));
        }
        late_unknown_ack_is_inert(&mut e, &first[0]);
    }
}

// TS: packages/safe-cex/src/__tests__/reference/bingx/bingx.exchange.ts:1544-1564,1854-1868.
// KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: bingx-spot | max-quantity-lots-and-tail
#[test]
fn all_accepted_split_control_returns_every_native_id_once() {
    let (mut e, _) = seed(true);
    let mut actions = submit_batch(
        &mut e,
        vec![json!({"symbol":"BTCUSDT","type":"limit",
        "side":"buy","amount":20.,"price":41000})],
    );
    for (index, quantity) in [9.13319, 9.13319, 1.73362].into_iter().enumerate() {
        let (body, next) = ack(&mut e, &actions, &format!("200{index}"));
        assert_eq!(body["quantity"], quantity);
        assert_eq!(body["price"], 41000.);
        assert!(body.get("positionSide").is_none());
        actions = next;
    }
    assert!(writes(&actions).is_empty());
    assert_eq!(done(&actions, 70), json!(["2000", "2001", "2002"]));
}

// TS: packages/safe-cex/src/__tests__/reference/bingx/bingx.exchange.ts:1565,assertSingleNativeClientOrderId.
#[test]
fn explicit_client_id_cannot_be_reused_across_native_lots() {
    let (mut e, _) = seed(true);
    let actions = submit_batch(&mut e, vec![batch_order(20., 41000., "caller")]);
    assert!(
        actions
            .iter()
            .all(|action| !matches!(action, Action::Http(_)))
    );
    assert_eq!(failure(&actions).outcome, Some(Outcome::NotSent));
}

// PARITY-EXCEPTION: stop-after-UNKNOWN (Sean/daxx, 2026-11-08); TS continues at
// packages/safe-cex/src/__tests__/reference/bingx/bingx.exchange.ts:1871-1889.
#[test]
fn gateway_after_refusal_preserves_native_prefix_and_unknown_text() {
    unknown_after_refusal(503, "upstream unavailable");
}

// PARITY-EXCEPTION: stop-after-UNKNOWN (Sean/daxx, 2026-11-08); TS continues at
// packages/safe-cex/src/__tests__/reference/bingx/bingx.exchange.ts:1871-1889.
#[test]
fn venue_decoded_unknown_after_refusal_emits_a_diagnostic_and_journal() {
    unknown_after_refusal(200, r#"{"code":100500,"msg":"unlisted native refusal"}"#);
}

fn unknown_after_refusal(status: u16, body: &str) {
    let (mut e, _) = seed(true);
    let rows = (0..4)
        .map(|i| batch_order(0.002, 41000., &format!("row_{i}")))
        .collect();
    let actions = submit_batch(&mut e, rows);
    let (_, actions) = ack(&mut e, &actions, "1001");
    let second = writes(&actions);
    reply(
        &mut e,
        &second[0],
        200,
        r#"{"code":100440,"msg":"definite refusal"}"#,
    );
    let third = writes(&collect(&mut e));
    assert_eq!(third.len(), 1);
    reply(&mut e, &third[0], status, body);
    let actions = collect(&mut e);
    assert!(writes(&actions).is_empty());
    unknown_outcome_and_journal(&actions, &["1001"]);
    if status == 200 {
        unknown_telemetry(&actions, "place");
    } else {
        engine_unknown_telemetry(&actions, third[0].id, Some(status));
    }
    late_unknown_ack_is_inert(&mut e, &third[0]);
}

// TS: packages/safe-cex/src/__tests__/reference/bingx/bingx.exchange.ts:1544-1564,1854-1889.
// KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: bingx-spot | quantity-minimum-and-notional
#[test]
fn market_buy_keeps_native_quote_conversion_and_defers_notional_limits() {
    let (mut e, _) = seed(true);
    let mut actions = submit_batch(
        &mut e,
        vec![json!({"symbol":"BTCUSDT","type":"market",
        "side":"buy","amount":20.,"price":41000})],
    );
    for (index, quote) in [374460.79, 374460.79, 71078.42].into_iter().enumerate() {
        let (body, next) = ack(&mut e, &actions, &format!("300{index}"));
        assert!(body.get("quantity").is_none());
        assert!(body.get("price").is_none());
        assert!((body["quoteOrderQty"].as_f64().unwrap() - quote).abs() < 0.000001);
        actions = next;
    }
    assert_eq!(done(&actions, 70), json!(["3000", "3001", "3002"]));

    let (mut e, _) = seed(true);
    let actions = submit_batch(
        &mut e,
        vec![
            batch_order(0.002, 41000., "prefix"),
            json!({"symbol":"BTCUSDT","type":"market","side":"buy","amount":0.002,
            "price":1_000_000_000.,"clientOrderId":"quote"}),
            batch_order(0.002, 41000., "suffix"),
        ],
    );
    let (_, actions) = ack(&mut e, &actions, "1001");
    let requests = writes(&actions);
    assert_eq!(requests.len(), 1);
    let body: Value = serde_json::from_str(requests[0].body.as_deref().unwrap()).unwrap();
    assert_eq!(body["quoteOrderQty"], 2_000_000.);
    reply(
        &mut e,
        &requests[0],
        200,
        r#"{"code":100440,"msg":"notional too large"}"#,
    );
    let actions = collect(&mut e);
    let (_, actions) = ack(&mut e, &actions, "1003");
    assert_eq!(
        failure(&actions).partial.as_ref().unwrap().order_ids,
        ["1001", "1003"]
    );
}
