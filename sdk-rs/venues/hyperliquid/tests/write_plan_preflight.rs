// Rule 5 (Sam, 2026-11-20): display wording only; outcome and no-retry checks stay.
mod support;

use safe_cex_engine::model::PlaceOrderOpts;
use safe_cex_engine::{Action, Command, Engine, Event, HttpRequest, Notice, Outcome};
use safe_cex_venue_hyperliquid::Hyperliquid;
use serde_json::{Value, json};
use support::{advance_packet_timers, prepared_now, ready, reply};

#[derive(Default)]
struct ScriptedTransport {
    requests: Vec<HttpRequest>,
}
impl ScriptedTransport {
    fn record(&mut self, actions: &[Action]) {
        self.requests
            .extend(actions.iter().filter_map(|action| match action {
                Action::Http(request) => Some(request.clone()),
                _ => None,
            }));
    }
}

fn order(amount: f64) -> PlaceOrderOpts {
    serde_json::from_value(json!({
        "symbol": "BTC-PERP", "type": "limit", "side": "buy",
        "amount": amount, "price": 100
    }))
    .expect("fixture order")
}

/// A standalone stop is never batched, so it keeps its neighbours in writes of their own.
/// Plain rows on one DEX share one order action since feat/hyperliquid-batched-placements;
/// the serial-continuation pins below put a stop between plain rows to keep one row per write.
fn separator() -> PlaceOrderOpts {
    serde_json::from_value(json!({
        "symbol": "BTC-PERP", "type": "stop_market", "side": "sell",
        "amount": 3, "price": 80
    }))
    .expect("fixture stop")
}

fn configured() -> Engine<Hyperliquid> {
    let (mut engine, _) = ready("disabled");
    engine.store_mut().update_markets(|market| {
        market.precision.amount = 1.;
        market.limits.amount.min = 2.;
        market.limits.amount.max = 5.;
    });
    engine
}

#[test]
fn legacy_serial_dispatch_does_not_add_native_rule_gates() {
    // TS hyperliquid.exchange.ts:3639,3727-3758,4439-4442.
    // KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: hyperliquid | serial-placement-batch/audit trading.rs:367
    for last in [
        order(1.),
        order(0.4),
        order(6.),
        {
            let mut last = order(3.);
            last.price = Some(3.);
            last
        },
        {
            let mut last = order(3.);
            last.price = Some(0.000000001);
            last
        },
        {
            let mut last = order(3.);
            last.stop_loss = Some(0.000000001);
            last
        },
    ] {
        let legs = [last.stop_loss, last.take_profit]
            .into_iter()
            .flatten()
            .filter(|price| *price > 0.)
            .count();
        let payloads = submit_valid(vec![order(3.), last]);
        // Plain rows now share one action (feat/hyperliquid-batched-placements); every row
        // is still formatted and sent without min/max/notional gates.
        let rows: usize = payloads
            .iter()
            .map(|payload| payload["action"]["orders"].as_array().expect("rows").len())
            .sum();
        assert_eq!(
            rows,
            2 + legs,
            "every row is sent without native rule gates"
        );
    }
}

#[test]
fn caller_id_validation_still_refuses_entire_batch() {
    // TS exchanges/base.ts:1173-1200; hyperliquid.exchange.ts:4440 checks the whole batch before I/O.
    let mut engine = configured();
    let mut last = order(3.);
    last.client_order_id = Some("invalid".into());
    engine.handle(
        2000.,
        Event::Command(Command::PlaceOrders {
            command: 77,
            orders: vec![order(3.), last],
        }),
    );
    let actions = prepared_now(&mut engine);
    assert!(
        !actions
            .iter()
            .any(|action| matches!(action, Action::Http(_)))
    );
    assert!(actions.iter().any(|action| matches!(action,
        Action::Notice(Notice::CommandFailed { command: 77, error })
        if error.outcome == Some(Outcome::NotSent) && error.message.contains("client order ID"))));
}

