mod support;

use safe_cex_engine::{Action, Command, Event, HttpRequest, Method, Notice, Outcome};
use serde_json::json;
use std::collections::{BTreeSet, VecDeque};
use support::managed::*;

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

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

fn unknown_child_stops(body: &str, expected_code: Option<&str>) -> Result {
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
    assert_eq!(error.outcome, Some(Outcome::Unknown));
    assert_eq!(error.venue_code.as_deref(), expected_code);
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
    )
}

#[test]
fn http200_error_envelope_keeps_unknown_and_stops_children() -> Result {
    unknown_child_stops(r#"{"error":"synthetic unavailable"}"#, None)
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
