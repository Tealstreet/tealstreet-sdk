mod support;

use safe_cex_engine::write_plan::{AppliedEffect, ElementResult};
use safe_cex_engine::{Action, Command, Event, HttpRequest, Method, Notice, Outcome, VenueError};
use serde_json::{Value, json};
use std::collections::{BTreeSet, VecDeque};
use support::managed::*;

const ENGINE_UNKNOWN_TEXT: &str = "The outcome is unknown. Check Open Orders";

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

const PRICE_CAPPED_MARKETS: &str = r#"[{"symbol":"SOL_USDC_PERP","marketType":"PERP","filters":{"price":{"minPrice":"0.01","maxPrice":"1000","tickSize":"0.01"},"quantity":{"minQuantity":"0.01","maxQuantity":"1000","stepSize":"0.01"}}}]"#;

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

#[test]
fn native_max_price_uses_rounded_wire_value_with_inclusive_bound() -> Result {
    for price in [1000., 1000.004] {
        let (mut engine, _, _) = ready_catalogue(PRICE_CAPPED_MARKETS, "[]");
        let mut order = limit(1.);
        order["price"] = json!(price);
        submit(&mut engine, vec![order])?;
        let packet = requests(&mut engine)
            .pop()
            .ok_or("missing valid boundary write")?;
        let row: Value = serde_json::from_str(packet.body.as_deref().ok_or("missing body")?)?;
        assert_eq!(row["price"], "1000.00");
        reply(&mut engine, &packet, 200, r#"{"id":"boundary"}"#);
        assert!(actions(&mut engine).iter().any(|action| matches!(
            action,
            Action::Notice(Notice::CommandDone { command: 901, .. })
        )));
        assert!(requests(&mut engine).is_empty());
    }
    Ok(())
}

// TS: packages/safe-cex/src/__tests__/reference/backpack/backpack.exchange.ts:1121-1141 cancels, replaces, then rethrows.
// Protective put-back is retained under SPLIT_PLAN_INVARIANT.md exception 2.
// Unknown replacement outcomes still stop without a proof read or another write.
#[test]
fn protective_refusal_puts_back_only_after_a_clean_proof() -> Result {
    for (old_trigger, new_trigger) in [("130", 125.), ("150", 155.)] {
        for status in [400, 200, 503] {
            let stop = ORDER
        .replace("\"orderType\":\"Limit\"", "\"orderType\":\"Market\",\"triggerPrice\":\"130\",\"triggerQuantity\":\"5\",\"reduceOnly\":true")
        .replace("\"side\":\"Bid\"", "\"side\":\"Ask\"")
        .replace("\"triggerPrice\":\"130\"", &format!("\"triggerPrice\":\"{old_trigger}\""));
            let (mut engine, _, _) = ready_catalogue(PRICE_CAPPED_MARKETS, &format!("[{stop}]"));
            engine.store_mut().update_ticker(SYMBOL, |ticker| {
                ticker.last = 140.;
                ticker.mark = 140.;
                ticker.index = 140.;
            });
            engine.handle(
                2000.,
                Event::Command(Command::AmendOrder {
                    command: 901,
                    id: "123".into(),
                    price: Some(new_trigger),
                    amount: None,
                }),
            );
            let mut transport = ScriptedTransport {
                responses: [
                    (
                        200,
                        r#"{"id":"123","status":"Cancelled","executedQuantity":"0"}"#.into(),
                    ),
                    (status, r#"{"code":"INVALID_ORDER","message":"bad"}"#.into()),
                    (200, "[]".into()),
                    (200, r#"{"id":"restored-original"}"#.into()),
                ]
                .into(),
                ..Default::default()
            };
            transport.poll(&mut engine);
            let cancel = transport.requests.first().ok_or("missing cancel")?;
            assert_eq!(cancel.method, Method::Delete);
            assert!(cancel.url.ends_with("/api/v1/order"));
            assert_eq!(
                serde_json::from_str::<Value>(
                    cancel.body.as_deref().ok_or("missing cancel body")?
                )?,
                json!({"orderId":"123","symbol":SYMBOL})
            );
            transport.poll(&mut engine);
            let replacement = transport
                .requests
                .get(1)
                .ok_or("missing replacement")?
                .clone();
            assert_eq!(replacement.method, Method::Post);
            let row: Value =
                serde_json::from_str(replacement.body.as_deref().ok_or("missing body")?)?;
            assert!(replacement.url.ends_with("/api/v1/order"));
            assert_eq!(
                row,
                json!({
                    "symbol":SYMBOL, "side":"Ask", "orderType":"Market",
                    "triggerPrice":format!("{new_trigger:.2}"), "triggerQuantity":"5.00",
                    "triggerBy":"MarkPrice", "reduceOnly":true,
                })
            );
            let out = transport.poll(&mut engine);
            if status == 400 {
                transport.poll(&mut engine);
                let out = transport.poll(&mut engine);
                let error = failed(&out)?;
                assert_eq!(
                    error.protection,
                    Some(safe_cex_engine::error::Protection::Restored)
                );
                assert_eq!(transport.requests.len(), 4);
                assert_eq!(transport.requests[2].method, Method::Get);
                assert!(transport.requests[2].url.contains("/api/v1/orders?"));
                let restore = &transport.requests[3];
                assert_eq!(restore.method, Method::Post);
                let restored: Value =
                    serde_json::from_str(restore.body.as_deref().ok_or("missing put-back")?)?;
                assert_eq!(restored["triggerPrice"], format!("{}.00", old_trigger));
                assert_eq!(restored["triggerQuantity"], "5.00");
                let elements = error
                    .elements
                    .as_ref()
                    .ok_or("missing protective outcomes")?;
                assert_eq!(elements.len(), 3);
                assert!(matches!(
                    &elements[0].result,
                    ElementResult::Applied {
                        effect: AppliedEffect::Cancel,
                        ..
                    }
                ));
                assert!(
                    matches!(&elements[1].result, ElementResult::NotSent { error: Some(error) } if error.outcome == Some(Outcome::Rejected))
                );
                assert!(
                    matches!(&elements[2].result, ElementResult::Placed { order_ids } if order_ids == &["restored-original"])
                );
                assert!(transport.responses.is_empty());
                reply(&mut engine, &replacement, 200, "[]");
                let late = transport.poll(&mut engine);
                assert!(!late.iter().any(|action| matches!(action, Action::Http(_))));
                assert_eq!(transport.requests.len(), 4);
                continue;
            }
            let error = failed(&out)?;
            assert_eq!(error.outcome, Some(Outcome::Unknown));
            assert!(error.message.contains(ENGINE_UNKNOWN_TEXT));
            assert_eq!(error.protection, None);
            assert_eq!(
                out.iter()
                    .filter(|action| matches!(
                        action,
                        Action::Notice(Notice::CommandFailed { command: 901, .. })
                    ))
                    .count(),
                1
            );
            let elements = error
                .elements
                .as_ref()
                .ok_or("missing protective outcomes")?;
            assert_eq!(elements.len(), 2);
            assert!(matches!(&elements[0].result, ElementResult::Applied {
        effect: AppliedEffect::Cancel, target_order_id: Some(id), ..
    } if id == "123"));
            assert!(
                matches!(&elements[1].result, ElementResult::Unknown { error }
        if error.venue_code.as_deref() == Some("INVALID_ORDER"))
            );
            transport.poll(&mut engine);
            assert_eq!(transport.requests.len(), 2);
            assert_eq!(
                transport.responses,
                [
                    (200, "[]".into()),
                    (200, r#"{"id":"restored-original"}"#.into())
                ]
            );
            reply(&mut engine, &replacement, 200, "[]");
            let late = transport.poll(&mut engine);
            assert!(!late.iter().any(|action| matches!(
                action,
                Action::Notice(Notice::CommandDone { command: 901, .. })
                    | Action::Notice(Notice::CommandFailed { command: 901, .. })
            )));
            assert_eq!(transport.requests.len(), 2);
        }
    }
    Ok(())
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

// Legacy TS: packages/safe-cex/src/__tests__/reference/backpack/backpack.exchange.ts:980-1028.
// KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: backpack | sdk-rs/venues/backpack/src/trading.rs:451 separate-TP-SL-children
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

fn failed(out: &[Action]) -> Result<&VenueError> {
    out.iter()
        .find_map(|action| match action {
            Action::Notice(Notice::CommandFailed {
                command: 901,
                error,
            }) => Some(error),
            _ => None,
        })
        .ok_or_else(|| "missing terminal failure".into())
}

// Legacy TS: packages/safe-cex/src/__tests__/reference/backpack/backpack.exchange.ts:1096-1142.
// KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: backpack | AmendOrder pre-cancel wire validation and class-A restore
#[test]
fn cancel_replace_reports_applied_cancel_then_unknown_without_restore() -> Result {
    let (mut engine, _) = ready_orders(&format!("[{ORDER}]"));
    engine.handle(
        2000.,
        Event::Command(Command::AmendOrder {
            command: 901,
            id: "123".into(),
            price: Some(142.),
            amount: None,
        }),
    );
    let mut transport = ScriptedTransport {
        responses: [
            (
                200,
                r#"{"id":"123","status":"Cancelled","executedQuantity":"0"}"#.into(),
            ),
            (
                400,
                r#"{"code":"INVALID_ORDER","message":"scripted failure"}"#.into(),
            ),
        ]
        .into(),
        ..Default::default()
    };
    transport.poll(&mut engine);
    transport.poll(&mut engine);
    let out = transport.poll(&mut engine);
    let error = failed(&out)?;
    let elements = error.elements.as_ref().ok_or("missing amend outcomes")?;
    assert_eq!(elements.len(), 2);
    assert!(
        matches!(&elements[0].result, ElementResult::Applied { effect: AppliedEffect::Cancel, target_order_id: Some(id), .. } if id == "123")
    );
    assert!(matches!(elements[1].result, ElementResult::Unknown { .. }));
    assert!(
        error
            .partial
            .as_ref()
            .is_none_or(|partial| partial.order_ids.is_empty())
    );
    transport.poll(&mut engine);
    assert_eq!(transport.requests.len(), 2);
    Ok(())
}
