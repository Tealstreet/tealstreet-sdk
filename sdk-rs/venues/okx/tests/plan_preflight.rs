#[allow(dead_code)]
mod support;
use safe_cex_engine::{
    Action, Command, Engine, ErrorKind, Event, HttpRequest, Method, Notice, Outcome, VenueError,
};
use safe_cex_venue_okx::Okx;
use serde_json::{Value, json};
use support::*;

fn instrument() -> Value {
    let mut row: Value = serde_json::from_str(MARKET).expect("instrument fixture");
    for name in [
        "maxIcebergSz",
        "maxLmtSz",
        "maxMktSz",
        "maxStopSz",
        "maxTriggerSz",
        "maxTwapSz",
    ] {
        row[name] = json!("5");
    }
    row["ctVal"] = json!("1");
    row["lotSz"] = json!("1");
    row["minSz"] = json!("4");
    row
}
fn ready(row: Value) -> Engine<Okx> {
    trading_engine_at_level_with_instrument(
        FixtureOptions {
            key: Some("test-key".into()),
            secret: Some("test-secret".into()),
            passphrase: Some("test-passphrase".into()),
            ..Default::default()
        },
        |_| {},
        "2",
        &row.to_string(),
    )
}
fn limit(amount: f64) -> Value {
    json!({"symbol":"BTCUSDT","type":"limit","side":"buy","amount":amount,"price":38000})
}
fn submit(engine: &mut Engine<Okx>, rows: Vec<Value>) -> Vec<Action> {
    engine.handle(
        5.,
        Event::Command(Command::PlaceOrders {
            command: 17,
            orders: serde_json::from_value(json!(rows)).expect("order fixtures"),
        }),
    );
    prepared(engine, 5.)
}
fn terminals(actions: &[Action], id: u32) -> Vec<Result<String, VenueError>> {
    actions
        .iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::CommandDone { command, result }) if *command == id => {
                Some(Ok(result.clone()))
            }
            Action::Notice(Notice::CommandFailed { command, error }) if *command == id => {
                Some(Err(error.clone()))
            }
            _ => None,
        })
        .collect()
}
#[derive(Default)]
struct ScriptedTransport {
    requests: Vec<HttpRequest>,
}
impl ScriptedTransport {
    fn record(&mut self, actions: &[Action]) -> Vec<HttpRequest> {
        let requests: Vec<_> = actions
            .iter()
            .filter_map(|a| match a {
                Action::Http(request) => Some(request.clone()),
                _ => None,
            })
            .collect();
        self.requests.extend(requests.iter().cloned());
        requests
    }
    fn write_count(&self) -> usize {
        self.requests
            .iter()
            .filter(|r| r.method != Method::Get)
            .count()
    }
}
fn not_sent(engine: &mut Engine<Okx>, rows: Vec<Value>) {
    let mut actions = submit(engine, rows);
    for _ in 0..3 {
        actions.extend(prepared(engine, 5.));
    }
    let mut transport = ScriptedTransport::default();
    transport.record(&actions);
    assert_eq!(
        transport.requests.len(),
        0,
        "known-invalid plan reached physical transport"
    );
    let results = terminals(&actions, 17);
    assert_eq!(results.len(), 1);
    let error = results[0].as_ref().expect_err("whole intent must refuse");
    assert_eq!(error.kind, Some(ErrorKind::InvalidInput));
    assert_eq!(error.outcome, Some(Outcome::NotSent));
}
fn finish(engine: &mut Engine<Okx>, rows: Vec<Value>) -> (Vec<(String, Value)>, usize) {
    let mut actions = submit(engine, rows);
    let mut seen = vec![];
    let mut now = 6.;
    let mut transport = ScriptedTransport::default();
    loop {
        let writes: Vec<_> = transport
            .record(&actions)
            .into_iter()
            .filter(|r| r.method != Method::Get)
            .collect();
        if writes.is_empty() {
            let results = terminals(&actions, 17);
            assert_eq!(results.len(), 1, "intent must terminate");
            let ids: Vec<String> =
                serde_json::from_str(results[0].as_ref().expect("valid control must succeed"))
                    .expect("result IDs");
            assert_eq!(
                ids,
                (0..seen.len())
                    .map(|i| (1000 + i).to_string())
                    .collect::<Vec<_>>()
            );
            assert!(
                prepared(engine, now)
                    .iter()
                    .all(|a| !matches!(a,Action::Http(r) if r.method!=Method::Get))
            );
            return (seen, transport.write_count());
        }
        assert_eq!(writes.len(), 1, "one physical request at a time");
        let request = &writes[0];
        let payload: Value = serde_json::from_str(request.body.as_deref().expect("request body"))
            .expect("JSON payload");
        let native: Vec<_> = match payload {
            Value::Array(rows) => rows,
            row => vec![row],
        };
        assert!(native.len() <= 20);
        let mut replies = vec![];
        for row in native {
            let id = (1000 + seen.len()).to_string();
            let algo = request.url.ends_with("/order-algo");
            replies.push(if algo {
                json!({"algoId":id,"sCode":"0","sMsg":""})
            } else {
                json!({"ordId":id,"sCode":"0","sMsg":""})
            });
            seen.push((request.url.clone(), row));
        }
        let response = json!({"code":"0","msg":"","data":replies}).to_string();
        reply(engine, &Action::Http(request.clone()), &response, now);
        actions = prepared(engine, now);
        now += 1.;
        assert!(seen.len() <= 100, "finite valid fixture");
    }
}
#[test]
fn valid_split_and_chunk_controls_place_each_element_once() {
    let (seen, requests) = finish(&mut ready(instrument()), vec![limit(15.)]);
    assert_eq!(requests, 1);
    assert_eq!(seen.len(), 3);
    assert!(seen.iter().all(|(_, row)| row["sz"] == "5"));
    let mut rows = vec![];
    for i in 0..41 {
        let mut row = limit(5.);
        row["clientOrderId"] = json!(format!("Caller{i}"));
        rows.push(row);
    }
    let (seen, requests) = finish(&mut ready(instrument()), rows);
    assert_eq!(requests, 3);
    assert_eq!(seen.len(), 41);
    for (i, (_, row)) in seen.iter().enumerate() {
        assert_eq!(row["clOrdId"], format!("Caller{i}"));
    }
}
#[test]
fn ts_pin_split_brackets_batch_entries_before_full_amount_children() {
    // TS okx.exchange.ts:1751-1763; exchanges/base.ts:1453-1476 derives original-amount children.
    // KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: okx | trading.rs:669 per-lot TP/SL child validation
    let mut row = limit(15.);
    row["stopLoss"] = json!(36000);
    row["takeProfit"] = json!(42000);
    let (seen, requests) = finish(&mut ready(instrument()), vec![row]);
    assert_eq!(requests, 3, "TS sends one entry batch, one SL, then one TP");
    assert_eq!(seen.len(), 5);
    for (path, row) in &seen[..3] {
        assert!(path.ends_with("/batch-orders"));
        assert_eq!(row["sz"], "5");
    }
    assert_eq!(seen[3].1["slTriggerPx"], "36000");
    assert_eq!(seen[4].1["tpTriggerPx"], "42000");
    for (path, leg) in &seen[3..] {
        assert!(path.ends_with("/order-algo"));
        assert_eq!(leg["sz"], "15");
        assert_eq!(leg["reduceOnly"], true);
        assert_eq!(leg["side"], "sell");
    }
}
#[test]
fn full_position_algo_zero_control() {
    for kind in ["stop_market", "take_profit_market"] {
        let (seen, requests) = finish(
            &mut ready(instrument()),
            vec![json!({"symbol":"BTCUSDT","type":kind,"side":"sell","amount":0,"price":36000})],
        );
        assert_eq!(requests, 1);
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0].1["closeFraction"], "1");
        assert!(seen[0].1.get("sz").is_none());
    }
}