fn submit_valid(orders: Vec<PlaceOrderOpts>) -> Vec<Value> {
    let mut engine = configured();
    engine.handle(
        2000.,
        Event::Command(Command::PlaceOrders {
            command: 77,
            orders,
        }),
    );
    let mut actions = prepared_now(&mut engine);
    let mut transport = ScriptedTransport::default();
    let mut payloads = Vec::new();
    for _ in 0..4 {
        transport.record(&actions);
        let mut writes = 0;
        for action in &actions {
            assert!(
                !matches!(action, Action::Notice(Notice::CommandFailed { .. })),
                "{actions:?}"
            );
            if let Action::Http(request) = action {
                assert!(request.url.ends_with("/exchange"));
                writes += 1;
                let body: Value =
                    serde_json::from_str(request.body.as_deref().expect("body")).expect("payload");
                let statuses: Vec<_> = body["action"]["orders"]
                    .as_array()
                    .expect("orders")
                    .iter()
                    .enumerate()
                    .map(|(index, _)| json!({"resting": {"oid": payloads.len() * 10 + index + 1}}))
                    .collect();
                payloads.push(body);
                reply(&mut engine, request, 200, &json!({"status":"ok", "response":{"type":"order", "data":{"statuses":statuses}}}).to_string());
            }
        }
        assert!(writes <= 1, "placements stay serial");
        let next = prepared_now(&mut engine);
        actions = advance_packet_timers(&mut engine, next, 4000.);
        if actions.iter().any(|action| {
            matches!(
                action,
                Action::Notice(Notice::CommandDone { command: 77, .. })
            )
        }) {
            assert_eq!(transport.requests.len(), payloads.len());
            return payloads;
        }
    }
    panic!("valid command did not finish: {actions:?}");
}

#[test]
fn all_valid_rows_place_once_with_brackets_and_caller_id() {
    // TS hyperliquid.exchange.ts:3639-3758.
    // KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: hyperliquid | limit-attached-TP-SL/audit trading.rs:733
    let mut first = order(3.);
    first.client_order_id = Some("0x00000000000000000000000000000001".into());
    first.stop_loss = Some(80.);
    first.take_profit = Some(120.);
    let mut second = order(3.);
    second.reduce_only = Some(true);
    let payloads = submit_valid(vec![first, second]);
    assert_eq!(payloads.len(), 2);
    let first = &payloads[0]["action"];
    assert_eq!(first["grouping"], "normalTpsl");
    assert_eq!(first["orders"].as_array().expect("orders").len(), 3);
    assert_eq!(
        first["orders"][0]["c"],
        "0x00000000000000000000000000000001"
    );
    for wire in first["orders"].as_array().expect("orders") {
        assert_eq!(wire["s"], "3");
    }
    assert_eq!(payloads[1]["action"]["orders"][0]["r"], true);
    assert_eq!(payloads[1]["action"]["orders"][0]["s"], "3");
}

#[test]
fn positional_zero_trigger_control() {
    let mut stop = order(0.);
    stop.kind = safe_cex_engine::model::OrderType::StopLoss;
    stop.side = safe_cex_engine::model::OrderSide::Sell;
    stop.price = Some(80.);
    stop.reduce_only = Some(true);
    let payloads = submit_valid(vec![stop]);
    assert_eq!(payloads.len(), 1);
    assert_eq!(payloads[0]["action"]["grouping"], "positionTpsl");
    assert_eq!(payloads[0]["action"]["orders"][0]["s"], "0");
}

#[test]
fn final_wire_bounds_allow_rounding_to_exact_minimum_and_maximum() {
    // One batched action since feat/hyperliquid-batched-placements; rows keep caller order.
    let payloads = submit_valid(vec![order(1.6), order(4.9)]);
    assert_eq!(payloads.len(), 1);
    assert_eq!(payloads[0]["action"]["orders"][0]["s"], "2");
    assert_eq!(payloads[0]["action"]["orders"][1]["s"], "5");
}

