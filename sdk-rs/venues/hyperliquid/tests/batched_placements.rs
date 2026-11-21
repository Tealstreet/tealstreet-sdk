//! One PlaceOrders command sends its plain placements as one signed `order` action.
mod support;

use safe_cex_engine::model::PlaceOrderOpts;
use safe_cex_engine::{Action, Command, Engine, Event, HttpRequest, Notice};
use safe_cex_venue_hyperliquid::Hyperliquid;
use serde_json::{Value, json};
use support::{advance_packet_timers, prepared_now, ready, reply};

fn limit(symbol: &str, side: &str, price: f64) -> PlaceOrderOpts {
    serde_json::from_value(json!({
        "symbol": symbol, "type": "limit", "side": side, "amount": 0.001, "price": price
    }))
    .expect("fixture order")
}

/// Far-from-market tiny limits, alternating markets and sides, each priced distinctly.
fn burst(count: usize) -> Vec<PlaceOrderOpts> {
    (0..count)
        .map(|index| {
            if index % 2 == 0 {
                limit("BTC-PERP", "buy", 50. + index as f64)
            } else {
                limit("ETH-PERP", "sell", 400. + index as f64)
            }
        })
        .collect()
}

struct Trace {
    writes: Vec<Value>,
    done: Option<String>,
    failed: Option<Value>,
}

/// Drives one command, answering each `/exchange` write with `answer(write index, action)`.
fn drive(
    orders: Vec<PlaceOrderOpts>,
    mut answer: impl FnMut(usize, &Value) -> Option<String>,
) -> Trace {
    let (mut engine, _) = ready("disabled");
    engine.handle(
        2000.,
        Event::Command(Command::PlaceOrders {
            command: 77,
            orders,
        }),
    );
    run(&mut engine, &mut answer)
}

fn run(
    engine: &mut Engine<Hyperliquid>,
    answer: &mut impl FnMut(usize, &Value) -> Option<String>,
) -> Trace {
    let mut trace = Trace {
        writes: Vec::new(),
        done: None,
        failed: None,
    };
    let mut actions = prepared_now(engine);
    for _ in 0..12 {
        for action in &actions {
            match action {
                Action::Http(request) if request.url.ends_with("/exchange") => {
                    let body: Value = serde_json::from_str(request.body.as_deref().expect("body"))
                        .expect("payload");
                    let index = trace.writes.len();
                    trace.writes.push(body["action"].clone());
                    match answer(index, &body["action"]) {
                        Some(reply_body) => reply(engine, request, 200, &reply_body),
                        None => lose(engine, request),
                    }
                }
                Action::Notice(Notice::CommandDone { command: 77, result }) => {
                    assert!(trace.done.is_none() && trace.failed.is_none());
                    trace.done = Some(result.clone());
                }
                Action::Notice(Notice::CommandFailed { command: 77, error }) => {
                    assert!(trace.done.is_none() && trace.failed.is_none());
                    trace.failed = Some(serde_json::to_value(error).expect("error JSON"));
                }
                _ => {}
            }
        }
        let next = prepared_now(engine);
        actions = advance_packet_timers(engine, next, 30_000.);
    }
    assert!(
        trace.done.is_some() || trace.failed.is_some(),
        "one terminal notice"
    );
    trace
}

fn lose(engine: &mut Engine<Hyperliquid>, request: &HttpRequest) {
    engine.handle(
        2500.,
        Event::HttpFailed {
            request: request.id,
            error: "transport reply lost",
        },
    );
}

fn statuses(rows: Vec<Value>) -> Option<String> {
    Some(
        json!({"status":"ok","response":{"type":"order","data":{"statuses":rows}}}).to_string(),
    )
}

fn resting_all(action: &Value, first_oid: u64) -> Option<String> {
    let count = action["orders"].as_array().expect("orders").len() as u64;
    statuses(
        (0..count)
            .map(|row| json!({"resting":{"oid": first_oid + row}}))
            .collect(),
    )
}

