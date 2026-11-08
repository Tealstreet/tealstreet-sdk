mod support;

use safe_cex_engine::{Action, Command, Event, HttpRequest, Method, Notice};
use serde_json::{Value, json};
use std::collections::{BTreeSet, VecDeque};
use support::managed::*;

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

#[derive(Default)]
struct ScriptedTransport {
    requests: Vec<HttpRequest>,
    responses: VecDeque<(u16, String)>,
}

impl ScriptedTransport {
    fn poll(&mut self, engine: &mut Driver) -> Vec<Action> {
        let out = actions(engine);
        for action in &out {
            if let Action::Http(request) = action {
                self.requests.push(request.clone());
                if let Some((status, body)) = self.responses.pop_front() {
                    reply(engine, request, status, &body);
                }
            }
        }
        out
    }
}

fn constrained(min: f64, max: f64) -> Driver {
    let (mut engine, _) = ready();
    engine.store_mut().update_markets(|market| {
        market.precision.amount = 0.1;
        market.limits.amount.min = min;
        market.limits.amount.max = max;
    });
    engine
}

fn limit(amount: f64) -> Value {
    json!({"symbol":SYMBOL,"type":"limit","side":"buy","amount":amount,"price":140})
}

fn submit(engine: &mut Driver, orders: Vec<Value>) -> Result {
    engine.handle(
        2000.,
        Event::Command(Command::PlaceOrders {
            command: 901,
            orders: serde_json::from_value(Value::Array(orders))?,
        }),
    );
    Ok(())
}

fn assert_placement_sequence(engine: &mut Driver, quantities: &[Vec<&str>]) -> Result {
    let mut expected_ids = vec![];
    for (step, expected) in quantities.iter().enumerate() {
        let packets = requests(engine);
        assert_eq!(packets.len(), 1, "step {step}: exactly one legacy write");
        let packet = packets.first().ok_or("missing placement")?;
        assert_eq!(packet.method, Method::Post);
        let wire: Value = serde_json::from_str(packet.body.as_deref().ok_or("missing body")?)?;
        let rows = match &wire {
            Value::Array(rows) => rows.clone(),
            row => vec![row.clone()],
        };
        assert_eq!(rows.len(), expected.len());
        assert!(
            packet
                .url
                .ends_with(if wire.is_array() { "/orders" } else { "/order" })
        );
        let mut acknowledgements = vec![];
        for (index, (row, quantity)) in rows.iter().zip(expected).enumerate() {
            let sent = row.get("quantity").or_else(|| row.get("triggerQuantity"));
            assert_eq!(sent, Some(&json!(quantity)), "step {step} row {index}");
            let id = format!("placed-{step}-{index}");
            expected_ids.push(id.clone());
            acknowledgements.push(json!({
                "operation":"Ok", "id":id, "clientId":row["clientId"],
                "symbol":SYMBOL, "side":row["side"], "orderType":row["orderType"]
            }));
        }
        let response = if wire.is_array() {
            Value::Array(acknowledgements)
        } else {
            acknowledgements
                .pop()
                .ok_or("missing single acknowledgement")?
        };
        reply(engine, packet, 200, &response.to_string());
    }
    let out = actions(engine);
    assert!(!out.iter().any(|action| matches!(action, Action::Http(_))));
    let result = out
        .iter()
        .find_map(|action| match action {
            Action::Notice(Notice::CommandDone {
                command: 901,
                result,
            }) => Some(result),
            _ => None,
        })
        .ok_or("missing placement completion")?;
    assert_eq!(serde_json::from_str::<Vec<String>>(result)?, expected_ids);
    Ok(())
}

