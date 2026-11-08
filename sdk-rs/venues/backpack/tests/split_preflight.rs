mod support;

use safe_cex_engine::write_plan::{AppliedEffect, ElementResult};
use safe_cex_engine::{
    Action, Command, ErrorKind, Event, HttpRequest, Method, Notice, Outcome, VenueError,
};
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

fn refused_before_transport(engine: &mut Driver) -> Result {
    let mut transport = ScriptedTransport::default();
    let out = transport.poll(engine);
    assert_eq!(
        transport.requests.len(),
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
    transport.poll(engine);
    assert!(transport.requests.is_empty());
    Ok(())
}

const PRICE_CAPPED_MARKETS: &str = r#"[{"symbol":"SOL_USDC_PERP","marketType":"PERP","filters":{"price":{"minPrice":"0.01","maxPrice":"1000","tickSize":"0.01"},"quantity":{"minQuantity":"0.01","maxQuantity":"1000","stepSize":"0.01"}}}]"#;

#[test]
fn native_max_price_in_later_write_refuses_entire_plan() -> Result {
    for price in [1000.01, 1000.006] {
        let (mut engine, _, _) = ready_catalogue(PRICE_CAPPED_MARKETS, "[]");
        let mut last = limit(1.);
        last["price"] = json!(price);
        submit(&mut engine, vec![limit(1.), limit(1.), last])?;
        refused_before_transport(&mut engine)?;
    }
    Ok(())
}

#[test]
fn native_max_price_replacement_refuses_before_cancel() -> Result {
    let (mut engine, _, _) = ready_catalogue(PRICE_CAPPED_MARKETS, &format!("[{ORDER}]"));
    engine.handle(
        2000.,
        Event::Command(Command::AmendOrder {
            command: 901,
            id: "123".into(),
            price: Some(1000.01),
            amount: None,
        }),
    );
    refused_before_transport(&mut engine)
}

#[test]
fn native_max_price_checks_protective_prices_before_entry() -> Result {
    for kind in ["limit", "market"] {
        for field in ["takeProfit", "stopLoss"] {
            let (mut engine, _, _) = ready_catalogue(PRICE_CAPPED_MARKETS, "[]");
            let mut order = limit(1.);
            order["type"] = json!(kind);
            order[field] = json!(1000.01);
            submit(&mut engine, vec![order])?;
            refused_before_transport(&mut engine)?;
        }
    }
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

#[test]
fn catalogue_refresh_keeps_native_caps_aligned_with_accepted_market_rows() -> Result {
    let mut catalogue: Vec<Value> = serde_json::from_str(PRICE_CAPPED_MARKETS)?;
    catalogue.push(json!({"symbol":"BTC_USDC_PERP","marketType":"PERP","quantityIncrement":"0"}));
    let partial = serde_json::to_string(&catalogue)?;
    let btc = json!({"symbol":"BTC_USDC_PERP","marketType":"PERP","quantityIncrement":"0.001","priceIncrement":"0.1"});
    for (max, allowed) in [
        (None, false),
        (Some("900"), false),
        (Some("2000"), true),
        (Some("absent"), true),
        (Some("bad"), false),
    ] {
        let (mut engine, _, timers) = ready_catalogue(&partial, "[]");
        let timer = timers
            .into_iter()
            .find_map(|(timer, at)| jittered(at - 1000., 1000.).then_some(timer))
            .ok_or("missing partial catalogue retry")?;
        engine.handle(2000., Event::TimerFired { timer });
        let packet = requests(&mut engine)
            .into_iter()
            .find(|request| request.url.ends_with("/markets"))
            .ok_or("missing catalogue refresh")?;
        let mut rows = vec![btc.clone()];
        if let Some(max) = max {
            let mut sol = catalogue[0].clone();
            if max == "absent" {
                sol["filters"]["price"] = json!({"minPrice":"0.01","tickSize":"0.01"});
            } else {
                sol["filters"]["price"]["maxPrice"] = json!(max);
            }
            rows.push(sol);
        }
        reply(&mut engine, &packet, 200, &serde_json::to_string(&rows)?);
        assert!(engine.store().market(SYMBOL).is_some());
        let mut order = limit(1.);
        order["price"] = json!(1000.01);
        submit(&mut engine, vec![order])?;
        if allowed {
            let writes = requests(&mut engine);
            assert_eq!(writes.len(), 1, "{max:?}");
            assert_eq!(writes[0].method, Method::Post);
        } else {
            refused_before_transport(&mut engine)?;
        }
    }
    Ok(())
}

#[test]
fn protective_rejection_never_reads_proof_or_puts_back_original_stop() -> Result {
    let stop = ORDER
        .replace("\"orderType\":\"Limit\"", "\"orderType\":\"Market\",\"triggerPrice\":\"130\",\"triggerQuantity\":\"5\",\"reduceOnly\":true")
        .replace("\"side\":\"Bid\"", "\"side\":\"Ask\"");
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
            price: Some(125.),
            amount: None,
        }),
    );
    let mut transport = ScriptedTransport {
        responses: [
            (
                200,
                r#"{"id":"123","status":"Cancelled","executedQuantity":"0"}"#.into(),
            ),
            (400, r#"{"code":"INVALID_ORDER","message":"bad"}"#.into()),
            (200, "[]".into()),
        ]
        .into(),
        ..Default::default()
    };
    transport.poll(&mut engine);
    assert_eq!(transport.requests[0].method, Method::Delete);
    transport.poll(&mut engine);
    let replacement = transport
        .requests
        .get(1)
        .ok_or("missing replacement")?
        .clone();
    assert_eq!(replacement.method, Method::Post);
    let row: Value = serde_json::from_str(replacement.body.as_deref().ok_or("missing body")?)?;
    assert_eq!(row["triggerPrice"], "125.00");
    assert_eq!(row["triggerQuantity"], "5.00");
    let out = transport.poll(&mut engine);
    let error = failed(&out)?;
    assert_eq!(error.outcome, Some(Outcome::Unknown));
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
    assert_eq!(transport.responses, [(200, "[]".into())]);
    reply(&mut engine, &replacement, 200, "[]");
    let late = transport.poll(&mut engine);
    assert!(!late.iter().any(|action| matches!(
        action,
        Action::Notice(Notice::CommandDone { command: 901, .. })
            | Action::Notice(Notice::CommandFailed { command: 901, .. })
    )));
    assert_eq!(transport.requests.len(), 2);
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

fn serial_orders() -> Vec<Value> {
    (1..=3).map(|client| json!({"symbol":SYMBOL,"type":"limit","side":"buy","amount":0.4,"price":140,"clientOrderId":client.to_string()})).collect()
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

fn prefix_failure(error: &VenueError, local: bool) -> Result {
    assert_eq!(
        error.outcome,
        if local { None } else { Some(Outcome::Unknown) }
    );
    assert_eq!(
        error
            .partial
            .as_ref()
            .ok_or("missing accepted prefix")?
            .order_ids,
        ["prefix"]
    );
    let elements = error
        .elements
        .as_ref()
        .ok_or("missing complete element outcomes")?;
    assert_eq!(elements.len(), 3);
    for (index, element) in elements.iter().enumerate() {
        assert_eq!(element.element.index, index);
    }
    assert!(
        matches!(&elements[0].result, ElementResult::Placed { order_ids } if order_ids == &["prefix"])
    );
    if local {
        assert!(
            matches!(&elements[1].result, ElementResult::NotSent { error: Some(error) } if error.outcome == Some(Outcome::NotSent))
        );
    } else {
        assert!(
            matches!(&elements[1].result, ElementResult::Unknown { error } if error.outcome == Some(Outcome::Unknown))
        );
    }
    assert!(matches!(
        &elements[2].result,
        ElementResult::NotSent { error: None }
    ));
    Ok(())
}

#[test]
fn runtime_native_failure_retains_prefix_and_stops_suffix() -> Result {
    for (bracket, status) in [(false, 400), (true, 400), (false, 200), (true, 200)] {
        let mut engine = constrained(0.1, 1.);
        let orders = if bracket {
            vec![
                json!({"symbol":SYMBOL,"type":"limit","side":"buy","amount":0.4,"price":140,"takeProfit":150,"stopLoss":130}),
            ]
        } else {
            serial_orders()
        };
        submit(&mut engine, orders)?;
        let mut transport = ScriptedTransport {
            responses: [
                (200, json!({"id":"prefix"}).to_string()),
                (
                    status,
                    json!({"code":"ORDER_REJECTED","message":"scripted refusal"}).to_string(),
                ),
            ]
            .into(),
            ..Default::default()
        };
        transport.poll(&mut engine);
        transport.poll(&mut engine);
        let out = transport.poll(&mut engine);
        let error = failed(&out)?;
        prefix_failure(error, false)?;
        let elements = error.elements.as_ref().ok_or("missing elements")?;
        assert!(
            matches!(&elements[1].result, ElementResult::Unknown { error } if error.venue_code.as_deref() == Some("ORDER_REJECTED") && error.venue_message.as_deref() == Some("scripted refusal"))
        );
        transport.poll(&mut engine);
        assert_eq!(transport.requests.len(), 2);
        assert!(
            transport
                .requests
                .iter()
                .all(|request| request.method == Method::Post)
        );
    }
    Ok(())
}

#[test]
fn lost_or_unproven_reply_retains_prefix_and_stops_suffix() -> Result {
    for status in [None, Some(503)] {
        let mut engine = constrained(0.1, 1.);
        submit(&mut engine, serial_orders())?;
        let mut transport = ScriptedTransport {
            responses: [(200, json!({"id":"prefix"}).to_string())].into(),
            ..Default::default()
        };
        transport.poll(&mut engine);
        transport.poll(&mut engine);
        let request = transport.requests.last().ok_or("missing second write")?;
        match status {
            Some(status) => reply(&mut engine, request, status, "service unavailable"),
            None => engine.handle(
                2000.,
                Event::HttpFailed {
                    request: request.id,
                    error: "lost reply",
                },
            ),
        }
        let out = transport.poll(&mut engine);
        prefix_failure(failed(&out)?, false)?;
        transport.poll(&mut engine);
        assert_eq!(transport.requests.len(), 2);
        assert!(
            transport
                .requests
                .iter()
                .all(|request| request.method == Method::Post)
        );
    }
    Ok(())
}

#[test]
fn local_preparation_veto_marks_pending_not_sent_after_prefix() -> Result {
    use safe_cex_engine::request_context::{Error, HttpAction};
    let mut engine = constrained(0.1, 1.);
    submit(&mut engine, serial_orders())?;
    let first = requests(&mut engine).pop().ok_or("missing prefix")?;
    reply(&mut engine, &first, 200, r#"{"id":"prefix"}"#);
    let token = engine
        .drain_actions()
        .find_map(|action| match action {
            Action::ManagedHttp(action) => match *action {
                HttpAction::Prepare { preparation, .. } => Some(preparation),
                _ => None,
            },
            _ => None,
        })
        .ok_or("missing pending preparation")?;
    engine.http_preparation_failed(
        2000.,
        token,
        Error::Invalid,
        VenueError::new("provider refused"),
    );
    let out = actions(&mut engine);
    prefix_failure(failed(&out)?, true)?;
    assert!(!out.iter().any(|action| matches!(action, Action::Http(_))));
    assert!(
        !actions(&mut engine)
            .iter()
            .any(|action| matches!(action, Action::Http(_)))
    );
    Ok(())
}

#[test]
fn mixed_pair_keeps_correlated_row_evidence_and_never_dispatches_next_pair() -> Result {
    let mut engine = constrained(0.1, 1.);
    submit(
        &mut engine,
        vec![limit(0.4), limit(0.5), limit(0.6), limit(0.7)],
    )?;
    let packet = requests(&mut engine).pop().ok_or("missing first pair")?;
    let rows: Vec<Value> =
        serde_json::from_str(packet.body.as_deref().ok_or("missing pair body")?)?;
    let body: Vec<_> = rows
        .iter()
        .enumerate()
        .map(|(index, row)| {
            json!({
                "operation": if index == 0 { "Ok" } else { "Error" },
                "id": if index == 0 { "prefix" } else { "" },
                "clientId":row["clientId"], "symbol":SYMBOL, "side":"Bid", "orderType":"Limit"
            })
        })
        .collect();
    reply(&mut engine, &packet, 200, &serde_json::to_string(&body)?);
    let out = actions(&mut engine);
    let elements = failed(&out)?
        .elements
        .as_ref()
        .ok_or("missing pair element outcomes")?;
    assert_eq!(elements.len(), 4);
    assert!(
        matches!(&elements[0].result, ElementResult::Placed { order_ids } if order_ids == &["prefix"])
    );
    assert!(matches!(&elements[1].result, ElementResult::Unknown { .. }));
    assert!(
        elements[2..]
            .iter()
            .all(|element| matches!(element.result, ElementResult::NotSent { error: None }))
    );
    assert!(!out.iter().any(|action| matches!(action, Action::Http(_))));
    assert!(requests(&mut engine).is_empty());
    Ok(())
}

#[test]
fn accepted_cancel_prefix_is_applied_without_created_ids() -> Result {
    let rows = ["123", "456", "789"].map(|id| ORDER.replace("\"123\"", &format!("\"{id}\"")));
    let (mut engine, _) = ready_orders(&format!("[{}]", rows.join(",")));
    engine.handle(
        2000.,
        Event::Command(Command::CancelOrders {
            command: 901,
            ids: vec!["123".into(), "456".into(), "789".into()],
        }),
    );
    let mut transport = ScriptedTransport {
        responses: [
            (200, r#"{"id":"123"}"#.into()),
            (
                400,
                r#"{"code":"UNKNOWN","message":"scripted failure"}"#.into(),
            ),
        ]
        .into(),
        ..Default::default()
    };
    transport.poll(&mut engine);
    transport.poll(&mut engine);
    let out = transport.poll(&mut engine);
    let error = failed(&out)?;
    let elements = error.elements.as_ref().ok_or("missing cancel outcomes")?;
    assert_eq!(elements.len(), 3);
    assert!(
        matches!(&elements[0].result, ElementResult::Applied { effect: AppliedEffect::Cancel, target_order_id: Some(id), position_key: None } if id == "123")
    );
    assert!(matches!(elements[1].result, ElementResult::Unknown { .. }));
    assert!(matches!(
        elements[2].result,
        ElementResult::NotSent { error: None }
    ));
    assert!(
        error
            .partial
            .as_ref()
            .is_none_or(|partial| partial.order_ids.is_empty())
    );
    transport.poll(&mut engine);
    assert_eq!(transport.requests.len(), 2);
    assert!(
        transport
            .requests
            .iter()
            .all(|request| request.method == Method::Delete)
    );
    Ok(())
}

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
