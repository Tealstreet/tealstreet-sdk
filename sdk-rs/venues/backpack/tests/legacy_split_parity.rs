mod support;

use safe_cex_engine::write_plan::{AppliedEffect, ElementResult};
use safe_cex_engine::{Action, Command, Event, HttpRequest, Method, Notice, VenueError};
use serde_json::{Value, json};
use support::managed::*;

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

fn place(engine: &mut Driver, orders: Vec<Value>) -> Result {
    engine.handle(
        2000.,
        Event::Command(Command::PlaceOrders {
            command: 901,
            orders: serde_json::from_value(Value::Array(orders))?,
        }),
    );
    Ok(())
}

fn limit(price: f64) -> Value {
    json!({"symbol":SYMBOL,"type":"limit","side":"buy","amount":0.4,"price":price})
}

fn packet(engine: &mut Driver, transport: &mut Vec<HttpRequest>) -> Result<HttpRequest> {
    let packets = requests(engine);
    assert_eq!(
        packets.len(),
        1,
        "legacy TS dispatches the next independent write"
    );
    transport.extend(packets.iter().cloned());
    packets
        .into_iter()
        .next()
        .ok_or_else(|| "missing write".into())
}

fn sequence(transport: &[HttpRequest], expected: &[(Method, &str)]) {
    assert_eq!(
        transport.len(),
        expected.len(),
        "exact legacy TS write count"
    );
    for (request, (method, path)) in transport.iter().zip(expected) {
        assert_eq!(&request.method, method);
        assert!(request.url.ends_with(path), "{}", request.url);
    }
}

fn pair_ack(request: &HttpRequest) -> Result<String> {
    let rows: Vec<Value> = serde_json::from_str(request.body.as_deref().ok_or("missing pair")?)?;
    Ok(serde_json::to_string(
        &rows
            .iter()
            .enumerate()
            .map(|(i, row)| {
                json!({
                    "operation":"Ok","id":format!("pair-{i}"),"clientId":row["clientId"],
                    "symbol":row["symbol"],"side":row["side"],"orderType":row["orderType"]
                })
            })
            .collect::<Vec<_>>(),
    )?)
}

fn failure(out: &[Action]) -> Result<&VenueError> {
    out.iter()
        .find_map(|action| match action {
            Action::Notice(Notice::CommandFailed {
                command: 901,
                error,
            }) => Some(error),
            _ => None,
        })
        .ok_or_else(|| "missing reporting-only terminal".into())
}