#[test]
fn burst_of_plain_limits_is_one_signed_order_action_in_caller_order() {
    let orders = burst(25);
    let trace = drive(orders.clone(), |_, action| resting_all(action, 1001));
    assert_eq!(trace.writes.len(), 1, "one round trip for the burst");
    let action = &trace.writes[0];
    assert_eq!(action["type"], "order");
    assert_eq!(action["grouping"], "na");
    let wires = action["orders"].as_array().expect("orders");
    assert_eq!(wires.len(), 25);
    for (wire, order) in wires.iter().zip(&orders) {
        assert_eq!(wire["a"], if order.symbol == "BTC-PERP" { 0 } else { 1 });
        assert_eq!(wire["b"], order.side == safe_cex_engine::model::OrderSide::Buy);
        assert_eq!(
            wire["p"].as_str().expect("price").parse::<f64>().unwrap(),
            order.price.unwrap()
        );
        assert_eq!(wire["r"], false);
        assert_eq!(wire["t"], json!({"limit":{"tif":"Gtc"}}));
    }
    let ids: Vec<String> = serde_json::from_str(&trace.done.expect("done")).expect("ids");
    let expected: Vec<String> = (1001..1026).map(|oid: u64| oid.to_string()).collect();
    assert_eq!(ids, expected, "each status maps to its own order, in order");
}

#[test]
fn one_refused_row_fails_only_itself() {
    let trace = drive(burst(3), |_, _| {
        statuses(vec![
            json!({"resting":{"oid":11}}),
            json!({"error":"Insufficient margin to place order. asset=1"}),
            json!({"filled":{"totalSz":"0.001","avgPx":"52","oid":13}}),
        ])
    });
    assert_eq!(trace.writes.len(), 1, "no resend of any row");
    let error = trace.failed.expect("partial command reports its rows");
    assert_eq!(error["partial"]["orderIds"], json!(["11", "13"]));
    let elements = error["elements"].as_array().expect("per-order outcomes");
    assert_eq!(elements.len(), 3);
    assert_eq!(elements[0]["status"], "placed");
    assert_eq!(elements[0]["orderIds"], json!(["11"]));
    assert_eq!(elements[1]["status"], "notSent");
    assert_eq!(elements[1]["error"]["outcome"], "rejected");
    assert_eq!(elements[2]["status"], "placed");
    assert_eq!(elements[2]["orderIds"], json!(["13"]));
}

#[test]
fn unlisted_row_error_is_unknown_only_for_that_row() {
    let trace = drive(burst(3), |_, _| {
        statuses(vec![
            json!({"resting":{"oid":11}}),
            json!({"resting":{"oid":12}}),
            json!({"error":"New undocumented failure"}),
        ])
    });
    assert_eq!(trace.writes.len(), 1);
    let error = trace.failed.expect("uncertain row fails the command");
    assert_eq!(error["outcome"], "unknown");
    let elements = error["elements"].as_array().expect("per-order outcomes");
    assert_eq!(elements[0]["status"], "placed");
    assert_eq!(elements[1]["status"], "placed");
    assert_eq!(elements[2]["status"], "unknown");
}

#[test]
fn one_error_row_for_a_batch_proves_nothing_and_is_never_resent() {
    let trace = drive(burst(3), |_, _| {
        statuses(vec![json!({"error":"Insufficient margin to place order."})])
    });
    assert_eq!(trace.writes.len(), 1);
    let error = trace.failed.expect("short reply fails");
    assert_eq!(error["outcome"], "unknown");
    for element in error["elements"].as_array().expect("rows") {
        assert_eq!(element["status"], "unknown", "{element}");
    }
}

#[test]
fn lost_batch_reply_is_unknown_for_every_row_and_never_resent() {
    let trace = drive(burst(5), |_, _| None);
    assert_eq!(trace.writes.len(), 1);
    let error = trace.failed.expect("lost reply fails");
    assert_eq!(error["outcome"], "unknown");
    let elements = error["elements"].as_array().expect("rows");
    assert_eq!(elements.len(), 5);
    assert!(elements.iter().all(|row| row["status"] == "unknown"));
}