#[test]
fn ts_pin_mixed_algos_follow_entries_with_stable_stop_trailing_tp_priority() {
    // TS okx.exchange.ts:1758-1763,2395-2400 sorts supplied and derived algos together, stably.
    // KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: okx | trading.rs:669 per-lot TP/SL child validation
    let mut bracket = limit(11.);
    bracket["stopLoss"] = json!(36000);
    bracket["takeProfit"] = json!(42000);
    let (seen, requests) = finish(
        &mut ready(instrument()),
        vec![
            json!({"symbol":"BTCUSDT","type":"conditional","side":"buy","amount":5,"price":38000,"triggerPrice":37000}),
            bracket,
            json!({"symbol":"BTCUSDT","type":"stop_market","side":"sell","amount":4,"price":35000}),
            json!({"symbol":"BTCUSDT","type":"take_profit_market","side":"sell","amount":4,"price":43000}),
            json!({"symbol":"BTCUSDT","type":"trailing_stop_market","side":"sell","amount":8,"price":36000}),
        ],
    );
    assert_eq!(requests, 7);
    assert_eq!(seen.len(), 9);
    assert!(
        seen[..3]
            .iter()
            .all(|(path, _)| path.ends_with("/batch-orders"))
    );
    // This is a priority pin only: inherited Rust trailing bytes differ from TS :2203-2208.
    let priority: Vec<_> = seen[3..]
        .iter()
        .map(|(path, row)| {
            assert!(path.ends_with("/order-algo"));
            if row.get("slTriggerPx").is_some() {
                "SL"
            } else if row.get("tpTriggerPx").is_some() {
                "TP"
            } else if row["ordType"] == "move_order_stop" {
                "trailing"
            } else {
                "conditional"
            }
        })
        .collect();
    assert_eq!(
        priority,
        ["SL", "SL", "trailing", "TP", "TP", "conditional"]
    );
    for (index, size, field, price) in [
        (3, "4", "slTriggerPx", "35000"),
        (4, "11", "slTriggerPx", "36000"),
        (6, "4", "tpTriggerPx", "43000"),
        (7, "11", "tpTriggerPx", "42000"),
        (8, "5", "triggerPx", "37000"),
    ] {
        assert_eq!(seen[index].1["sz"], size);
        assert_eq!(seen[index].1[field], price);
    }
}

