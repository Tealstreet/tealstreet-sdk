//! Separate placement commands queued behind a write share the next signed
//! `order` action; each command still settles only its own rows (AGT-145).
mod support;

use safe_cex_engine::model::PlaceOrderOpts;
use safe_cex_engine::{Action, Command, CommandId, Engine, Event, HttpRequest, Notice, Response};
use safe_cex_venue_hyperliquid::Hyperliquid;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use support::{prepared_now, ready};

/// One signed round trip in the simulation, above the 334 ms pacing gap.
const ROUND_TRIP_MS: f64 = 400.;
const FIRST: CommandId = 100;

fn limit(symbol: &str, side: &str, price: f64) -> PlaceOrderOpts {
    serde_json::from_value(json!({
        "symbol": symbol, "type": "limit", "side": side, "amount": 0.001, "price": price
    }))
    .expect("fixture order")
}

/// The AGT-125 burst shape: far-from-market tiny limits, alternating markets.
fn tap(index: usize) -> PlaceOrderOpts {
    if index.is_multiple_of(2) {
        limit("BTC-PERP", "buy", 50. + index as f64)
    } else {
        limit("ETH-PERP", "sell", 400. + index as f64)
    }
}

#[derive(Debug)]
enum Terminal {
    Done(Vec<String>),
    Failed(Value),
}

#[derive(Default)]
struct Trace {
    /// Each `/exchange` action and the time it was released.
    writes: Vec<(f64, Value)>,
    terminals: BTreeMap<CommandId, (f64, Terminal)>,
    receipts: BTreeMap<CommandId, Vec<String>>,
    events: Vec<safe_cex_engine::log::LogEvent>,
}

impl Trace {
    fn done(&self, command: CommandId) -> &[String] {
        match &self.terminals[&command].1 {
            Terminal::Done(ids) => ids,
            other => panic!("{command}: {other:?}"),
        }
    }

    fn failed(&self, command: CommandId) -> &Value {
        match &self.terminals[&command].1 {
            Terminal::Failed(error) => error,
            other => panic!("{command}: {other:?}"),
        }
    }

    fn sizes(&self) -> Vec<usize> {
        self.writes
            .iter()
            .map(|(_, action)| action["orders"].as_array().map_or(0, Vec::len))
            .collect()
    }

    fn unknown_events(&self, command: CommandId) -> usize {
        self.events
            .iter()
            .filter(|event| {
                event.code == "order.outcome_unknown"
                    && event
                        .fields
                        .iter()
                        .any(|(key, value)| *key == "commandId" && *value == command.into())
            })
            .count()
    }
}

/// Sends one PlaceOrders command per entry, 1 ms apart, then answers each
/// write `ROUND_TRIP_MS` after its release with `answer(write index, action)`
/// (`None` loses the reply). Time only advances to the next reply.
fn burst(
    commands: Vec<Vec<PlaceOrderOpts>>,
    mut answer: impl FnMut(usize, &Value) -> Option<String>,
) -> Trace {
    let (mut engine, _) = ready("disabled");
    let count = commands.len();
    let start = 2000.;
    let mut trace = Trace::default();
    let mut inflight: Vec<(f64, HttpRequest, Value)> = Vec::new();
    for (index, orders) in commands.into_iter().enumerate() {
        engine.handle(
            start + index as f64,
            Event::Command(Command::PlaceOrders {
                command: FIRST + index as CommandId,
                orders,
            }),
        );
        collect(&mut engine, &mut trace, &mut inflight);
    }
    while trace.terminals.len() < count {
        inflight.sort_by(|a, b| a.0.total_cmp(&b.0));
        assert!(
            !inflight.is_empty(),
            "stalled: {:?}",
            trace.terminals.keys()
        );
        let (due, request, action) = inflight.remove(0);
        let index = trace
            .writes
            .iter()
            .position(|(_, written)| *written == action)
            .expect("written");
        match answer(index, &action) {
            Some(body) => engine.handle(
                due,
                Event::HttpResponse {
                    request: request.id,
                    response: Response {
                        status: 200,
                        headers: "",
                        body: &body,
                    },
                },
            ),
            None => engine.handle(
                due,
                Event::HttpFailed {
                    request: request.id,
                    error: "transport reply lost",
                },
            ),
        }
        collect(&mut engine, &mut trace, &mut inflight);
    }
    trace
}