fn runtime_failure(status: u16, second_reply: &str, expected: &str) {
    runtime_failure_orders(
        vec![separator(), order(3.), separator()],
        status,
        second_reply,
        expected,
    );
}

fn runtime_failure_orders(
    orders: Vec<PlaceOrderOpts>,
    status: u16,
    second_reply: &str,
    expected: &str,
) {
    runtime_failure_suffix(
        orders,
        status,
        second_reply,
        expected,
        false,
        expected == "unknown",
    );
}

fn runtime_failure_suffix(
    orders: Vec<PlaceOrderOpts>,
    status: u16,
    second_reply: &str,
    expected: &str,
    fenced_suffix: bool,
    recorded_exception: bool,
) {
    let rows = |order: &PlaceOrderOpts| {
        1 + [order.stop_loss, order.take_profit]
            .into_iter()
            .flatten()
            .filter(|price| *price > 0.)
            .count()
    };
    let second_rows = rows(&orders[1]);
    let tail_rows = rows(&orders[2]);
    let mut engine = configured();
    engine.handle(
        2000.,
        Event::Command(Command::PlaceOrders {
            command: 77,
            orders,
        }),
    );
    let mut actions = prepared_now(&mut engine);
    let mut writes = Vec::new();
    let mut failure = None;
    let mut events = Vec::new();
    for _ in 0..8 {
        for action in &actions {
            match action {
                Action::Http(request) if request.url.ends_with("/exchange") => {
                    writes.push(request.clone());
                    assert!(
                        writes.len() <= 2,
                        "no suffix, retry or rollback: {writes:?}"
                    );
                    if writes.len() == 1 {
                        reply(
                            &mut engine,
                            request,
                            200,
                            r#"{"status":"ok","response":{"type":"order","data":{"statuses":[{"resting":{"oid":91}}]}}}"#,
                        );
                    } else if status == 0 {
                        engine.handle(
                            2500.,
                            Event::HttpFailed {
                                request: request.id,
                                error: "transport reply lost",
                            },
                        );
                    } else {
                        reply(&mut engine, request, status, second_reply);
                    }
                }
                Action::Notice(Notice::CommandFailed { command: 77, error }) => {
                    assert!(failure.is_none(), "one terminal failure");
                    failure = Some(serde_json::to_value(error).expect("error JSON"));
                }
                Action::Notice(Notice::CommandDone { command: 77, .. }) => {
                    panic!("failed intent reported whole")
                }
                Action::Notice(Notice::Event(event)) => events.push(event.clone()),
                _ => {}
            }
        }
        let next = prepared_now(&mut engine);
        actions = advance_packet_timers(&mut engine, next, 20_000.);
    }
    assert_eq!(writes.len(), 2);
    let error = failure.expect("terminal failure");
    if recorded_exception {
        support::assert_unknown_publication(&error, &events, 77);
    }
    if expected == "notSent" {
        assert!(
            error.get("outcome").is_none(),
            "known partial execution: {error}"
        );
    } else {
        assert_eq!(error["outcome"], "unknown", "{error}");
        assert!(
            error["message"]
                .as_str()
                .expect("terminal message")
                .contains(support::ENGINE_UNKNOWN_TEXT),
            "{error}"
        );
    }
    assert_eq!(error["partial"]["orderIds"], json!(["91"]));
    let elements = error["elements"]
        .as_array()
        .expect("complete per-element outcomes");
    assert_eq!(elements.len(), second_rows + 1 + tail_rows);
    assert_eq!(elements[0]["index"], 0);
    assert_eq!(elements[0]["status"], "placed");
    assert_eq!(elements[0]["orderIds"], json!(["91"]));
    for element in &elements[1..=second_rows] {
        assert_eq!(element["status"], expected);
        if expected == "unknown" {
            assert!(
                element["error"]["message"]
                    .as_str()
                    .expect("element message")
                    .contains(if status >= 500 { "The exchange reply didn’t confirm this change. Check your open orders before trying again." } else { support::ENGINE_UNKNOWN_TEXT }),
                "{element}"
            );
        }
        assert_eq!(
            element["error"]["outcome"],
            if expected == "notSent" {
                "rejected"
            } else {
                "unknown"
            }
        );
    }
    for tail in &elements[second_rows + 1..] {
        assert_eq!(tail["status"], "notSent");
        if fenced_suffix {
            assert_eq!(tail["error"]["outcome"], "notSent");
            assert_eq!(tail["error"]["message"], "This request isn’t ready to send");
        } else {
            assert!(tail.get("error").is_none(), "{error}");
        }
    }
}