#[test]
fn legacy_standalone_algos_do_not_add_a_native_cap_gate() {
    // TS okx.exchange.ts:2133-2211,2318-2343: pin released native writes, without a new gate.
    // KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: okx | standalone algo native cap and positive SL/TP minimum
    for kind in ["stop_market", "take_profit_market", "conditional"] {
        let mut product = instrument();
        product["maxStopSz"] = json!("7");
        product["maxTriggerSz"] = json!("9");
        let amount = if kind == "conditional" { 10. } else { 8. };
        let (seen, requests) = finish(
            &mut ready(product),
            vec![
                json!({"symbol":"BTCUSDT","type":kind,"side":"sell","amount":amount,"price":36000,"triggerPrice":37000}),
            ],
        );
        assert_eq!(requests, 1);
        assert_eq!(seen.len(), 1);
        assert!(seen[0].0.ends_with("/order-algo"));
        assert_eq!(seen[0].1["sz"], amount.to_string());
    }
}

#[test]
fn inherited_trailing_payload_control_is_not_a_ts_wire_parity_pin() {
    // TS okx.exchange.ts:2203-2208 uses closeFraction, omits sz, and sends price as spread.
    // Rust's existing native sz/distance behavior is deliberately documented separately.
    let (seen, requests) = finish(
        &mut ready(instrument()),
        vec![
            json!({"symbol":"BTCUSDT","type":"trailing_stop_market","side":"sell","amount":8,"price":36000}),
        ],
    );
    assert_eq!(requests, 1);
    assert_eq!(seen.len(), 1);
    assert!(seen[0].0.ends_with("/order-algo"));
    assert_eq!(seen[0].1["sz"], "8");
    assert_eq!(seen[0].1["callbackSpread"], "4000");
    assert!(seen[0].1.get("closeFraction").is_none());
}

#[test]
fn ts_positive_subminimum_standalone_sl_tp_sends_one_native_algo_and_its_result() {
    // TS okx.exchange.ts:2181-2200 emits positive SL/TP sizes without comparing minSz.
    // KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: okx | standalone algo native cap and positive SL/TP minimum
    for (kind, field, price) in [
        ("stop_market", "slTriggerPx", 36000),
        ("take_profit_market", "tpTriggerPx", 42000),
    ] {
        let (seen, requests) = finish(
            &mut ready(instrument()),
            vec![json!({"symbol":"BTCUSDT","type":kind,"side":"sell","amount":1,"price":price})],
        );
        assert_eq!(requests, 1);
        assert_eq!(seen.len(), 1);
        assert!(seen[0].0.ends_with("/order-algo"));
        assert_eq!(seen[0].1["sz"], "1");
        assert_eq!(seen[0].1[field], price.to_string());
        assert_eq!(seen[0].1["reduceOnly"], true);
        assert!(seen[0].1.get("closeFraction").is_none());
    }
}

