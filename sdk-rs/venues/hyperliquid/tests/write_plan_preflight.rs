mod support;

use safe_cex_engine::model::PlaceOrderOpts;
use safe_cex_engine::{Action, Command, Engine, ErrorKind, Event, HttpRequest, Notice, Outcome};
use safe_cex_venue_hyperliquid::Hyperliquid;
use serde_json::{Value, json};
use support::{Plain, advance_packet_timers, message, prepared_now, ready, reply};

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

fn configured() -> Engine<Hyperliquid> {
    let (mut engine, _) = ready("disabled");
    engine.store_mut().update_markets(|market| {
        market.precision.amount = 1.;
        market.limits.amount.min = 2.;
        market.limits.amount.max = 5.;
    });
    engine
}

fn refuse(engine: &mut Engine<Hyperliquid>, command: Command) {
    engine.handle(2000., Event::Command(command));
    let actions = prepared_now(engine);
    let mut transport = ScriptedTransport::default();
    transport.record(&actions);
    assert_eq!(
        transport.requests.len(),
        0,
        "preknown invalid plan must send zero transport requests: {actions:?}"
    );
    let errors: Vec<_> = actions
        .iter()
        .filter_map(|action| match action {
            Action::Notice(Notice::CommandFailed { command: 77, error }) => Some(error),
            _ => None,
        })
        .collect();
    assert_eq!(errors.len(), 1, "{actions:?}");
    assert_eq!(errors[0].outcome, Some(Outcome::NotSent));
    assert_eq!(errors[0].kind, Some(ErrorKind::InvalidInput));
    assert!(errors[0].partial.is_none());
}

#[test]
fn valid_first_below_min_second_zero_writes() {
    refuse(
        &mut configured(),
        Command::PlaceOrders {
            command: 77,
            orders: vec![order(3.), order(1.)],
        },
    );
}

#[test]
fn valid_first_rounds_zero_second_zero_writes() {
    refuse(
        &mut configured(),
        Command::PlaceOrders {
            command: 77,
            orders: vec![order(3.), order(0.4)],
        },
    );
}

#[test]
fn valid_first_above_max_second_zero_writes() {
    refuse(
        &mut configured(),
        Command::PlaceOrders {
            command: 77,
            orders: vec![order(3.), order(6.)],
        },
    );
}

#[test]
fn rounded_last_quantity_above_max_zero_writes() {
    let mut engine = configured();
    engine
        .store_mut()
        .update_markets(|market| market.limits.amount.max = 4.9);
    refuse(
        &mut engine,
        Command::PlaceOrders {
            command: 77,
            orders: vec![order(3.), order(4.9)],
        },
    );
}

#[test]
fn invalid_last_bracket_wire_zero_writes() {
    let mut last = order(3.);
    last.stop_loss = Some(0.000000001);
    refuse(
        &mut configured(),
        Command::PlaceOrders {
            command: 77,
            orders: vec![order(3.), last],
        },
    );
}

#[test]
fn last_row_below_native_notional_zero_writes() {
    let mut last = order(3.);
    last.price = Some(3.);
    refuse(
        &mut configured(),
        Command::PlaceOrders {
            command: 77,
            orders: vec![order(3.), last],
        },
    );
}

#[test]
fn positive_last_price_rounds_zero_zero_writes() {
    let mut last = order(3.);
    last.price = Some(0.000000001);
    refuse(
        &mut configured(),
        Command::PlaceOrders {
            command: 77,
            orders: vec![order(3.), last],
        },
    );
}

#[test]
fn invalid_replacement_never_cancels() {
    let (mut engine, socket) = ready("disabled");
    message(
        &mut engine,
        socket,
        1900.,
        r#"{"channel":"orderUpdates","data":[{"order":{"coin":"BTC","oid":9,"side":"B","limitPx":"100","origSz":"3","sz":"3","reduceOnly":false,"orderType":"Limit"},"status":"open"}]}"#,
    );
    engine.drain_plain().for_each(drop);
    engine
        .store_mut()
        .update_markets(|market| market.limits.amount.min = 2.);
    refuse(
        &mut engine,
        Command::AmendOrder {
            command: 77,
            id: "9".into(),
            price: Some(110.),
            amount: Some(1.),
        },
    );
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
    let payloads = submit_valid(vec![order(1.6), order(4.9)]);
    assert_eq!(payloads.len(), 2);
    assert_eq!(payloads[0]["action"]["orders"][0]["s"], "2");
    assert_eq!(payloads[1]["action"]["orders"][0]["s"], "5");
}