#[test]
fn rounded_pair_quantities_follow_legacy_ts_without_wire_bound_recheck() -> Result {
    // TS reference/backpack/backpack.exchange.ts:746-779,872-878,1203-1232 rounds after raw bounds.
    // KNOWN-ISSUE: a rounded quantity may be rejected after earlier pairs were placed.
    for (amounts, quantities) in [
        (vec![0.4, 0.95], vec![vec!["0.4", "1.0"]]),
        (vec![0.4, 0.5, 0.95], vec![vec!["0.4", "0.5"], vec!["1.0"]]),
        (
            vec![0.4, 0.5, 0.6, 0.95],
            vec![vec!["0.4", "0.5"], vec!["0.6", "1.0"]],
        ),
    ] {
        let mut engine = constrained(0.1, 0.95);
        submit(&mut engine, amounts.into_iter().map(limit).collect())?;
        assert_placement_sequence(&mut engine, &quantities)?;
    }
    Ok(())
}

#[test]
fn rounded_bracket_quantity_follows_legacy_ts() -> Result {
    // TS reference/backpack/backpack.exchange.ts:918-925,980-1047 sends rounded entry/TP/SL.
    // KNOWN-ISSUE: native bounds can refuse a rounded entry or protective child after earlier writes.
    for kind in ["limit", "market"] {
        for prefix in [false, true] {
            let mut orders = vec![];
            let mut quantities = vec![];
            if prefix {
                orders.extend([limit(0.4), limit(0.5)]);
                quantities.push(vec!["0.4", "0.5"]);
            }
            orders.push(json!({"symbol":SYMBOL,"type":kind,"side":"buy","amount":0.95,"price":140,"takeProfit":150,"stopLoss":130}));
            quantities.push(vec!["1.0"]);
            if kind == "limit" {
                quantities.extend([vec!["1.0"], vec!["1.0"]]);
            }
            let mut engine = constrained(0.1, 0.95);
            submit(&mut engine, orders)?;
            assert_placement_sequence(&mut engine, &quantities)?;
        }
    }
    Ok(())
}

#[test]
fn rounded_last_row_below_min_follows_legacy_ts() -> Result {
    // TS reference/backpack/backpack.exchange.ts:746-779,918-925 formats without rounded bounds.
    // KNOWN-ISSUE: a rounded last row can be below the minimum after the valid pair was placed.
    let mut engine = constrained(0.14, 1.);
    submit(&mut engine, vec![limit(0.4), limit(0.5), limit(0.14)])?;
    assert_placement_sequence(&mut engine, &[vec!["0.4", "0.5"], vec!["0.1"]])
}

#[test]
fn rounded_replacement_follows_legacy_cancel_then_place() -> Result {
    // TS reference/backpack/backpack.exchange.ts:1096-1147 cancels then calls the rounded formatter.
    let (mut engine, _) = ready_orders(&format!("[{ORDER}]"));
    engine.store_mut().update_markets(|market| {
        market.precision.amount = 0.1;
        market.limits.amount.max = 0.95;
    });
    engine.handle(
        2000.,
        Event::Command(Command::AmendOrder {
            command: 901,
            id: "123".into(),
            price: Some(142.),
            amount: Some(0.95),
        }),
    );
    let packets = requests(&mut engine);
    assert_eq!(packets.len(), 1, "legacy cancellation must be released");
    let cancel = packets.first().ok_or("missing cancellation")?;
    assert_eq!(cancel.method, Method::Delete);
    reply(&mut engine, cancel, 200, ORDER);
    assert_placement_sequence(&mut engine, &[vec!["1.0"]])
}