#[test]
fn bursts_split_at_forty_orders_per_action() {
    let trace = drive(burst(45), |index, action| {
        resting_all(action, 1 + 100 * index as u64)
    });
    let sizes: Vec<_> = trace
        .writes
        .iter()
        .map(|action| action["orders"].as_array().unwrap().len())
        .collect();
    assert_eq!(sizes, [40, 5]);
    let ids: Vec<String> = serde_json::from_str(&trace.done.expect("done")).expect("ids");
    assert_eq!(ids.len(), 45);
    assert_eq!(ids[39], "40");
    assert_eq!(ids[40], "101");
}

#[test]
fn a_refused_chunk_stops_later_chunks_unsent() {
    let trace = drive(burst(42), |_, action| {
        let count = action["orders"].as_array().unwrap().len();
        statuses(
            (0..count)
                .map(|row| {
                    if row == 3 {
                        json!({"error":"Order must have minimum value of $10."})
                    } else {
                        json!({"resting":{"oid": row + 1}})
                    }
                })
                .collect(),
        )
    });
    assert_eq!(trace.writes.len(), 1, "a failed write halts later writes");
    let error = trace.failed.expect("failure");
    let elements = error["elements"].as_array().expect("rows");
    assert_eq!(elements.len(), 42);
    assert_eq!(elements[3]["status"], "notSent");
    assert_eq!(elements[3]["error"]["outcome"], "rejected");
    assert!(elements[40].get("error").is_none(), "never sent");
    assert_eq!(elements[41]["status"], "notSent");
}

#[test]
fn grouped_brackets_and_triggers_never_join_a_batch() {
    let mut bracketed = limit("BTC-PERP", "buy", 60.);
    bracketed.stop_loss = Some(40.);
    bracketed.take_profit = Some(90.);
    let mut stop: PlaceOrderOpts = serde_json::from_value(json!({
        "symbol":"BTC-PERP","type":"stop_market","side":"sell","amount":0.001,"price":40,
        "reduceOnly":true
    }))
    .unwrap();
    stop.client_order_id = None;
    let orders = vec![
        limit("BTC-PERP", "buy", 50.),
        limit("ETH-PERP", "sell", 400.),
        bracketed,
        limit("BTC-PERP", "buy", 51.),
        stop,
        limit("BTC-PERP", "buy", 52.),
        limit("ETH-PERP", "sell", 401.),
    ];
    let mut oid = 0;
    let trace = drive(orders, |_, action| {
        let count = action["orders"].as_array().unwrap().len();
        let rows = (0..count)
            .map(|row| {
                oid += 1;
                if action["grouping"] == "normalTpsl" && row > 0 {
                    json!("waitingForFill")
                } else {
                    json!({"resting":{"oid": oid}})
                }
            })
            .collect();
        statuses(rows)
    });
    let shapes: Vec<(String, usize)> = trace
        .writes
        .iter()
        .map(|action| {
            (
                action["grouping"].as_str().unwrap().to_owned(),
                action["orders"].as_array().unwrap().len(),
            )
        })
        .collect();
    assert_eq!(
        shapes,
        [
            ("na".into(), 2),
            ("normalTpsl".into(), 3),
            ("na".into(), 1),
            ("na".into(), 1),
            ("na".into(), 2),
        ]
    );
    assert!(trace.writes[3]["orders"][0]["t"]["trigger"].is_object());
    assert!(trace.done.is_some(), "{:?}", trace.failed);
}

#[test]
fn reduce_only_and_post_only_survive_per_row() {
    let mut reduce = limit("BTC-PERP", "sell", 500.);
    reduce.reduce_only = Some(true);
    let mut post = limit("ETH-PERP", "buy", 50.);
    post.time_in_force = Some(safe_cex_engine::model::TimeInForce::PostOnly);
    let trace = drive(
        vec![limit("BTC-PERP", "buy", 50.), reduce, post],
        |_, action| resting_all(action, 1),
    );
    assert_eq!(trace.writes.len(), 1);
    let wires = trace.writes[0]["orders"].as_array().unwrap();
    assert_eq!(wires[0]["r"], false);
    assert_eq!(wires[1]["r"], true);
    assert_eq!(wires[2]["t"], json!({"limit":{"tif":"Alo"}}));
}