fn collect(
    engine: &mut Engine<Hyperliquid>,
    trace: &mut Trace,
    inflight: &mut Vec<(f64, HttpRequest, Value)>,
) {
    let now = safe_cex_engine::test_support::dispatch_now(engine);
    for action in prepared_now(engine) {
        match action {
            Action::Http(request) if request.url.ends_with("/exchange") => {
                let body: Value =
                    serde_json::from_str(request.body.as_deref().expect("body")).expect("payload");
                trace.writes.push((now, body["action"].clone()));
                inflight.push((now + ROUND_TRIP_MS, request, body["action"].clone()));
            }
            Action::Notice(Notice::CommandDone { command, result }) => {
                let ids = serde_json::from_str(&result).expect("ids");
                assert!(
                    trace
                        .terminals
                        .insert(command, (now, Terminal::Done(ids)))
                        .is_none()
                );
            }
            Action::Notice(Notice::CommandFailed { command, error }) => {
                let error = serde_json::to_value(error).expect("error JSON");
                assert!(
                    trace
                        .terminals
                        .insert(command, (now, Terminal::Failed(error)))
                        .is_none()
                );
            }
            Action::Notice(Notice::CommandProgress { command, receipts }) => trace
                .receipts
                .entry(command)
                .or_default()
                .extend(receipts.into_iter().map(|receipt| receipt.identity)),
            Action::Notice(Notice::Event(event)) => trace.events.push(event),
            _ => {}
        }
    }
}

fn statuses(rows: Vec<Value>) -> Option<String> {
    Some(json!({"status":"ok","response":{"type":"order","data":{"statuses":rows}}}).to_string())
}

/// Every row rests, numbered `100 * (write + 1) + row`.
fn rest_all(index: usize, action: &Value) -> Option<String> {
    let count = action["orders"].as_array().expect("orders").len();
    statuses(
        (0..count)
            .map(|row| json!({"resting":{"oid": 100 * (index + 1) + row}}))
            .collect(),
    )
}

fn taps(count: usize) -> Vec<Vec<PlaceOrderOpts>> {
    (0..count).map(|index| vec![tap(index)]).collect()
}

#[test]
fn a_lone_order_is_sent_at_once_and_alone() {
    let trace = burst(taps(1), rest_all);
    assert_eq!(trace.sizes(), [1]);
    assert_eq!(
        trace.writes[0].0, 2000.,
        "released in the command's own turn"
    );
    assert_eq!(trace.writes[0].1["grouping"], "na");
    assert_eq!(trace.done(FIRST), ["100"]);
    assert_eq!(trace.terminals[&FIRST].0, 2000. + ROUND_TRIP_MS);
}

#[test]
fn separate_commands_queued_behind_a_write_share_the_next_order_action() {
    let commands = taps(25);
    let trace = burst(commands.clone(), rest_all);
    // The first goes at once; the 24 that queued behind it share one action.
    assert_eq!(trace.sizes(), [1, 24]);
    let wires = trace.writes[1].1["orders"].as_array().expect("orders");
    for (wire, orders) in wires.iter().zip(&commands[1..]) {
        let order = &orders[0];
        assert_eq!(wire["a"], if order.symbol == "BTC-PERP" { 0 } else { 1 });
        let price: f64 = wire["p"].as_str().expect("price").parse().unwrap();
        assert_eq!(price, order.price.unwrap(), "rows keep command order");
    }
    assert_eq!(trace.done(FIRST), ["100"]);
    for row in 0..24 {
        let command = FIRST + 1 + row as CommandId;
        assert_eq!(trace.done(command), [(200 + row).to_string()], "{command}");
    }
    // Each command learns only its own order's identity.
    for (index, (_, action)) in trace.writes.iter().enumerate() {
        for (row, wire) in action["orders"].as_array().unwrap().iter().enumerate() {
            let command = if index == 0 {
                FIRST
            } else {
                FIRST + 1 + row as CommandId
            };
            assert_eq!(trace.receipts[&command], [wire["c"].as_str().unwrap()]);
        }
    }
}

