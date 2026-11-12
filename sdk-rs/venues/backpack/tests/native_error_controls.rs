mod support;

use safe_cex_engine::{Action, Command, Event, HttpRequest, Method, Notice, Outcome, VenueError};
use serde_json::json;
use std::collections::{BTreeSet, VecDeque};
use support::managed::*;

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

const ENGINE_UNKNOWN_TEXT: &str = "The outcome is unknown. Check Open Orders";
const UNKNOWN_MESSAGE: &str = ENGINE_UNKNOWN_TEXT;

fn assert_unknown(error: &VenueError) {
    assert_eq!(error.outcome, Some(Outcome::Unknown));
    assert!(error.message.contains(ENGINE_UNKNOWN_TEXT));
    assert_eq!(error.message, UNKNOWN_MESSAGE);
    assert!(!error.message.to_ascii_lowercase().contains("rejected"));
}

struct RecordingTransport {
    requests: Vec<HttpRequest>,
    responses: VecDeque<String>,
}

impl RecordingTransport {
    fn new(child_response: &str) -> Self {
        Self {
            requests: vec![],
            responses: [
                r#"{"id":"accepted-entry"}"#.into(),
                child_response.into(),
                r#"{"id":"accepted-sl"}"#.into(),
            ]
            .into(),
        }
    }

    fn poll(&mut self, engine: &mut Driver) -> Vec<Action> {
        let out = actions(engine);
        for action in &out {
            if let Action::Http(request) = action {
                self.requests.push(request.clone());
                if let Some(body) = self.responses.pop_front() {
                    reply(engine, request, 200, &body);
                }
            }
        }
        out
    }

    fn drain(&mut self, engine: &mut Driver) -> Vec<Action> {
        let mut trace = vec![];
        for _ in 0..8 {
            let out = self.poll(engine);
            let idle = out.is_empty();
            trace.extend(out);
            if idle {
                break;
            }
        }
        trace
    }

    fn assert_no_resend_or_cancel(&self, expected: usize) {
        assert_eq!(self.requests.len(), expected);
        let ids: BTreeSet<_> = self.requests.iter().map(|request| request.id.0).collect();
        assert_eq!(ids.len(), expected);
        assert!(
            self.requests
                .iter()
                .all(|request| request.method == Method::Post && request.url.ends_with("/order"))
        );
    }
}

fn start_bracket(engine: &mut Driver) -> Result {
    engine.handle(
        2000.,
        Event::Command(Command::PlaceOrders {
            command: 904,
            orders: serde_json::from_value(json!([{
                "symbol":SYMBOL,"type":"limit","side":"buy","amount":1,"price":140,
                "takeProfit":150,"stopLoss":130
            }]))?,
        }),
    );
    Ok(())
}