#[test]
fn valid_wire_rounding_and_reduce_only_caller_identity_are_preserved() {
    let mut product = instrument();
    product["lotSz"] = json!("0.1");
    let mut row = limit(4.96);
    row["price"] = json!(38000.04);
    row["clientOrderId"] = json!("CallerRound");
    row["reduceOnly"] = json!(true);
    let (seen, requests) = finish(&mut ready(product), vec![row]);
    assert_eq!(requests, 1);
    assert_eq!(seen[0].1["sz"], "5");
    assert_eq!(seen[0].1["px"], "38000");
    assert_eq!(seen[0].1["reduceOnly"], true);
    assert_eq!(seen[0].1["clOrdId"], "CallerRound");
}

fn runtime_failure(status: u16, code: &str, expected: Outcome) {
    // TS okx.exchange.ts:1763,2319-2339: all entry lots precede the first child refusal.
    // KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: okx | trading.rs:669 per-lot TP/SL child validation
    let mut engine = ready(instrument());
    let mut row = limit(15.);
    row["stopLoss"] = json!(36000);
    row["takeProfit"] = json!(42000);
    let mut transport = ScriptedTransport::default();
    let first = transport.record(&submit(&mut engine, vec![row]));
    let [entry] = first.as_slice() else {
        panic!("first entry only")
    };
    assert!(entry.url.ends_with("/batch-orders"));
    let body: Value =
        serde_json::from_str(entry.body.as_deref().expect("entry body")).expect("JSON");
    assert_eq!(body.as_array().expect("batch").len(), 3);
    assert!(
        body.as_array()
            .expect("batch")
            .iter()
            .all(|row| row["sz"] == "5")
    );
    reply(
        &mut engine,
        &Action::Http(entry.clone()),
        r#"{"code":"0","data":[{"ordId":"1000","sCode":"0"},{"ordId":"1001","sCode":"0"},{"ordId":"1002","sCode":"0"}]}"#,
        6.,
    );
    let second = transport.record(&prepared(&mut engine, 6.));
    let [stop] = second.as_slice() else {
        panic!("one protective write")
    };
    assert!(stop.url.ends_with("/order-algo"));
    let stop_body: Value =
        serde_json::from_str(stop.body.as_deref().expect("SL body")).expect("JSON");
    assert_eq!(stop_body["sz"], "15");
    assert_eq!(stop_body["slTriggerPx"], "36000");
    let body = json!({"code":"1","msg":"scripted failure","data":[{
        "algoId":"","sCode":code,"sMsg":"scripted failure"
    }]})
    .to_string();
    if status == 0 {
        engine.handle(
            7.,
            Event::HttpFailed {
                request: stop.id,
                error: "lost reply",
            },
        );
    } else {
        engine.handle(
            7.,
            Event::HttpResponse {
                request: stop.id,
                response: safe_cex_engine::Response {
                    status,
                    headers: "",
                    body: &body,
                },
            },
        );
    }
    let mut output = prepared(&mut engine, 7.);
    output.extend(prepared(&mut engine, 8.));
    transport.record(&output);
    assert_eq!(
        transport.write_count(),
        2,
        "no suffix, resend, cancel or rollback"
    );
    let results = terminals(&output, 17);
    let [Err(error)] = results.as_slice() else {
        panic!("one failed terminal")
    };
    assert!(matches!(error.outcome, None | Some(Outcome::Unknown)));
    if expected == Outcome::Unknown {
        assert_unknown_message(error, ENGINE_UNKNOWN_TEXT);
    }
    let elements = error.elements.as_ref().expect("complete shared journal");
    assert_eq!(elements.len(), 5);
    assert_eq!(
        elements
            .iter()
            .map(|row| row.element.index)
            .collect::<Vec<_>>(),
        (0..5).collect::<Vec<_>>()
    );
    assert!(
        elements
            .iter()
            .all(|row| row.element.client_order_id.is_some())
    );
    use safe_cex_engine::write_plan::ElementResult;
    for (index, element) in elements.iter().take(3).enumerate() {
        assert_eq!(
            element.result,
            ElementResult::Placed {
                order_ids: vec![format!("{}", 1000 + index)]
            }
        );
    }
    match (&elements[3].result, expected) {
        (ElementResult::NotSent { error: Some(error) }, Outcome::Rejected) => {
            assert_eq!(error.outcome, Some(Outcome::Rejected));
            assert_eq!(error.venue_code.as_deref(), Some(code));
        }
        (ElementResult::Unknown { error }, Outcome::Unknown) => {
            assert_unknown_message(error, ENGINE_UNKNOWN_TEXT);
        }
        _ => panic!("native failure classification must survive"),
    }
    assert!(
        elements
            .iter()
            .skip(4)
            .all(|row| row.result == ElementResult::NotSent { error: None })
    );
    assert_eq!(
        error.partial.as_ref().expect("accepted prefix").order_ids,
        ["1000", "1001", "1002"]
    );
}