#[test]
fn runtime_rejection_retains_prefix_and_unsent_suffix() {
    // TS hyperliquid.order-response.ts:104-124; hyperliquid.exchange.ts:4441.
    // KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: hyperliquid | runtime-serial-failure
    runtime_failure(
        200,
        r#"{"status":"ok","response":{"type":"order","data":{"statuses":[{"error":"Insufficient margin to place order."}]}}}"#,
        "notSent",
    );
}

#[test]
fn runtime_unlisted_refusal_is_unknown() {
    runtime_failure(
        200,
        r#"{"status":"ok","response":{"type":"order","data":{"statuses":[{"error":"New undocumented failure"}]}}}"#,
        "unknown",
    );
}

#[test]
fn runtime_malformed_reply_is_unknown() {
    runtime_failure(200, "malformed reply", "unknown");
}

#[test]
fn runtime_lost_reply_is_unknown() {
    runtime_failure(0, "", "unknown");
}

#[test]
fn runtime_server_failure_retains_complete_journal() {
    // Engine-sealed HTTP503 retains halt/journal/canonical-message pins.
    // Telemetry for this path awaits the engine hook approved by the lead.
    runtime_failure_suffix(
        vec![separator(), order(3.), separator()],
        503,
        "service unavailable",
        "unknown",
        false,
        false,
    );
}

#[test]
fn later_market_write_uses_current_price_without_frozen_plan_veto() {
    // TS hyperliquid.exchange.ts:3891-3898,4441.
    // KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: hyperliquid | live-price-freezing
    let mut engine = configured();
    let mut second = order(3.);
    second.kind = safe_cex_engine::model::OrderType::Market;
    second.price = None;
    engine.handle(
        2000.,
        Event::Command(Command::PlaceOrders {
            command: 77,
            orders: vec![separator(), second],
        }),
    );
    let mut actions = prepared_now(&mut engine);
    let mut writes = 0;
    let mut done = false;
    for _ in 0..8 {
        for action in &actions {
            assert!(
                !matches!(action, Action::Notice(Notice::CommandFailed { .. })),
                "{actions:?}"
            );
            match action {
                Action::Http(request) if request.url.ends_with("/exchange") => {
                    writes += 1;
                    assert!(writes <= 2);
                    if writes == 1 {
                        engine
                            .store_mut()
                            .update_ticker("BTC-PERP", |ticker| ticker.mark = 200.);
                        engine
                            .store_mut()
                            .update_markets(|market| market.limits.amount.max = 2.);
                    } else {
                        let body: Value =
                            serde_json::from_str(request.body.as_deref().expect("body"))
                                .expect("wire");
                        assert_eq!(body["action"]["orders"][0]["p"], "210");
                    }
                    reply(&mut engine, request, 200, &json!({"status":"ok","response":{"type":"order","data":{"statuses":[{"resting":{"oid":90+writes}}]}}}).to_string());
                }
                Action::Notice(Notice::CommandDone { command: 77, .. }) => done = true,
                _ => {}
            }
        }
        let next = prepared_now(&mut engine);
        actions = advance_packet_timers(&mut engine, next, 4000.);
    }
    assert!(done);
    assert_eq!(writes, 2);
}