#[test]
fn burst_throughput_two_round_trips_instead_of_twenty_five() {
    let trace = burst(taps(25), rest_all);
    let last = trace
        .terminals
        .values()
        .map(|(at, _)| *at)
        .fold(0., f64::max);
    let first = trace.terminals[&FIRST].0;
    println!(
        "AGT-145 burst: 25 commands, {} signed writes, first result +{} ms, last result +{} ms \
         (round trip {ROUND_TRIP_MS} ms)",
        trace.writes.len(),
        first - 2000.,
        last - 2000.
    );
    assert_eq!(trace.writes.len(), 2);
    assert!(last - 2000. <= 2. * ROUND_TRIP_MS + 25., "{last}");
}

#[test]
fn one_refused_row_fails_only_its_own_command() {
    let trace = burst(taps(4), |index, action| {
        if index == 0 {
            return rest_all(index, action);
        }
        statuses(vec![
            json!({"resting":{"oid":201}}),
            json!({"error":"Insufficient margin to place order. asset=1"}),
            json!({"filled":{"totalSz":"0.001","avgPx":"52","oid":203}}),
        ])
    });
    assert_eq!(trace.sizes(), [1, 3], "no resend of any row");
    assert_eq!(trace.done(FIRST + 1), ["201"]);
    assert_eq!(trace.done(FIRST + 3), ["203"]);
    let error = trace.failed(FIRST + 2);
    assert_eq!(error["outcome"], "rejected");
    assert!(error.get("partial").is_none(), "{error}");
    let elements = error["elements"].as_array().expect("own rows");
    assert_eq!(elements.len(), 1);
    assert_eq!(elements[0]["index"], 0);
    assert_eq!(elements[0]["status"], "notSent");
    assert_eq!(elements[0]["error"]["outcome"], "rejected");
    assert_eq!(trace.unknown_events(FIRST + 2), 0);
}

#[test]
fn an_unproven_row_is_unknown_only_for_its_own_command() {
    for text in [
        json!("waitingForFill"),
        json!({"error":"New undocumented failure"}),
    ] {
        let trace = burst(taps(4), |index, action| {
            if index == 0 {
                return rest_all(index, action);
            }
            statuses(vec![
                json!({"resting":{"oid":201}}),
                text.clone(),
                json!({"resting":{"oid":203}}),
            ])
        });
        assert_eq!(trace.sizes(), [1, 3], "{text}: never resent");
        assert_eq!(trace.done(FIRST + 1), ["201"], "{text}");
        assert_eq!(trace.done(FIRST + 3), ["203"], "{text}");
        let error = trace.failed(FIRST + 2);
        assert_eq!(error["outcome"], "unknown", "{text}");
        assert_eq!(error["elements"][0]["status"], "unknown", "{text}");
        assert_eq!(trace.unknown_events(FIRST + 2), 1, "{text}");
        assert_eq!(trace.unknown_events(FIRST + 1), 0, "{text}");
    }
}

#[test]
fn a_lost_batch_reply_is_unknown_for_every_batched_command_and_never_resent() {
    let trace = burst(taps(5), |index, action| {
        (index == 0).then(|| rest_all(index, action)).flatten()
    });
    assert_eq!(trace.sizes(), [1, 4]);
    assert_eq!(trace.done(FIRST), ["100"]);
    for command in FIRST + 1..FIRST + 5 {
        let error = trace.failed(command);
        assert_eq!(error["outcome"], "unknown", "{command}");
        assert_ne!(error["outcome"], "notSent", "{command}");
        assert_eq!(trace.unknown_events(command), 1, "{command}");
    }
}