#[test]
fn runtime_rejection_reports_prefix_failed_element_and_unsent_suffix() {
    runtime_failure(200, "51008", Outcome::Rejected);
}

#[test]
fn legacy_native_placement_failure_stops_every_remaining_batch_and_algo() {
    // TS okx.exchange.ts:1763,2291-2311,2319-2339: pin released native writes, without a new gate.
    // KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: okx | trading.rs:723 later-row/20-row batch boundary
    let mut bracket = limit(15.);
    bracket["stopLoss"] = json!(36000);
    bracket["takeProfit"] = json!(42000);
    let mut cases = vec![vec![limit(11.)], vec![limit(5.); 21], vec![bracket]];
    for kind in [
        "conditional",
        "stop_market",
        "take_profit_market",
        "trailing_stop_market",
    ] {
        let row = json!({"symbol":"BTCUSDT","type":kind,"side":"sell","amount":8,
            "price":36000,"triggerPrice":37000});
        cases.push(vec![row.clone(), row]);
    }
    for rows in cases {
        let mut engine = ready(instrument());
        let mut transport = ScriptedTransport::default();
        let first = transport.record(&submit(&mut engine, rows));
        let [request] = first.as_slice() else {
            panic!("one initial native write")
        };
        let payload: Value =
            serde_json::from_str(request.body.as_deref().expect("payload")).expect("native JSON");
        let count = payload.as_array().map_or(1, Vec::len);
        let algo = request.url.ends_with("/order-algo");
        let replies: Vec<_> = (0..count)
            .map(|index| {
                if index + 1 == count {
                    json!({"ordId":"","algoId":"","sCode":"51008","sMsg":"refused"})
                } else if algo {
                    json!({"algoId":format!("native{index}"),"sCode":"0"})
                } else {
                    json!({"ordId":format!("native{index}"),"sCode":"0"})
                }
            })
            .collect();
        reply(
            &mut engine,
            &Action::Http(request.clone()),
            &json!({"code":"0","data":replies}).to_string(),
            6.,
        );
        let mut output = prepared(&mut engine, 6.);
        output.extend(prepared(&mut engine, 7.));
        transport.record(&output);
        assert_eq!(transport.write_count(), 1, "TS stops the native series");
        assert!(matches!(terminals(&output, 17).as_slice(), [Err(_)]));
    }
}

#[test]
fn runtime_ambiguous_native_reply_preserves_unknown_and_unsent_suffix() {
    // PARITY-EXCEPTION: stop-after-UNKNOWN (Sean/daxx, 2026-11-08); TS continues at okx.exchange.ts:2325-2327 for code-0 rows carrying algoId.
    runtime_failure(200, "50004", Outcome::Unknown);
}

#[test]
fn engine_owned_server_failure_preserves_unknown_and_unsent_suffix() {
    // PARITY-EXCEPTION: stop-after-UNKNOWN (Sean/daxx, 2026-11-08); TS continues at utils/signatureRejection.ts:19-40 for signature-looking responses.
    runtime_failure(503, "50001", Outcome::Unknown);
}

#[test]
fn lost_reply_preserves_unknown_and_unsent_suffix() {
    // PARITY-EXCEPTION: stop-after-UNKNOWN (Sean/daxx, 2026-11-08); TS continues at okx.exchange.ts:2325-2327 for uncertain rows carrying algoId.
    runtime_failure(0, "", Outcome::Unknown);
}