fn accepted_idless_prefix(first: PlaceOrderOpts, statuses: Value, expected: &[&str]) {
    let mut engine = configured();
    engine.handle(
        2000.,
        Event::Command(Command::PlaceOrders {
            command: 77,
            orders: vec![first, order(3.), separator()],
        }),
    );
    let mut actions = prepared_now(&mut engine);
    let mut writes = Vec::new();
    let mut failure = None;
    for _ in 0..8 {
        for action in &actions {
            match action {
                Action::Http(request) if request.url.ends_with("/exchange") => {
                    writes.push(request.clone());
                    assert!(writes.len() <= 2, "no retry, suffix or rollback");
                    let rows = if writes.len() == 1 {
                        statuses.clone()
                    } else {
                        json!([{"error":"Insufficient margin to place order."}])
                    };
                    reply(&mut engine, request, 200, &json!({"status":"ok","response":{"type":"order","data":{"statuses":rows}}}).to_string());
                }
                Action::Notice(Notice::CommandFailed { command: 77, error }) => {
                    failure = Some(serde_json::to_value(error).expect("error JSON"))
                }
                Action::Notice(Notice::CommandDone { command: 77, .. }) => {
                    panic!("partial intent reported whole")
                }
                _ => {}
            }
        }
        let next = prepared_now(&mut engine);
        actions = advance_packet_timers(&mut engine, next, 20_000.);
    }
    assert_eq!(writes.len(), 2);
    let error = failure.expect("terminal failure");
    if expected.first() == Some(&"placed") {
        assert_eq!(error["partial"]["orderIds"], json!(["91"]));
    } else {
        assert!(
            error.get("partial").is_none(),
            "client identity is not a native oid: {error}"
        );
    }
    let elements = error["elements"]
        .as_array()
        .expect("every dispatched row journaled");
    assert_eq!(elements.len(), expected.len());
    for (index, (element, status)) in elements.iter().zip(expected).enumerate() {
        assert_eq!(element["index"], index);
        assert_eq!(element["status"], *status);
        if *status == "applied" {
            assert!(
                element.get("orderIds").is_none(),
                "no invented native order identity"
            );
        }
    }
    assert_eq!(elements[expected.len() - 2]["error"]["outcome"], "rejected");
    assert!(elements[expected.len() - 1].get("error").is_none());
}

#[test]
fn deferred_children_have_applied_outcomes_before_later_rejection() {
    let mut first = order(3.);
    first.stop_loss = Some(80.);
    first.take_profit = Some(120.);
    accepted_idless_prefix(
        first,
        json!([{"resting":{"oid":91}}, "waitingForFill", "waitingForFill"]),
        &["placed", "applied", "applied", "notSent", "notSent"],
    );
}

#[test]
fn full_position_trigger_has_applied_outcome_before_later_rejection() {
    let mut first = order(0.);
    first.kind = safe_cex_engine::model::OrderType::StopLoss;
    first.side = safe_cex_engine::model::OrderSide::Sell;
    first.price = Some(80.);
    first.reduce_only = Some(true);
    accepted_idless_prefix(
        first,
        json!(["waitingForTrigger"]),
        &["applied", "notSent", "notSent"],
    );
}