#[test]
fn a_short_batch_reply_proves_nothing_for_any_command() {
    let trace = burst(taps(3), |index, action| {
        if index == 0 {
            return rest_all(index, action);
        }
        statuses(vec![json!({"error":"Insufficient margin to place order."})])
    });
    assert_eq!(trace.sizes(), [1, 2]);
    for command in [FIRST + 1, FIRST + 2] {
        assert_eq!(trace.failed(command)["outcome"], "unknown", "{command}");
    }
}

#[test]
fn only_plain_placements_on_the_same_dex_join_and_queue_order_holds() {
    let mut stop: PlaceOrderOpts = serde_json::from_value(json!({
        "symbol":"BTC-PERP","type":"stop_market","side":"sell","amount":0.001,"price":40,
        "reduceOnly":true
    }))
    .unwrap();
    stop.client_order_id = None;
    let mut bracketed = limit("BTC-PERP", "buy", 60.);
    bracketed.stop_loss = Some(40.);
    let commands = vec![
        vec![tap(0)],
        vec![tap(1)],
        vec![tap(2)],
        vec![stop],
        vec![tap(3)],
        vec![bracketed],
        vec![tap(4)],
    ];
    let trace = burst(commands, |index, action| {
        let count = action["orders"].as_array().unwrap().len();
        statuses(
            (0..count)
                .map(|row| {
                    if action["grouping"] == "normalTpsl" && row > 0 {
                        json!("waitingForFill")
                    } else {
                        json!({"resting":{"oid": 100 * (index + 1) + row}})
                    }
                })
                .collect(),
        )
    });
    let shapes: Vec<_> = trace
        .writes
        .iter()
        .map(|(_, action)| {
            (
                action["grouping"].as_str().unwrap().to_owned(),
                action["orders"].as_array().unwrap().len(),
            )
        })
        .collect();
    assert_eq!(
        shapes,
        [
            ("na".into(), 1),
            ("na".into(), 2),
            ("na".into(), 1),
            ("na".into(), 1),
            ("normalTpsl".into(), 2),
            ("na".into(), 1),
        ]
    );
    assert!(trace.writes[2].1["orders"][0]["t"]["trigger"].is_object());
    assert!(
        trace
            .terminals
            .values()
            .all(|(_, t)| matches!(t, Terminal::Done(_)))
    );
}

#[test]
fn a_shared_action_holds_at_most_forty_orders() {
    let trace = burst(taps(45), rest_all);
    assert_eq!(trace.sizes(), [1, 40, 4]);
    assert_eq!(trace.done(FIRST + 41), ["300"]);
    assert_eq!(trace.done(FIRST + 44), ["303"]);
}

#[test]
fn a_multi_order_command_keeps_all_its_rows_together() {
    let commands = vec![vec![tap(0)], vec![tap(1), tap(2), tap(3)], vec![tap(4)]];
    let trace = burst(commands, |index, action| {
        if index == 0 {
            return rest_all(index, action);
        }
        statuses(vec![
            json!({"resting":{"oid":201}}),
            json!({"error":"Order has invalid price."}),
            json!({"resting":{"oid":203}}),
            json!({"resting":{"oid":204}}),
        ])
    });
    assert_eq!(trace.sizes(), [1, 4]);
    // Its own placed and refused rows: partial, never unsent.
    let error = trace.failed(FIRST + 1);
    assert_eq!(error["outcome"], "unknown");
    assert_eq!(error["partial"]["orderIds"], json!(["201", "203"]));
    assert_eq!(error["elements"].as_array().unwrap().len(), 3);
    assert_eq!(trace.done(FIRST + 2), ["204"]);
}