fn mixed_batch_failure(code: &str, rejected: bool) {
    use safe_cex_engine::write_plan::ElementResult;
    let mut engine = ready(instrument());
    let orders = (0..41)
        .map(|index| {
            let mut row = limit(5.);
            row["clientOrderId"] = json!(format!("Batch{index}"));
            row
        })
        .collect();
    let mut transport = ScriptedTransport::default();
    let first = transport.record(&submit(&mut engine, orders));
    let [first] = first.as_slice() else {
        panic!("one first batch")
    };
    let acks: Vec<_> = (0..20)
        .map(|i| json!({"ordId":format!("native{i}"), "sCode":"0", "sMsg":""}))
        .collect();
    reply(
        &mut engine,
        &Action::Http(first.clone()),
        &json!({"code":"0","data":acks}).to_string(),
        6.,
    );
    let second = transport.record(&prepared(&mut engine, 6.));
    let [second] = second.as_slice() else {
        panic!("one second batch")
    };
    let acks: Vec<_> = (20..40)
        .map(|i| {
            if i == 20 {
                json!({"ordId":"", "sCode":code, "sMsg":"scripted row failure"})
            } else {
                json!({"ordId":format!("native{i}"), "sCode":"0", "sMsg":""})
            }
        })
        .collect();
    reply(
        &mut engine,
        &Action::Http(second.clone()),
        &json!({"code":"2","data":acks}).to_string(),
        7.,
    );
    let mut out = prepared(&mut engine, 7.);
    out.extend(prepared(&mut engine, 8.));
    transport.record(&out);
    assert_eq!(transport.write_count(), 2, "third batch must stay unsent");
    let results = terminals(&out, 17);
    let [Err(error)] = results.as_slice() else {
        panic!("one partial terminal")
    };
    if !rejected {
        assert_unknown_message(error, ENGINE_UNKNOWN_TEXT);
    }
    let elements = error.elements.as_ref().expect("complete row journal");
    assert_eq!(elements.len(), 41);
    for (i, element) in elements.iter().enumerate() {
        assert_eq!(element.element.index, i);
        assert_eq!(
            element.element.client_order_id.as_deref(),
            Some(format!("Batch{i}").as_str())
        );
        if i == 20 {
            if rejected {
                assert!(
                    matches!(&element.result, ElementResult::NotSent { error: Some(e) } if e.outcome == Some(Outcome::Rejected) && e.venue_code.as_deref() == Some(code))
                );
            } else {
                let ElementResult::Unknown { error } = &element.result else {
                    panic!("unknown row")
                };
                assert_unknown_message(error, ENGINE_UNKNOWN_TEXT);
            }
        } else if i == 40 {
            assert_eq!(element.result, ElementResult::NotSent { error: None });
        } else {
            assert_eq!(
                element.result,
                ElementResult::Placed {
                    order_ids: vec![format!("native{i}")]
                }
            );
        }
    }
}

#[test]
fn native_batch_rejection_keeps_successful_rows_and_only_stops_unsent_writes() {
    mixed_batch_failure("51008", true);
}

#[test]
fn ambiguous_native_batch_row_keeps_other_proven_rows() {
    // PARITY-EXCEPTION: stop-after-UNKNOWN (Sean/daxx, 2026-11-08); TS continues at okx.exchange.ts:2297-2299 for code-0 rows carrying ordId.
    mixed_batch_failure("50004", false);
}

#[test]
fn ts_zero_adjusted_last_ordinary_row_refuses_before_any_write() {
    // TS okx.exchange.ts:1751-1755,2238-2242: formatter refusal precedes every native write.
    for amount in [0., 0.1] {
        not_sent(&mut ready(instrument()), vec![limit(5.), limit(amount)]);
    }
}

#[test]
fn ts_conditional_invalid_amount_or_trigger_refuses_before_any_io() {
    // TS okx.exchange.ts:1759-1763,2162-2172 checks all conditional amounts/triggers before POST.
    for (amount, trigger) in [(0., 37000.), (-1., 37000.), (5., 0.), (5., -1.)] {
        not_sent(
            &mut ready(instrument()),
            vec![
                limit(5.),
                json!({"symbol":"BTCUSDT","type":"conditional","side":"buy",
                "amount":amount,"price":38000,"triggerPrice":trigger}),
            ],
        );
    }
}

#[test]
fn ts_missing_conditional_trigger_refuses_before_any_write() {
    // TS okx.exchange.ts:1759-1763,2162-2172: formatter refusal precedes every native write.
    not_sent(
        &mut ready(instrument()),
        vec![
            limit(5.),
            json!({"symbol":"BTCUSDT","type":"conditional","side":"buy","amount":5,"price":38000}),
        ],
    );
}