// Legacy TS: packages/safe-cex/src/__tests__/reference/backpack/backpack.exchange.ts:728-779,1207-1229.
// KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: backpack | sdk-rs/venues/backpack/src/trading.rs:522 pair-batching-and-serial-rows
#[test]
fn legacy_rounds_without_rechecking_wire_quantity_bounds() -> Result {
    for (min, raw, wire) in [(0.1, 0.95, "1.0"), (0.14, 0.14, "0.1")] {
        let (mut engine, _) = ready();
        let mut transport = vec![];
        engine.store_mut().update_markets(|market| {
            market.precision.amount = 0.1;
            market.limits.amount.min = min;
            market.limits.amount.max = 0.95;
        });
        let mut last = limit(142.);
        last["amount"] = json!(raw);
        place(&mut engine, vec![limit(140.), limit(141.), last])?;
        let first = packet(&mut engine, &mut transport)?;
        assert!(first.url.ends_with("/orders"));
        reply(&mut engine, &first, 200, &pair_ack(&first)?);
        let last = packet(&mut engine, &mut transport)?;
        let row: Value = serde_json::from_str(last.body.as_deref().ok_or("missing last row")?)?;
        assert_eq!(row["quantity"], wire);
        sequence(
            &transport,
            &[
                (Method::Post, "/api/v1/orders"),
                (Method::Post, "/api/v1/order"),
            ],
        );
        reply(&mut engine, &last, 200, r#"{"id":"last"}"#);
        assert!(actions(&mut engine).iter().any(|action| matches!(
            action,
            Action::Notice(Notice::CommandDone { command: 901, .. })
        )));
    }
    Ok(())
}

// Legacy TS: packages/safe-cex/src/__tests__/reference/backpack/backpack.exchange.ts:906-923,1121-1135.
// KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: backpack | native maxPrice gate (placement, trigger, attached price and amendment)
#[test]
fn legacy_does_not_gate_price_caps_before_placement_or_cancel() -> Result {
    let markets = r#"[{"symbol":"SOL_USDC_PERP","marketType":"PERP","quantityIncrement":"0.01","priceIncrement":"0.01","maxQuantity":"1000","filters":{"price":{"maxPrice":"1000"}}}]"#;
    let (mut engine, _, _) = ready_catalogue(markets, "[]");
    let mut transport = vec![];
    place(&mut engine, vec![limit(140.), limit(141.), limit(1000.01)])?;
    let first = packet(&mut engine, &mut transport)?;
    reply(&mut engine, &first, 200, &pair_ack(&first)?);
    let last = packet(&mut engine, &mut transport)?;
    let row: Value = serde_json::from_str(last.body.as_deref().ok_or("missing over-cap row")?)?;
    assert_eq!(row["price"], "1000.01");
    sequence(
        &transport,
        &[
            (Method::Post, "/api/v1/orders"),
            (Method::Post, "/api/v1/order"),
        ],
    );
    let (mut engine, _, _) = ready_catalogue(markets, &format!("[{ORDER}]"));
    let mut transport = vec![];
    engine.handle(
        2000.,
        Event::Command(Command::AmendOrder {
            command: 901,
            id: "123".into(),
            price: Some(1000.01),
            amount: None,
        }),
    );
    let cancel = packet(&mut engine, &mut transport)?;
    assert_eq!(cancel.method, Method::Delete);
    reply(
        &mut engine,
        &cancel,
        200,
        r#"{"id":"123","status":"Cancelled","executedQuantity":"0"}"#,
    );
    let replacement = packet(&mut engine, &mut transport)?;
    let row: Value =
        serde_json::from_str(replacement.body.as_deref().ok_or("missing replacement")?)?;
    assert_eq!(row["price"], "1000.01");
    sequence(
        &transport,
        &[
            (Method::Delete, "/api/v1/order"),
            (Method::Post, "/api/v1/order"),
        ],
    );
    Ok(())
}

// Legacy TS: packages/safe-cex/src/__tests__/reference/backpack/backpack.exchange.ts:906-923,991-1025,1031-1044.
// KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: backpack | native maxPrice gate (placement, trigger, attached price and amendment)
#[test]
fn legacy_trigger_and_attached_prices_are_sent_past_catalogue_cap() -> Result {
    let markets = r#"[{"symbol":"SOL_USDC_PERP","marketType":"PERP","quantityIncrement":"0.01","priceIncrement":"0.01","maxQuantity":"1000","filters":{"price":{"maxPrice":"1000"}}}]"#;
    for order in [
        json!({"symbol":SYMBOL,"type":"stop_market","side":"sell","amount":1,"price":1000.01}),
        json!({"symbol":SYMBOL,"type":"market","side":"buy","amount":1,"takeProfit":1000.01}),
    ] {
        let (mut engine, _, _) = ready_catalogue(markets, "[]");
        let mut transport = vec![];
        place(&mut engine, vec![order])?;
        let request = packet(&mut engine, &mut transport)?;
        let row: Value =
            serde_json::from_str(request.body.as_deref().ok_or("missing trigger body")?)?;
        assert_eq!(
            row[if row.get("triggerPrice").is_some() {
                "triggerPrice"
            } else {
                "takeProfitTriggerPrice"
            }],
            "1000.01"
        );
        reply(&mut engine, &request, 200, r#"{"id":"accepted"}"#);
        assert!(actions(&mut engine).iter().any(|action| matches!(
            action,
            Action::Notice(Notice::CommandDone { command: 901, .. })
        )));
        assert!(requests(&mut engine).is_empty());
        sequence(&transport, &[(Method::Post, "/api/v1/order")]);
    }
    Ok(())
}

// Legacy TS: packages/safe-cex/src/__tests__/reference/backpack/backpack.exchange.ts:1223-1229.
// KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: backpack | runtime row/pair failure suffix
#[test]
fn legacy_serial_failure_continues_with_complete_reporting() -> Result {
    for status in [Some(400), Some(200), Some(503), None] {
        let (mut engine, _) = ready();
        let mut transport = vec![];
        place(
            &mut engine,
            (1..=3)
                .map(|client| {
                    let mut row = limit(140.);
                    row["clientOrderId"] = json!(client.to_string());
                    row
                })
                .collect(),
        )?;
        let prefix = packet(&mut engine, &mut transport)?;
        reply(&mut engine, &prefix, 200, r#"{"id":"prefix"}"#);
        let middle = packet(&mut engine, &mut transport)?;
        if let Some(status) = status {
            reply(
                &mut engine,
                &middle,
                status,
                r#"{"code":"ORDER_REJECTED","message":"scripted failure"}"#,
            );
        } else {
            engine.handle(
                2000.,
                Event::HttpFailed {
                    request: middle.id,
                    error: "lost reply",
                },
            );
        }
        let suffix = packet(&mut engine, &mut transport)?;
        assert_ne!(suffix.id, middle.id);
        reply(&mut engine, &suffix, 200, r#"{"id":"suffix"}"#);
        let out = actions(&mut engine);
        let error = failure(&out)?;
        let elements = error.elements.as_ref().ok_or("missing elements")?;
        assert_eq!(elements.len(), 3);
        assert!(
            matches!(&elements[0].result,ElementResult::Placed {order_ids} if order_ids==&["prefix"])
        );
        assert!(matches!(&elements[1].result, ElementResult::Unknown { .. }));
        assert!(
            matches!(&elements[2].result,ElementResult::Placed {order_ids} if order_ids==&["suffix"])
        );
        assert_eq!(
            error
                .partial
                .as_ref()
                .ok_or("missing accepted IDs")?
                .order_ids,
            ["prefix", "suffix"]
        );
        sequence(
            &transport,
            &[
                (Method::Post, "/api/v1/order"),
                (Method::Post, "/api/v1/order"),
                (Method::Post, "/api/v1/order"),
            ],
        );
        assert!(requests(&mut engine).is_empty());
    }
    Ok(())
}

// Legacy TS: packages/safe-cex/src/__tests__/reference/backpack/backpack.exchange.ts:991-1025.
// KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: backpack | runtime TP/SL child failure suffix
#[test]
fn legacy_tp_failure_continues_to_sl() -> Result {
    let (mut engine, _) = ready();
    let mut transport = vec![];
    place(
        &mut engine,
        vec![
            json!({"symbol":SYMBOL,"type":"limit","side":"buy","amount":1,"price":140,"takeProfit":150,"stopLoss":130}),
        ],
    )?;
    let entry = packet(&mut engine, &mut transport)?;
    reply(&mut engine, &entry, 200, r#"{"id":"entry"}"#);
    let tp = packet(&mut engine, &mut transport)?;
    reply(
        &mut engine,
        &tp,
        400,
        r#"{"code":"INVALID_ORDER","message":"bad TP"}"#,
    );
    let sl = packet(&mut engine, &mut transport)?;
    let row: Value = serde_json::from_str(sl.body.as_deref().ok_or("missing SL")?)?;
    assert_eq!(row["triggerPrice"], "130.00");
    reply(&mut engine, &sl, 200, r#"{"id":"sl"}"#);
    let out = actions(&mut engine);
    let elements = failure(&out)?
        .elements
        .as_ref()
        .ok_or("missing bracket outcomes")?;
    assert_eq!(elements.len(), 3);
    assert!(matches!(&elements[1].result, ElementResult::Unknown { .. }));
    assert!(matches!(&elements[2].result,ElementResult::Placed {order_ids} if order_ids==&["sl"]));
    sequence(
        &transport,
        &[
            (Method::Post, "/api/v1/order"),
            (Method::Post, "/api/v1/order"),
            (Method::Post, "/api/v1/order"),
        ],
    );
    assert!(requests(&mut engine).is_empty());
    Ok(())
}

// Legacy TS: packages/safe-cex/src/__tests__/reference/backpack/backpack.exchange.ts:1010-1028,1223-1229.
// KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: backpack | runtime TP/SL child failure suffix
#[test]
fn legacy_sl_failure_continues_to_later_entry() -> Result {
    let (mut engine, _) = ready();
    let mut transport = vec![];
    place(
        &mut engine,
        vec![
            json!({"symbol":SYMBOL,"type":"limit","side":"buy","amount":1,"price":140,"takeProfit":150,"stopLoss":130}),
            limit(142.),
        ],
    )?;
    for id in ["entry", "tp"] {
        let request = packet(&mut engine, &mut transport)?;
        reply(&mut engine, &request, 200, &json!({"id":id}).to_string());
    }
    let sl = packet(&mut engine, &mut transport)?;
    let row: Value = serde_json::from_str(sl.body.as_deref().ok_or("missing SL")?)?;
    assert_eq!(row["triggerPrice"], "130.00");
    reply(
        &mut engine,
        &sl,
        400,
        r#"{"code":"INVALID_ORDER","message":"bad SL"}"#,
    );
    let suffix = packet(&mut engine, &mut transport)?;
    let row: Value = serde_json::from_str(suffix.body.as_deref().ok_or("missing suffix")?)?;
    assert_eq!(row["price"], "142.00");
    assert!(row.get("triggerPrice").is_none());
    reply(&mut engine, &suffix, 200, r#"{"id":"suffix"}"#);
    let out = actions(&mut engine);
    let error = failure(&out)?;
    let elements = error.elements.as_ref().ok_or("missing journal")?;
    assert_eq!(elements.len(), 4);
    assert!(matches!(&elements[2].result, ElementResult::Unknown { .. }));
    assert_eq!(
        error.partial.as_ref().ok_or("missing IDs")?.order_ids,
        ["entry", "tp", "suffix"]
    );
    sequence(
        &transport,
        &[
            (Method::Post, "/api/v1/order"),
            (Method::Post, "/api/v1/order"),
            (Method::Post, "/api/v1/order"),
            (Method::Post, "/api/v1/order"),
        ],
    );
    assert!(requests(&mut engine).is_empty());
    Ok(())
}

// Legacy TS: packages/safe-cex/src/__tests__/reference/backpack/backpack.exchange.ts:1072-1079.
// KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: backpack | CancelOrders accepted-ID-less effect and failed-row continuation
#[test]
fn legacy_cancel_failure_continues_without_inventing_ids() -> Result {
    let rows = ["123", "456", "789"].map(|id| ORDER.replace("\"123\"", &format!("\"{id}\"")));
    let (mut engine, _) = ready_orders(&format!("[{}]", rows.join(",")));
    let mut transport = vec![];
    engine.handle(
        2000.,
        Event::Command(Command::CancelOrders {
            command: 901,
            ids: vec!["123".into(), "456".into(), "789".into()],
        }),
    );
    let first = packet(&mut engine, &mut transport)?;
    reply(&mut engine, &first, 200, r#"{"id":"123"}"#);
    let middle = packet(&mut engine, &mut transport)?;
    reply(
        &mut engine,
        &middle,
        400,
        r#"{"code":"UNKNOWN","message":"bad cancel"}"#,
    );
    let last = packet(&mut engine, &mut transport)?;
    assert_eq!(last.method, Method::Delete);
    reply(&mut engine, &last, 200, r#"{"id":"789"}"#);
    let out = actions(&mut engine);
    let error = failure(&out)?;
    let elements = error.elements.as_ref().ok_or("missing cancel outcomes")?;
    assert_eq!(elements.len(), 3);
    for (index, id) in [(0, "123"), (2, "789")] {
        assert!(matches!(&elements[index].result,ElementResult::Applied {
            effect:AppliedEffect::Cancel,target_order_id:Some(target),..
        } if target==id));
    }
    assert!(matches!(&elements[1].result, ElementResult::Unknown { .. }));
    assert!(
        error
            .partial
            .as_ref()
            .is_none_or(|partial| partial.order_ids.is_empty())
    );
    sequence(
        &transport,
        &[
            (Method::Delete, "/api/v1/order"),
            (Method::Delete, "/api/v1/order"),
            (Method::Delete, "/api/v1/order"),
        ],
    );
    assert!(requests(&mut engine).is_empty());
    Ok(())
}

// Legacy TS: packages/safe-cex/src/__tests__/reference/backpack/backpack.exchange.ts:1104-1135.
// KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: backpack | amendment changed-fill/fresh-reference gates
#[test]
fn legacy_amend_preserves_requested_total_after_cancel_fills() -> Result {
    for (amount, expected) in [(None, "5.00"), (Some(4.), "4.00"), (Some(1.), "1.00")] {
        let old = ORDER.replace("\"executedQuantity\":\"0\"", "\"executedQuantity\":\"2\"");
        let (mut engine, _) = ready_orders(&format!("[{old}]"));
        let mut transport = vec![];
        engine.handle(
            2000.,
            Event::Command(Command::AmendOrder {
                command: 901,
                id: "123".into(),
                price: Some(142.),
                amount,
            }),
        );
        let cancel = packet(&mut engine, &mut transport)?;
        assert_eq!(cancel.method, Method::Delete);
        reply(
            &mut engine,
            &cancel,
            200,
            r#"{"id":"123","status":"Cancelled","executedQuantity":"3"}"#,
        );
        let replacement = packet(&mut engine, &mut transport)?;
        let row: Value =
            serde_json::from_str(replacement.body.as_deref().ok_or("missing replacement")?)?;
        assert_eq!(row["quantity"], expected);
        sequence(
            &transport,
            &[
                (Method::Delete, "/api/v1/order"),
                (Method::Post, "/api/v1/order"),
            ],
        );
    }
    Ok(())
}

// Legacy TS: packages/safe-cex/src/__tests__/reference/backpack/backpack.exchange.ts:1104-1135.
// KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: backpack | amendment changed-fill/fresh-reference gates
#[test]
fn legacy_stop_amend_does_not_add_a_position_or_reference_gate() -> Result {
    let stop=ORDER.replace("\"orderType\":\"Limit\"","\"orderType\":\"Market\",\"triggerPrice\":\"130\",\"triggerQuantity\":\"5\",\"reduceOnly\":true")
        .replace("\"side\":\"Bid\"","\"side\":\"Ask\"");
    let (mut engine, _) = ready_orders(&format!("[{stop}]"));
    let mut transport = vec![];
    engine.store_mut().set_positions(vec![]);
    engine.handle(
        2000.,
        Event::Command(Command::AmendOrder {
            command: 901,
            id: "123".into(),
            price: Some(125.),
            amount: None,
        }),
    );
    let cancel = packet(&mut engine, &mut transport)?;
    reply(
        &mut engine,
        &cancel,
        200,
        r#"{"id":"123","status":"Cancelled","executedQuantity":"0"}"#,
    );
    let replacement = packet(&mut engine, &mut transport)?;
    let row: Value = serde_json::from_str(replacement.body.as_deref().ok_or("missing stop")?)?;
    assert_eq!(row["triggerPrice"], "125.00");
    assert_eq!(row["triggerQuantity"], "5.00");
    sequence(
        &transport,
        &[
            (Method::Delete, "/api/v1/order"),
            (Method::Post, "/api/v1/order"),
        ],
    );
    Ok(())
}

// Legacy TS: packages/safe-cex/src/__tests__/reference/backpack/backpack.exchange.ts:980-1028.
// KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: backpack | sdk-rs/venues/backpack/src/trading.rs:451 separate-TP-SL-children
#[test]
fn legacy_bracket_sends_each_rounded_quantity_without_a_wire_bound_gate() -> Result {
    let (mut engine, _) = ready();
    let mut transport = vec![];
    engine.store_mut().update_markets(|market| {
        market.precision.amount = 0.1;
        market.limits.amount.max = 0.95;
    });
    place(
        &mut engine,
        vec![
            json!({"symbol":SYMBOL,"type":"limit","side":"buy","amount":0.95,"price":140,"takeProfit":150,"stopLoss":130}),
        ],
    )?;
    for (index, id) in ["entry", "tp", "sl"].into_iter().enumerate() {
        let request = packet(&mut engine, &mut transport)?;
        let row: Value =
            serde_json::from_str(request.body.as_deref().ok_or("missing bracket body")?)?;
        assert_eq!(
            row[if index == 0 {
                "quantity"
            } else {
                "triggerQuantity"
            }],
            "1.0"
        );
        reply(&mut engine, &request, 200, &json!({"id":id}).to_string());
    }
    assert!(actions(&mut engine).iter().any(|action| matches!(
        action,
        Action::Notice(Notice::CommandDone { command: 901, .. })
    )));
    sequence(
        &transport,
        &[
            (Method::Post, "/api/v1/order"),
            (Method::Post, "/api/v1/order"),
            (Method::Post, "/api/v1/order"),
        ],
    );
    assert!(requests(&mut engine).is_empty());
    Ok(())
}

// Legacy TS: packages/safe-cex/src/__tests__/reference/backpack/backpack.exchange.ts:984,1047-1049,1223-1229.
// KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: backpack | runtime TP/SL child failure suffix
#[test]
fn legacy_failed_entry_skips_its_children_but_continues_other_orders() -> Result {
    let (mut engine, _) = ready();
    let mut transport = vec![];
    place(
        &mut engine,
        vec![
            json!({"symbol":SYMBOL,"type":"limit","side":"buy","amount":1,"price":140,"takeProfit":150,"stopLoss":130}),
            limit(142.),
        ],
    )?;
    let entry = packet(&mut engine, &mut transport)?;
    reply(
        &mut engine,
        &entry,
        400,
        r#"{"code":"INVALID_ORDER","message":"bad entry"}"#,
    );
    let suffix = packet(&mut engine, &mut transport)?;
    let row: Value = serde_json::from_str(suffix.body.as_deref().ok_or("missing later order")?)?;
    assert_eq!(row["price"], "142.00");
    assert!(row.get("triggerPrice").is_none());
    reply(&mut engine, &suffix, 200, r#"{"id":"suffix"}"#);
    let out = actions(&mut engine);
    let elements = failure(&out)?.elements.as_ref().ok_or("missing outcomes")?;
    assert_eq!(elements.len(), 4);
    assert!(matches!(&elements[0].result, ElementResult::Unknown { .. }));
    assert!(
        elements[1..3]
            .iter()
            .all(|element| matches!(element.result, ElementResult::NotSent { error: None }))
    );
    assert!(
        matches!(&elements[3].result, ElementResult::Placed {order_ids} if order_ids==&["suffix"])
    );
    sequence(
        &transport,
        &[
            (Method::Post, "/api/v1/order"),
            (Method::Post, "/api/v1/order"),
        ],
    );
    assert!(requests(&mut engine).is_empty());
    Ok(())
}

// Legacy TS: packages/safe-cex/src/__tests__/reference/backpack/backpack.exchange.ts:1213-1219.
// KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: backpack | runtime row/pair failure suffix
#[test]
fn legacy_mixed_pair_failure_still_releases_the_next_pair() -> Result {
    let (mut engine, _) = ready();
    let mut transport = vec![];
    engine
        .store_mut()
        .update_markets(|market| market.limits.amount.max = 1000.);
    place(
        &mut engine,
        [140., 141., 142., 143.].into_iter().map(limit).collect(),
    )?;
    let first = packet(&mut engine, &mut transport)?;
    let rows: Vec<Value> =
        serde_json::from_str(first.body.as_deref().ok_or("missing first pair")?)?;
    let ack:Vec<_> = rows.iter().enumerate().map(|(index,row)| json!({
        "operation":if index == 0 {"Ok"} else {"Error"},
        "id":if index == 0 {"prefix"} else {""},
        "clientId":row["clientId"],"symbol":row["symbol"],"side":row["side"],"orderType":row["orderType"]
    })).collect();
    reply(&mut engine, &first, 200, &serde_json::to_string(&ack)?);
    let second = packet(&mut engine, &mut transport)?;
    assert!(second.url.ends_with("/orders"));
    assert_ne!(second.id, first.id);
    reply(&mut engine, &second, 200, &pair_ack(&second)?);
    let out = actions(&mut engine);
    let error = failure(&out)?;
    let elements = error
        .elements
        .as_ref()
        .ok_or("missing correlated outcomes")?;
    assert_eq!(elements.len(), 4);
    assert!(
        matches!(&elements[0].result, ElementResult::Placed {order_ids} if order_ids==&["prefix"])
    );
    assert!(matches!(&elements[1].result, ElementResult::Unknown { .. }));
    for (index, id) in [(2, "pair-0"), (3, "pair-1")] {
        assert!(
            matches!(&elements[index].result, ElementResult::Placed {order_ids} if order_ids==&[id])
        );
    }
    assert_eq!(
        error
            .partial
            .as_ref()
            .ok_or("missing accepted IDs")?
            .order_ids,
        ["prefix", "pair-0", "pair-1"]
    );
    sequence(
        &transport,
        &[
            (Method::Post, "/api/v1/orders"),
            (Method::Post, "/api/v1/orders"),
        ],
    );
    assert!(requests(&mut engine).is_empty());
    Ok(())
}