fn unknown_child_stops(body: &str, expected_code: Option<&str>, expected_message: &str) -> Result {
    let (mut engine, _) = ready();
    let mut transport = RecordingTransport::new(body);
    start_bracket(&mut engine)?;
    let out = transport.drain(&mut engine);
    transport.assert_no_resend_or_cancel(2);
    assert_eq!(
        transport.responses.len(),
        1,
        "the last child must remain unsent"
    );
    let failures: Vec<_> = out
        .iter()
        .filter_map(|action| match action {
            Action::Notice(Notice::CommandFailed {
                command: 904,
                error,
            }) => Some(error),
            _ => None,
        })
        .collect();
    assert_eq!(failures.len(), 1);
    let error = failures.first().ok_or("missing native failure")?;
    assert_unknown(error);
    let warning = out
        .iter()
        .find_map(|action| match action {
            Action::Notice(Notice::Event(event))
                if event.code == safe_cex_engine::log::code::CHILD_LEG_FAILED =>
            {
                Some(event)
            }
            _ => None,
        })
        .ok_or("missing child failure warning")?;
    assert_eq!(warning.message, error.message);
    assert_eq!(error.venue_code.as_deref(), expected_code);
    assert_eq!(error.venue_message.as_deref(), Some(expected_message));
    assert_eq!(
        error
            .partial
            .as_ref()
            .ok_or("lost prior accepted ID")?
            .order_ids,
        ["accepted-entry"]
    );
    assert!(!out.iter().any(|action| matches!(
        action,
        Action::Notice(Notice::CommandDone { command: 904, .. })
    )));
    let child = transport
        .requests
        .get(1)
        .ok_or("missing first child")?
        .clone();
    reply(&mut engine, &child, 200, r#"{"id":"late-child"}"#);
    let late = transport.drain(&mut engine);
    transport.assert_no_resend_or_cancel(2);
    assert!(!late.iter().any(|action| matches!(
        action,
        Action::Notice(
            Notice::CommandDone { command: 904, .. } | Notice::CommandFailed { command: 904, .. }
        )
    )));
    Ok(())
}

#[test]
fn http200_code_envelope_keeps_unknown_and_stops_children() -> Result {
    unknown_child_stops(
        r#"{"code":"NATIVE_UNAVAILABLE","message":"synthetic detail"}"#,
        Some("NATIVE_UNAVAILABLE"),
        "synthetic detail",
    )
}

#[test]
fn http200_error_envelope_keeps_unknown_and_stops_children() -> Result {
    unknown_child_stops(
        r#"{"error":"synthetic unavailable"}"#,
        None,
        "synthetic unavailable",
    )
}

#[test]
fn definitive_trigger_replacement_refusal_keeps_rejection_wording() -> Result {
    let stop = ORDER
        .replace("\"orderType\":\"Limit\"", "\"orderType\":\"Market\",\"triggerPrice\":\"130\",\"triggerQuantity\":\"5\",\"reduceOnly\":true")
        .replace("\"side\":\"Bid\"", "\"side\":\"Ask\"")
        .replace("\"executedQuantity\":\"0\"", "\"executedQuantity\":\"1\"");
    let (mut engine, _) = ready_orders(&format!("[{stop}]"));
    engine.store_mut().update_ticker(SYMBOL, |ticker| {
        ticker.last = 140.;
        ticker.mark = 140.;
        ticker.index = 140.;
    });
    engine.handle(
        2000.,
        Event::Command(Command::AmendOrder {
            command: 905,
            id: "123".into(),
            price: Some(125.),
            amount: None,
        }),
    );
    let cancel = requests(&mut engine).pop().ok_or("missing cancel")?;
    assert_eq!(cancel.method, Method::Delete);
    reply(
        &mut engine,
        &cancel,
        200,
        r#"{"id":"123","status":"Cancelled","executedQuantity":"1"}"#,
    );
    let replacement = requests(&mut engine).pop().ok_or("missing replacement")?;
    assert_eq!(replacement.method, Method::Post);
    reply(
        &mut engine,
        &replacement,
        400,
        r#"{"code":"INVALID_ORDER","message":"synthetic refusal"}"#,
    );
    let out = actions(&mut engine);
    let error = out
        .iter()
        .find_map(|action| match action {
            Action::Notice(Notice::CommandFailed {
                command: 905,
                error,
            }) => Some(error),
            _ => None,
        })
        .ok_or("missing definitive refusal")?;
    assert_eq!(error.kind, Some(safe_cex_engine::ErrorKind::Rejected));
    assert_ne!(error.outcome, Some(Outcome::Unknown));
    assert_eq!(
        error.protection,
        Some(safe_cex_engine::error::Protection::Unprotected)
    );
    assert!(error.message.contains("replacement was refused"));
    assert!(!error.message.contains("outcome is unknown"));
    assert_eq!(error.venue_code.as_deref(), Some("INVALID_ORDER"));
    assert_eq!(error.venue_message.as_deref(), Some("synthetic refusal"));
    assert!(!out.iter().any(|action| matches!(action, Action::Http(_))));
    assert!(requests(&mut engine).is_empty());
    Ok(())
}

#[test]
fn genuine_http200_success_continues_each_child_once() -> Result {
    let (mut engine, _) = ready();
    let mut transport = RecordingTransport::new(r#"{"id":"accepted-tp"}"#);
    start_bracket(&mut engine)?;
    let out = transport.drain(&mut engine);
    transport.assert_no_resend_or_cancel(3);
    assert!(transport.responses.is_empty());
    for (index, expected) in ["", "150.00", "130.00"].into_iter().enumerate() {
        let request = transport
            .requests
            .get(index)
            .ok_or("missing bracket write")?;
        let row: serde_json::Value =
            serde_json::from_str(request.body.as_deref().ok_or("missing body")?)?;
        if index == 0 {
            assert!(row.get("triggerPrice").is_none());
        } else {
            assert_eq!(row["triggerPrice"], expected);
        }
    }
    let done: Vec<_> = out
        .iter()
        .filter_map(|action| match action {
            Action::Notice(Notice::CommandDone {
                command: 904,
                result,
            }) => Some(result),
            _ => None,
        })
        .collect();
    assert_eq!(done.len(), 1);
    assert_eq!(
        serde_json::from_str::<Vec<String>>(done.first().ok_or("missing success")?)?,
        ["accepted-entry", "accepted-tp", "accepted-sl"]
    );
    assert!(!out.iter().any(|action| matches!(
        action,
        Action::Notice(Notice::CommandFailed { command: 904, .. })
    )));
    Ok(())
}

#[test]
fn dispatched_child_transport_failure_guides_terminal_and_warning() -> Result {
    let (mut engine, _) = ready();
    start_bracket(&mut engine)?;
    let entry = requests(&mut engine).pop().ok_or("missing entry")?;
    reply(&mut engine, &entry, 200, r#"{"id":"accepted-entry"}"#);
    let child = requests(&mut engine).pop().ok_or("missing child")?;
    engine.handle(
        2000.,
        Event::HttpFailed {
            request: child.id,
            error: "ambiguous transport loss",
        },
    );
    let out = actions(&mut engine);
    let error = out
        .iter()
        .find_map(|action| match action {
            Action::Notice(Notice::CommandFailed {
                command: 904,
                error,
            }) => Some(error),
            _ => None,
        })
        .ok_or("missing terminal")?;
    assert_unknown(error);
    assert_eq!(
        error
            .partial
            .as_ref()
            .ok_or("missing accepted ID")?
            .order_ids,
        ["accepted-entry"]
    );
    let warning = out
        .iter()
        .find_map(|action| match action {
            Action::Notice(Notice::Event(event))
                if event.code == safe_cex_engine::log::code::CHILD_LEG_FAILED =>
            {
                Some(event)
            }
            _ => None,
        })
        .ok_or("missing child warning")?;
    assert_eq!(warning.message, error.message);
    assert!(!out.iter().any(|action| matches!(action, Action::Http(_))));
    assert!(requests(&mut engine).is_empty());
    assert_ne!(entry.id, child.id);
    assert_eq!(entry.method, Method::Post);
    assert_eq!(child.method, Method::Post);
    reply(&mut engine, &child, 200, r#"{"id":"late-child"}"#);
    let late = actions(&mut engine);
    assert!(!late.iter().any(|action| matches!(
        action,
        Action::Http(_)
            | Action::Notice(
                Notice::CommandDone { command: 904, .. }
                    | Notice::CommandFailed { command: 904, .. }
            )
    )));
    Ok(())
}

enum BatchReply {
    Missing,
    Error,
    Duplicate,
    Malformed,
    IdentityMismatch,
    Extra,
    Accepted,
}

fn batch_reply(mode: BatchReply) -> Result {
    let (mut engine, _) = ready();
    engine
        .store_mut()
        .update_markets(|market| market.limits.amount.max = 1000.);
    engine.handle(
        2000.,
        Event::Command(Command::PlaceOrders {
            command: 906,
            orders: serde_json::from_value(json!([
                {"symbol":SYMBOL,"type":"limit","side":"buy","amount":1,"price":140},
                {"symbol":SYMBOL,"type":"limit","side":"sell","amount":1,"price":150}
            ]))?,
        }),
    );
    let sent = requests(&mut engine);
    assert_eq!(sent.len(), 1);
    let request = sent.first().ok_or("missing batch")?;
    assert_eq!(request.method, Method::Post);
    assert!(request.url.ends_with("/orders"));
    let rows: Vec<serde_json::Value> =
        serde_json::from_str(request.body.as_deref().ok_or("missing batch body")?)?;
    assert_eq!(rows.len(), 2);
    let row = |index: usize, operation: &str, id: &str| {
        json!({
            "operation":operation,"id":id,"clientId":rows[index]["clientId"],
            "symbol":rows[index]["symbol"],"side":rows[index]["side"],"orderType":rows[index]["orderType"]
        })
    };
    let first = row(0, "Ok", "placed-one");
    let second = row(1, "Ok", "placed-two");
    let body = match mode {
        BatchReply::Missing => json!([first]).to_string(),
        BatchReply::Error => json!([first, row(1, "Error", "")]).to_string(),
        BatchReply::Duplicate => json!([first, first]).to_string(),
        BatchReply::Malformed => "[".into(),
        BatchReply::IdentityMismatch => {
            let mut mismatch = second;
            mismatch["symbol"] = json!("WRONG_PERP");
            json!([first, mismatch]).to_string()
        }
        BatchReply::Extra => json!([first, second, {"operation":"Error"}]).to_string(),
        BatchReply::Accepted => json!([first, second]).to_string(),
    };
    reply(&mut engine, request, 200, &body);
    let out = actions(&mut engine);
    assert!(!out.iter().any(|action| matches!(action, Action::Http(_))));
    assert!(requests(&mut engine).is_empty());
    if matches!(mode, BatchReply::Accepted) {
        let result = out
            .iter()
            .find_map(|action| match action {
                Action::Notice(Notice::CommandDone {
                    command: 906,
                    result,
                }) => Some(result),
                _ => None,
            })
            .ok_or("missing batch success")?;
        assert_eq!(
            serde_json::from_str::<Vec<String>>(result)?,
            ["placed-one", "placed-two"]
        );
        assert!(!out.iter().any(|action| matches!(
            action,
            Action::Notice(Notice::CommandFailed { command: 906, .. })
        )));
        return Ok(());
    }
    let error = out
        .iter()
        .find_map(|action| match action {
            Action::Notice(Notice::CommandFailed {
                command: 906,
                error,
            }) => Some(error),
            _ => None,
        })
        .ok_or("missing batch failure")?;
    let ids = error
        .partial
        .as_ref()
        .map_or(&[][..], |partial| partial.order_ids.as_slice());
    let expected: &[&str] = match mode {
        BatchReply::Duplicate | BatchReply::Malformed => &[],
        BatchReply::Extra => &["placed-one", "placed-two"],
        _ => &["placed-one"],
    };
    assert_eq!(ids, expected);
    assert_unknown(error);
    reply(
        &mut engine,
        request,
        200,
        &json!([row(0, "Ok", "late-one"), row(1, "Ok", "late-two")]).to_string(),
    );
    let late = actions(&mut engine);
    assert!(!late.iter().any(|action| matches!(
        action,
        Action::Http(_)
            | Action::Notice(
                Notice::CommandDone { command: 906, .. }
                    | Notice::CommandFailed { command: 906, .. }
            )
    )));
    Ok(())
}

#[test]
fn missing_batch_row_guides_unknown() -> Result {
    batch_reply(BatchReply::Missing)
}
#[test]
fn failed_batch_row_guides_unknown() -> Result {
    batch_reply(BatchReply::Error)
}
#[test]
fn duplicate_batch_row_guides_unknown() -> Result {
    batch_reply(BatchReply::Duplicate)
}
#[test]
fn malformed_batch_reply_guides_unknown() -> Result {
    batch_reply(BatchReply::Malformed)
}
#[test]
fn mismatched_batch_identity_guides_unknown() -> Result {
    batch_reply(BatchReply::IdentityMismatch)
}
#[test]
fn extra_batch_row_guides_unknown() -> Result {
    batch_reply(BatchReply::Extra)
}
#[test]
fn accepted_batch_keeps_success_control() -> Result {
    batch_reply(BatchReply::Accepted)
}

fn single_reply(status: Option<u16>, body: &str) -> Result {
    let (mut engine, _) = ready();
    engine.handle(
        2000.,
        Event::Command(Command::PlaceOrders {
            command: 907,
            orders: serde_json::from_value(json!([
                {"symbol":SYMBOL,"type":"limit","side":"buy","amount":1,"price":140}
            ]))?,
        }),
    );
    let sent = requests(&mut engine);
    assert_eq!(sent.len(), 1);
    let request = sent.first().ok_or("missing order")?;
    if let Some(status) = status {
        reply(&mut engine, request, status, body);
    } else {
        engine.handle(
            2000.,
            Event::HttpFailed {
                request: request.id,
                error: "ambiguous transport loss",
            },
        );
    }
    let out = actions(&mut engine);
    let error = out
        .iter()
        .find_map(|action| match action {
            Action::Notice(Notice::CommandFailed {
                command: 907,
                error,
            }) => Some(error),
            _ => None,
        })
        .ok_or("missing single-order failure")?;
    assert_unknown(error);
    assert!(!out.iter().any(|action| matches!(action, Action::Http(_))));
    assert!(requests(&mut engine).is_empty());
    Ok(())
}

#[test]
fn standalone_transport_failure_guides_unknown() -> Result {
    single_reply(None, "")
}

fn nonplacement_transport_failure(command: Command, method: Method, path: &str) -> Result {
    let (mut engine, _) = ready_orders(&format!("[{ORDER}]"));
    engine.handle(2000., Event::Command(command));
    let sent = requests(&mut engine);
    assert_eq!(sent.len(), 1);
    let request = sent.first().ok_or("missing mutation")?;
    assert_eq!(request.method, method);
    assert!(request.url.ends_with(path));
    engine.handle(
        2000.,
        Event::HttpFailed {
            request: request.id,
            error: "ambiguous transport loss",
        },
    );
    let out = actions(&mut engine);
    let error = out
        .iter()
        .find_map(|action| match action {
            Action::Notice(Notice::CommandFailed {
                command: 908,
                error,
            }) => Some(error),
            _ => None,
        })
        .ok_or("missing mutation failure")?;
    assert_unknown(error);
    assert!(!out.iter().any(|action| matches!(action, Action::Http(_))));
    assert!(requests(&mut engine).is_empty());
    Ok(())
}

#[test]
fn cancel_transport_failure_guides_unknown() -> Result {
    nonplacement_transport_failure(
        Command::CancelOrders {
            command: 908,
            ids: vec!["123".into()],
        },
        Method::Delete,
        "/api/v1/order",
    )
}

#[test]
fn cancel_all_transport_failure_guides_unknown() -> Result {
    nonplacement_transport_failure(
        Command::CancelSymbolOrders {
            command: 908,
            symbol: SYMBOL.into(),
        },
        Method::Delete,
        "/api/v1/orders",
    )
}

#[test]
fn leverage_transport_failure_guides_unknown() -> Result {
    nonplacement_transport_failure(
        Command::SetLeverage {
            command: 908,
            symbol: SYMBOL.into(),
            leverage: 20.,
            side: None,
        },
        Method::Patch,
        "/api/v1/account",
    )
}
#[test]
fn unproven_http200_ack_guides_unknown() -> Result {
    for body in ["", "{", "[]", r#"{"id":""}"#] {
        single_reply(Some(200), body)?;
    }
    Ok(())
}
#[test]
fn unproven_http_statuses_keep_unknown_guidance() -> Result {
    for status in [408, 429, 500, 503] {
        single_reply(Some(status), "")?;
    }
    Ok(())
}
#[test]
fn unclassified_native_http_error_keeps_unknown_guidance() -> Result {
    single_reply(
        Some(400),
        r#"{"code":"INVALID_ORDER","message":"synthetic detail"}"#,
    )
}