#[test]
fn mixed_native_group_retains_placed_applied_refused_and_unsent_rows() {
    // TS hyperliquid.exchange.ts:3727-3758; hyperliquid.order-response.ts:104-124.
    // KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: hyperliquid | limit-attached-TP-SL/audit trading.rs:733
    let mut engine = configured();
    let mut first = order(3.);
    first.stop_loss = Some(80.);
    first.take_profit = Some(120.);
    engine.handle(
        2000.,
        Event::Command(Command::PlaceOrders {
            command: 77,
            orders: vec![first, order(3.)],
        }),
    );
    let mut actions = prepared_now(&mut engine);
    let mut writes = 0;
    let mut failure = None;
    for _ in 0..8 {
        for action in &actions {
            match action {
                Action::Http(request) if request.url.ends_with("/exchange") => {
                    writes += 1;
                    assert_eq!(writes, 1, "no suffix, retry or rollback");
                    reply(
                        &mut engine,
                        request,
                        200,
                        r#"{"status":"ok","response":{"type":"order","data":{"statuses":[{"resting":{"oid":91}},"waitingForFill",{"error":"Insufficient margin to place order."}]}}}"#,
                    );
                }
                Action::Notice(Notice::CommandFailed { command: 77, error }) => {
                    failure = Some(serde_json::to_value(error).expect("error JSON"))
                }
                Action::Notice(Notice::CommandDone { command: 77, .. }) => {
                    panic!("failed group reported whole")
                }
                _ => {}
            }
        }
        let next = prepared_now(&mut engine);
        actions = advance_packet_timers(&mut engine, next, 20_000.);
    }
    assert_eq!(writes, 1);
    let error = failure.expect("terminal failure");
    assert_eq!(error["partial"]["orderIds"], json!(["91"]));
    let elements = error["elements"]
        .as_array()
        .expect("complete group outcomes");
    assert_eq!(elements.len(), 4);
    assert_eq!(elements[0]["status"], "placed");
    assert_eq!(elements[1]["status"], "applied");
    assert_eq!(elements[1]["effect"], "deferredChild");
    assert_eq!(elements[1]["targetOrderId"], "91");
    assert_eq!(elements[2]["status"], "notSent");
    assert_eq!(elements[2]["error"]["outcome"], "rejected");
    assert_eq!(elements[3]["status"], "notSent");
    assert!(elements[3].get("error").is_none());
}