#[test]
fn exact_native_max_pair_control() -> Result {
    let mut engine = constrained(0.1, 1.);
    submit(
        &mut engine,
        vec![
            limit(0.95),
            json!({"symbol":SYMBOL,"type":"limit","side":"sell","amount":1.,"price":150,"reduceOnly":true}),
            limit(1.),
            json!({"symbol":SYMBOL,"type":"limit","side":"sell","amount":0.95,"price":150}),
        ],
    )?;
    let mut clients = BTreeSet::new();
    let mut sends = 0;
    for pair in 0..2 {
        let packets = requests(&mut engine);
        assert_eq!(packets.len(), 1);
        let packet = packets.first().ok_or("missing pair")?;
        assert_eq!(packet.method, Method::Post);
        assert!(packet.url.ends_with("/orders"));
        let rows: Vec<Value> =
            serde_json::from_str(packet.body.as_deref().ok_or("missing pair body")?)?;
        assert_eq!(rows.len(), 2);
        let mut acknowledgements = vec![];
        for (row_index, row) in rows.iter().enumerate() {
            assert_eq!(row["quantity"], "1.0");
            assert_eq!(row["side"], if row_index == 0 { "Bid" } else { "Ask" });
            if pair == 0 && row_index == 1 {
                assert_eq!(row["reduceOnly"], true);
            }
            let client = row["clientId"]
                .as_u64()
                .ok_or("missing generated client ID")?;
            assert!(client > 0 && clients.insert(client));
            acknowledgements.push(json!({
                "operation":"Ok", "id":format!("pair-{pair}-{row_index}"),
                "clientId":client, "symbol":SYMBOL, "side":row["side"], "orderType":"Limit"
            }));
        }
        sends += 1;
        reply(
            &mut engine,
            packet,
            200,
            &serde_json::to_string(&acknowledgements)?,
        );
    }
    assert_eq!(sends, 2);
    assert_eq!(clients.len(), 4);
    let out = actions(&mut engine);
    assert!(!out.iter().any(|action| matches!(action, Action::Http(_))));
    let result = out
        .iter()
        .find_map(|action| match action {
            Action::Notice(Notice::CommandDone {
                command: 901,
                result,
            }) => Some(result),
            _ => None,
        })
        .ok_or("missing successful plan completion")?;
    assert_eq!(
        serde_json::from_str::<Vec<String>>(result)?,
        ["pair-0-0", "pair-0-1", "pair-1-0", "pair-1-1"]
    );
    Ok(())
}

#[test]
fn valid_bracket_places_each_leg_once() -> Result {
    let mut engine = constrained(0.1, 0.95);
    submit(
        &mut engine,
        vec![
            json!({"symbol":SYMBOL,"type":"limit","side":"buy","amount":0.85,"price":140,"takeProfit":150,"stopLoss":130,"timeInForce":"PostOnly"}),
        ],
    )?;
    let mut transport = ScriptedTransport {
        responses: ["entry", "tp", "sl"]
            .into_iter()
            .map(|id| (200, json!({"id":id}).to_string()))
            .collect(),
        ..Default::default()
    };
    for (index, id) in ["entry", "tp", "sl"].into_iter().enumerate() {
        let packets: Vec<_> = transport
            .poll(&mut engine)
            .into_iter()
            .filter_map(|action| match action {
                Action::Http(request) => Some(request),
                _ => None,
            })
            .collect();
        assert_eq!(packets.len(), 1);
        let packet = packets.first().ok_or("missing bracket write")?;
        assert_eq!(packet.method, Method::Post);
        let row: Value =
            serde_json::from_str(packet.body.as_deref().ok_or("missing bracket body")?)?;
        if index == 0 {
            assert_eq!(row["quantity"], "0.9");
            assert_eq!(row["postOnly"], true);
            assert_eq!(row["side"], "Bid");
        } else {
            assert_eq!(row["triggerQuantity"], "0.9");
            assert_eq!(
                row["triggerPrice"],
                if index == 1 { "150.00" } else { "130.00" }
            );
            assert_eq!(row["side"], "Ask");
            assert_eq!(row["reduceOnly"], true);
        }
        assert_eq!(transport.requests.len(), index + 1, "{id}");
    }
    let out = transport.poll(&mut engine);
    assert_eq!(transport.requests.len(), 3);
    assert!(transport.responses.is_empty());
    assert!(!out.iter().any(|action| matches!(action, Action::Http(_))));
    let result = out
        .iter()
        .find_map(|action| match action {
            Action::Notice(Notice::CommandDone {
                command: 901,
                result,
            }) => Some(result),
            _ => None,
        })
        .ok_or("missing successful bracket completion")?;
    assert_eq!(
        serde_json::from_str::<Vec<String>>(result)?,
        ["entry", "tp", "sl"]
    );
    Ok(())
}