#[test]
fn ts_invalid_ordinary_market_rules_refuse_before_any_write() {
    // TS okx.exchange.ts:1751,2226-2235: formatter refusal precedes every native write.
    let mut engine = ready(instrument());
    engine
        .store_mut()
        .update_markets(|market| market.limits.amount.max = f64::INFINITY);
    not_sent(&mut engine, vec![limit(5.)]);
}

#[test]
fn ts_invalid_ordinary_amount_precision_refuses_before_any_io() {
    // TS okx.exchange.ts:2226-2235 refuses invalid amount precision before dispatch.
    for precision in [0., -1., f64::INFINITY, f64::NEG_INFINITY, f64::NAN] {
        let mut engine = ready(instrument());
        engine
            .store_mut()
            .update_markets(|market| market.precision.amount = precision);
        not_sent(&mut engine, vec![limit(5.)]);
    }
}

fn first_native_rows(engine: &mut Engine<Okx>, rows: Vec<Value>) -> Value {
    let output = submit(engine, rows);
    let mut transport = ScriptedTransport::default();
    let requests = transport.record(&output);
    let [request] = requests.as_slice() else {
        panic!("one native request")
    };
    assert!(
        terminals(&output, 17).is_empty(),
        "legacy valid formatter input reaches transport"
    );
    serde_json::from_str(request.body.as_deref().expect("native body")).expect("native JSON")
}

#[test]
fn legacy_split_tail_below_native_minimum_is_not_newly_refused() {
    // TS okx.exchange.ts:2266-2284,2290-2295: pin released native writes, without a new gate.
    // KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: okx | trading.rs:645 split-tail/native-minQty
    let body = first_native_rows(&mut ready(instrument()), vec![limit(11.)]);
    assert_eq!(
        body.as_array()
            .expect("native batch")
            .iter()
            .map(|row| &row["sz"])
            .collect::<Vec<_>>(),
        vec![&json!("5"), &json!("5"), &json!("1")]
    );
}

#[test]
fn legacy_later_native_batch_row_is_not_prevalidated_against_minimum() {
    // TS okx.exchange.ts:2290-2315: pin released native writes, without a new gate.
    // KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: okx | trading.rs:723 later-row/20-row batch boundary
    let mut engine = ready(instrument());
    let mut rows = vec![limit(5.); 20];
    rows.push(limit(1.));
    let output = submit(&mut engine, rows);
    let mut transport = ScriptedTransport::default();
    let first = transport.record(&output);
    let [request] = first.as_slice() else {
        panic!("first batch")
    };
    let acks: Vec<_> = (0..20)
        .map(|i| json!({"ordId":format!("native{i}"),"sCode":"0"}))
        .collect();
    reply(
        &mut engine,
        &Action::Http(request.clone()),
        &json!({"code":"0","data":acks}).to_string(),
        6.,
    );
    let output = prepared(&mut engine, 6.);
    let second = transport.record(&output);
    let [request] = second.as_slice() else {
        panic!("later native batch")
    };
    let body: Value = serde_json::from_str(request.body.as_deref().expect("body")).expect("JSON");
    assert_eq!(body[0]["sz"], "1");
    assert_eq!(transport.write_count(), 2);
}

#[test]
fn legacy_protective_child_native_bounds_are_not_a_new_gate() {
    // TS okx.exchange.ts:1758-1763,2181-2200: pin released native writes, without a new gate.
    // KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: okx | trading.rs:669 per-lot TP/SL child validation
    let mut product = instrument();
    product["maxStopSz"] = json!("3");
    let mut row = limit(10.);
    row["stopLoss"] = json!(36000);
    row["takeProfit"] = json!(42000);
    let (seen, requests) = finish(&mut ready(product), vec![row]);
    assert_eq!(requests, 3);
    assert_eq!(seen.len(), 6);
    assert!(
        seen[..4]
            .iter()
            .all(|(path, _)| path.ends_with("/batch-orders"))
    );
    assert_eq!(
        seen[..4]
            .iter()
            .map(|(_, row)| row["sz"].as_str().expect("size"))
            .collect::<Vec<_>>(),
        vec!["3", "3", "3", "1"]
    );
    assert!(
        seen[4..]
            .iter()
            .all(|(path, row)| path.ends_with("/order-algo") && row["sz"] == "10")
    );
    assert_eq!(seen[4].1["slTriggerPx"], "36000");
    assert_eq!(seen[5].1["tpTriggerPx"], "42000");
}