#[test]
fn settings_and_schedule_controls_have_journaled_runtime_refusals() {
    let commands = [
        (
            Command::SetLeverage {
                command: 77,
                symbol: "BTC-PERP".into(),
                leverage: 5.,
                side: None,
            },
            "Cannot switch leverage type with open position.",
        ),
        (
            safe_cex_engine::command::decode_command(r#"{"op":"scheduleCancel","command":77}"#)
                .expect("schedule command"),
            "Cannot set scheduled cancel time until enough volume traded",
        ),
    ];
    for (command, refusal) in commands {
        let mut engine = configured();
        engine.handle(2000., Event::Command(command));
        let actions = prepared_now(&mut engine);
        let requests: Vec<_> = actions
            .iter()
            .filter_map(|action| match action {
                Action::Http(request) => Some(request),
                _ => None,
            })
            .collect();
        assert_eq!(requests.len(), 1);
        assert!(requests[0].url.ends_with("/exchange"));
        reply(
            &mut engine,
            requests[0],
            200,
            &json!({"status":"err","response":refusal}).to_string(),
        );
        let actions = prepared_now(&mut engine);
        let errors: Vec<_> = actions
            .iter()
            .filter_map(|action| match action {
                Action::Notice(Notice::CommandFailed { command: 77, error }) => Some(error),
                _ => None,
            })
            .collect();
        assert_eq!(errors.len(), 1, "{actions:?}");
        assert_eq!(errors[0].outcome, Some(Outcome::Rejected));
        let elements = errors[0].elements.as_ref().expect("control journal");
        assert_eq!(elements.len(), 1);
        assert!(
            matches!(&elements[0].result, safe_cex_engine::write_plan::ElementResult::NotSent { error: Some(error) } if error.outcome == Some(Outcome::Rejected))
        );
        assert!(!actions.iter().any(
            |action| matches!(action, Action::Http(request) if request.url.ends_with("/exchange"))
        ));
    }
}

#[test]
fn caller_id_size_gate_matches_ts_rounded_maximum() {
    // TS exchanges/base.ts:1211-1223 rounds before the positive-cap unsplit check.
    let mut accepted = order(5.1);
    accepted.client_order_id = Some("0x00000000000000000000000000000001".into());
    // The second row of the one batched action (feat/hyperliquid-batched-placements).
    let payloads = submit_valid(vec![order(3.), accepted]);
    assert_eq!(payloads[0]["action"]["orders"][1]["s"], "5");

    let mut engine = configured();
    engine
        .store_mut()
        .update_markets(|market| market.limits.amount.max = 4.9);
    let mut refused = order(4.9);
    refused.client_order_id = Some("0x00000000000000000000000000000001".into());
    engine.handle(
        2000.,
        Event::Command(Command::PlaceOrders {
            command: 77,
            orders: vec![order(3.), refused],
        }),
    );
    let actions = prepared_now(&mut engine);
    assert!(
        !actions
            .iter()
            .any(|action| matches!(action, Action::Http(_)))
    );
    assert!(actions.iter().any(|action| matches!(action,
        Action::Notice(Notice::CommandFailed { command: 77, error })
        if error.outcome == Some(Outcome::NotSent) && error.message.contains("cannot be split"))));
}

#[test]
fn market_row_refusal_pins_stop_before_next_live_price_preparation() {
    // TS hyperliquid.exchange.ts:3891-3929 prepares the due row; :4441 stops on its failure.
    // KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: hyperliquid | live-price-freezing
    let mut market = order(3.);
    market.kind = safe_cex_engine::model::OrderType::Market;
    market.price = None;
    runtime_failure_orders(
        vec![separator(), market, separator()],
        200,
        r#"{"status":"ok","response":{"type":"order","data":{"statuses":[{"error":"Insufficient margin to place order."}]}}}"#,
        "notSent",
    );
}

// AGT-117: a market entry's legs are rows of its own grouped write, so a refused market
// bracket journals three refused rows, and an unsent one three unsent rows.
#[test]
fn market_bracket_rows_are_journaled_when_refused_or_unsent() {
    let mut bracket = order(3.);
    bracket.kind = safe_cex_engine::model::OrderType::Market;
    bracket.price = None;
    bracket.stop_loss = Some(80.);
    bracket.take_profit = Some(120.);
    runtime_failure_orders(
        vec![order(3.), bracket.clone(), bracket],
        200,
        r#"{"status":"ok","response":{"type":"order","data":{"statuses":[{"error":"Insufficient margin to place order."}]}}}"#,
        "notSent",
    );
}

#[test]
fn serial_tail_pins_native_rounding_and_below_minimum_group_wire() {
    // TS hyperliquid.exchange.ts:3639-3719 rounds the due row and builds all its children.
    // KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: hyperliquid | serial-placement-batch/audit trading.rs:367
    for (amount, expected_size) in [(0.4, "0"), (1., "1"), (6., "6")] {
        let mut tail = order(amount);
        tail.price = Some(3.);
        tail.stop_loss = Some(2.);
        tail.take_profit = Some(4.);
        let payloads = submit_valid(vec![order(3.), tail]);
        assert_eq!(payloads.len(), 2);
        assert_eq!(payloads[0]["action"]["orders"][0]["s"], "3");
        let action = &payloads[1]["action"];
        assert_eq!(action["type"], "order");
        assert_eq!(action["grouping"], "normalTpsl");
        let rows = action["orders"].as_array().expect("group rows");
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0]["p"], "3");
        assert_eq!(rows[1]["t"]["trigger"]["triggerPx"], "2");
        assert_eq!(rows[2]["t"]["trigger"]["triggerPx"], "4");
        for row in rows {
            assert_eq!(row["s"], expected_size);
        }
    }
}

