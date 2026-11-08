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
#[test]
fn split_tail_below_min_sz_zero_writes() {
    not_sent(&mut ready(instrument()), vec![limit(11.)]);
}
#[test]
fn bracketed_split_tail_below_min_sz_zero_writes() {
    let mut row = limit(11.);
    row["stopLoss"] = json!(36000);
    row["takeProfit"] = json!(42000);
    not_sent(&mut ready(instrument()), vec![row]);
}
#[test]
fn invalid_last_row_in_second_batch_zero_writes() {
    let mut rows = vec![limit(5.); 20];
    rows.push(limit(1.));
    not_sent(&mut ready(instrument()), rows);
}
#[test]
fn oversized_standalone_algo_zero_writes() {
    for kind in [
        "stop_market",
        "take_profit_market",
        "trailing_stop_market",
        "conditional",
    ] {
        let row = json!({"symbol":"BTCUSDT","type":kind,"side":"sell","amount":6,
            "price":36000,"triggerPrice":37000});
        not_sent(&mut ready(instrument()), vec![limit(5.), row]);
    }
}
#[test]
fn native_minimum_is_in_contracts_even_when_public_minimum_is_scaled() {
    let mut product = instrument();
    product["ctVal"] = json!("0.01");
    not_sent(&mut ready(product), vec![limit(5.), limit(1.)]);
}
#[test]
fn protective_cap_cannot_leave_subminimum_parent_lots() {
    let mut product = instrument();
    product["maxStopSz"] = json!("3");
    let mut row = limit(5.);
    row["stopLoss"] = json!(36000);
    row["takeProfit"] = json!(42000);
    not_sent(&mut ready(product), vec![row]);
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
fn valid_brackets_keep_per_lot_entry_stop_take_profit_order() {
    let mut row = limit(15.);
    row["stopLoss"] = json!(36000);
    row["takeProfit"] = json!(42000);
    let (seen, requests) = finish(&mut ready(instrument()), vec![row]);
    assert_eq!(requests, 9);
    assert_eq!(seen.len(), 9);
    for lot in seen.chunks(3) {
        assert!(lot[0].0.ends_with("/batch-orders"));
        assert_eq!(lot[0].1["sz"], "5");
        assert_eq!(lot[1].1["slTriggerPx"], "36000");
        assert_eq!(lot[2].1["tpTriggerPx"], "42000");
        for (_, leg) in &lot[1..] {
            assert_eq!(leg["sz"], "5");
            assert_eq!(leg["reduceOnly"], true);
            assert_eq!(leg["side"], "sell");
        }
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
fn rounded_wire_quantity_is_checked_against_both_native_bounds() {
    let mut below = instrument();
    below["minSz"] = json!("4.04");
    below["lotSz"] = json!("0.1");
    not_sent(&mut ready(below), vec![limit(5.), limit(4.04)]);

    let mut above = instrument();
    above["maxStopSz"] = json!("5.45");
    above["lotSz"] = json!("0.1");
    not_sent(
        &mut ready(above),
        vec![
            limit(5.),
            json!({"symbol":"BTCUSDT","type":"stop_market","side":"sell","amount":5.45,"price":36000}),
        ],
    );
}

#[test]
fn standalone_algos_keep_their_order_type_cap_above_the_public_cap() {
    for kind in [
        "stop_market",
        "take_profit_market",
        "trailing_stop_market",
        "conditional",
    ] {
        let mut product = instrument();
        product["maxStopSz"] = json!("7");
        product["maxTriggerSz"] = json!("9");
        let amount = if kind == "conditional" { 8. } else { 6. };
        let (seen, requests) = finish(
            &mut ready(product),
            vec![
                json!({"symbol":"BTCUSDT","type":kind,"side":"sell","amount":amount,"price":36000,"triggerPrice":37000}),
            ],
        );
        assert_eq!(requests, 1);
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0].1["sz"], amount.to_string());
    }
}

#[test]
fn missing_specific_cap_keeps_the_conservative_native_bound() {
    let mut product = instrument();
    product["maxStopSz"] = json!("");
    not_sent(
        &mut ready(product),
        vec![
            limit(5.),
            json!({"symbol":"BTCUSDT","type":"stop_market","side":"sell","amount":6,"price":36000}),
        ],
    );
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
