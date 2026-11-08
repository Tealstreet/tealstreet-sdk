mod support;

use safe_cex_engine::{Action, Command, ErrorKind, Event, Method, Notice, Outcome};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use support::managed::*;

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

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

fn refused_before_transport(engine: &mut Driver) -> Result {
    let out = actions(engine);
    assert_eq!(
        out.iter()
            .filter(|action| matches!(action, Action::Http(_)))
            .count(),
        0,
        "the entire intent must be validated before the first transport request"
    );
    let errors: Vec<_> = out
        .iter()
        .filter_map(|action| match action {
            Action::Notice(Notice::CommandFailed {
                command: 901,
                error,
            }) => Some(error),
            _ => None,
        })
        .collect();
    assert_eq!(errors.len(), 1);
    let error = errors.first().ok_or("missing planning failure")?;
    assert_eq!(error.kind, Some(ErrorKind::InvalidInput));
    assert_eq!(error.outcome, Some(Outcome::NotSent));
    assert!(error.partial.is_none());
    assert!(requests(engine).is_empty());
    Ok(())
}

#[test]
fn rounded_second_pair_row_above_max_zero_writes() -> Result {
    for orders in [
        vec![limit(0.4), limit(0.95)],
        vec![limit(0.4), limit(0.5), limit(0.95)],
        vec![limit(0.4), limit(0.5), limit(0.6), limit(0.95)],
    ] {
        let mut engine = constrained(0.1, 0.95);
        submit(&mut engine, orders)?;
        refused_before_transport(&mut engine)?;
    }
    Ok(())
}

#[test]
fn rounded_bracket_quantity_above_max_zero_writes() -> Result {
    for kind in ["limit", "market"] {
        for prefix in [vec![], vec![limit(0.4), limit(0.5)]] {
            let mut orders = prefix;
            orders.push(json!({"symbol":SYMBOL,"type":kind,"side":"buy","amount":0.95,"price":140,"takeProfit":150,"stopLoss":130}));
            let mut engine = constrained(0.1, 0.95);
            submit(&mut engine, orders)?;
            refused_before_transport(&mut engine)?;
        }
    }
    Ok(())
}

#[test]
fn rounded_last_row_below_min_zero_writes() -> Result {
    let mut engine = constrained(0.14, 1.);
    submit(&mut engine, vec![limit(0.4), limit(0.5), limit(0.14)])?;
    refused_before_transport(&mut engine)
}

#[test]
fn rounded_invalid_replacement_never_cancels() -> Result {
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
    refused_before_transport(&mut engine)
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
    let mut sends = 0;
    for (index, id) in ["entry", "tp", "sl"].into_iter().enumerate() {
        let packets = requests(&mut engine);
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
        sends += 1;
        reply(&mut engine, packet, 200, &json!({"id":id}).to_string());
    }
    assert_eq!(sends, 3);
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
        .ok_or("missing successful bracket completion")?;
    assert_eq!(
        serde_json::from_str::<Vec<String>>(result)?,
        ["entry", "tp", "sl"]
    );
    Ok(())
}