#[test]
fn context_free_deferred_plain_limit_reply_keeps_unknown_fence() {
    // TS order-response.ts:137-145; hyperliquid.exchange.ts:3758-3760,4439-4442 accepts no-ID deferred rows.
    // PARITY-EXCEPTION: stop-after-UNKNOWN (Sean/daxx, 2026-11-08); TS continues at hyperliquid.order-response.ts:137-145; hyperliquid.exchange.ts:4439-4442.
    // KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: hyperliquid | serial-placement-batch/audit trading.rs:367
    runtime_failure_suffix(
        vec![separator(), order(3.), separator()],
        200,
        r#"{"status":"ok","response":{"type":"order","data":{"statuses":["waitingForFill"]}}}"#,
        "unknown",
        true,
        true,
    );
}

#[test]
fn context_free_deferred_market_reply_keeps_fence_before_next_market() {
    // TS order-response.ts:137-145; hyperliquid.exchange.ts:3927-3929,4441 accepts no-ID deferred rows.
    // PARITY-EXCEPTION: stop-after-UNKNOWN (Sean/daxx, 2026-11-08); TS continues at hyperliquid.order-response.ts:137-145; hyperliquid.exchange.ts:3927-3929,4441.
    // KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: hyperliquid | live-price-freezing
    let mut market = order(3.);
    market.kind = safe_cex_engine::model::OrderType::Market;
    market.price = None;
    runtime_failure_suffix(
        vec![separator(), market, separator()],
        200,
        r#"{"status":"ok","response":{"type":"order","data":{"statuses":["waitingForFill"]}}}"#,
        "unknown",
        true,
        true,
    );
}

#[test]
fn deferred_group_without_proven_parent_keeps_unknown_fence() {
    // TS order-response.ts:137-145; hyperliquid.exchange.ts:3758-3760,4441 accepts all-deferred groups.
    // PARITY-EXCEPTION: stop-after-UNKNOWN (Sean/daxx, 2026-11-08); TS continues at hyperliquid.order-response.ts:137-145; hyperliquid.exchange.ts:3758-3760,4439-4442.
    // KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: hyperliquid | limit-attached-TP-SL/audit trading.rs:733
    let mut group = order(3.);
    group.stop_loss = Some(80.);
    group.take_profit = Some(120.);
    runtime_failure_suffix(
        vec![order(3.), group, order(3.)],
        200,
        r#"{"status":"ok","response":{"type":"order","data":{"statuses":["waitingForFill","waitingForFill","waitingForFill"]}}}"#,
        "unknown",
        false,
        true,
    );
}

// AGT-115: `wires` groups legs only with a limit or market entry, so a stop or take-profit entry
// was placed without them. It now refuses the whole batch before any write.
#[test]
fn a_stop_entry_with_attached_legs_refuses_entire_batch() {
    let refusal = safe_cex_engine::model::stop_entry_legs_refusal("Hyperliquid");
    for kind in ["stop_market", "take_profit_market"] {
        for legs in [
            json!({"stopLoss": 80}),
            json!({"takeProfit": 120}),
            json!({"stopLoss": 0, "takeProfit": 120}),
        ] {
            let mut stop = json!({"symbol": "BTC-PERP", "type": kind, "side": "buy", "amount": 3, "price": 110});
            for (k, v) in legs.as_object().unwrap() {
                stop[k] = v.clone();
            }
            let mut engine = configured();
            engine.handle(
                2000.,
                Event::Command(Command::PlaceOrders {
                    command: 77,
                    orders: vec![order(3.), serde_json::from_value(stop).unwrap()],
                }),
            );
            let actions = prepared_now(&mut engine);
            assert!(
                !actions
                    .iter()
                    .any(|action| matches!(action, Action::Http(_))),
                "{kind} {legs}"
            );
            assert!(
                actions.iter().any(|action| matches!(action,
                    Action::Notice(Notice::CommandFailed { command: 77, error })
                    if error.outcome == Some(Outcome::NotSent) && error.legacy_text() == refusal)),
                "{kind} {legs}: {actions:?}"
            );
        }
    }
}
