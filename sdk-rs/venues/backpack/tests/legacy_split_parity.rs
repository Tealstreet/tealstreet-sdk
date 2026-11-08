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

fn packet(engine: &mut Driver) -> Result<HttpRequest> {
    let packets = requests(engine);
    assert_eq!(
        packets.len(),
        1,
        "legacy TS dispatches the next independent write"
    );
    packets
        .into_iter()
        .next()
        .ok_or_else(|| "missing write".into())
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

#[test]
fn legacy_rounds_without_rechecking_wire_quantity_bounds() -> Result {
    for (min, raw, wire) in [(0.1, 0.95, "1.0"), (0.14, 0.14, "0.1")] {
        let (mut engine, _) = ready();
        engine.store_mut().update_markets(|market| {
            market.precision.amount = 0.1;
            market.limits.amount.min = min;
            market.limits.amount.max = 0.95;
        });
        let mut last = limit(142.);
        last["amount"] = json!(raw);
        place(&mut engine, vec![limit(140.), limit(141.), last])?;
        let first = packet(&mut engine)?;
        assert!(first.url.ends_with("/orders"));
        reply(&mut engine, &first, 200, &pair_ack(&first)?);
        let last = packet(&mut engine)?;
        let row: Value = serde_json::from_str(last.body.as_deref().ok_or("missing last row")?)?;
        assert_eq!(row["quantity"], wire);
        reply(&mut engine, &last, 200, r#"{"id":"last"}"#);
        assert!(actions(&mut engine).iter().any(|action| matches!(
            action,
            Action::Notice(Notice::CommandDone { command: 901, .. })
        )));
    }
    Ok(())
}

#[test]
fn legacy_does_not_gate_price_caps_before_placement_or_cancel() -> Result {
    let markets = r#"[{"symbol":"SOL_USDC_PERP","marketType":"PERP","quantityIncrement":"0.01","priceIncrement":"0.01","maxQuantity":"1000","filters":{"price":{"maxPrice":"1000"}}}]"#;
    let (mut engine, _, _) = ready_catalogue(markets, "[]");
    place(&mut engine, vec![limit(140.), limit(141.), limit(1000.01)])?;
    let first = packet(&mut engine)?;
    reply(&mut engine, &first, 200, &pair_ack(&first)?);
    let last = packet(&mut engine)?;
    let row: Value = serde_json::from_str(last.body.as_deref().ok_or("missing over-cap row")?)?;
    assert_eq!(row["price"], "1000.01");
    let (mut engine, _, _) = ready_catalogue(markets, &format!("[{ORDER}]"));
    engine.handle(
        2000.,
        Event::Command(Command::AmendOrder {
            command: 901,
            id: "123".into(),
            price: Some(1000.01),
            amount: None,
        }),
    );
    let cancel = packet(&mut engine)?;
    assert_eq!(cancel.method, Method::Delete);
    reply(
        &mut engine,
        &cancel,
        200,
        r#"{"id":"123","status":"Cancelled","executedQuantity":"0"}"#,
    );
    let replacement = packet(&mut engine)?;
    let row: Value =
        serde_json::from_str(replacement.body.as_deref().ok_or("missing replacement")?)?;
    assert_eq!(row["price"], "1000.01");
    Ok(())
}

#[test]
fn legacy_serial_failure_continues_with_complete_reporting() -> Result {
    for status in [400, 200, 503] {
        let (mut engine, _) = ready();
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
        let prefix = packet(&mut engine)?;
        reply(&mut engine, &prefix, 200, r#"{"id":"prefix"}"#);
        let middle = packet(&mut engine)?;
        reply(
            &mut engine,
            &middle,
            status,
            r#"{"code":"ORDER_REJECTED","message":"scripted failure"}"#,
        );
        let suffix = packet(&mut engine)?;
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
        assert!(requests(&mut engine).is_empty());
    }
    Ok(())
}

#[test]
fn legacy_tp_failure_continues_to_sl() -> Result {
    let (mut engine, _) = ready();
    place(
        &mut engine,
        vec![
            json!({"symbol":SYMBOL,"type":"limit","side":"buy","amount":1,"price":140,"takeProfit":150,"stopLoss":130}),
        ],
    )?;
    let entry = packet(&mut engine)?;
    reply(&mut engine, &entry, 200, r#"{"id":"entry"}"#);
    let tp = packet(&mut engine)?;
    reply(
        &mut engine,
        &tp,
        400,
        r#"{"code":"INVALID_ORDER","message":"bad TP"}"#,
    );
    let sl = packet(&mut engine)?;
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
    assert!(requests(&mut engine).is_empty());
    Ok(())
}

#[test]
fn legacy_cancel_failure_continues_without_inventing_ids() -> Result {
    let rows = ["123", "456", "789"].map(|id| ORDER.replace("\"123\"", &format!("\"{id}\"")));
    let (mut engine, _) = ready_orders(&format!("[{}]", rows.join(",")));
    engine.handle(
        2000.,
        Event::Command(Command::CancelOrders {
            command: 901,
            ids: vec!["123".into(), "456".into(), "789".into()],
        }),
    );
    let first = packet(&mut engine)?;
    reply(&mut engine, &first, 200, r#"{"id":"123"}"#);
    let middle = packet(&mut engine)?;
    reply(
        &mut engine,
        &middle,
        400,
        r#"{"code":"UNKNOWN","message":"bad cancel"}"#,
    );
    let last = packet(&mut engine)?;
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
    assert!(requests(&mut engine).is_empty());
    Ok(())
}
