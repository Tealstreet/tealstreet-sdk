mod support;

use safe_cex_engine::{Action, Command, Event, HttpRequest, Method, Notice};
use serde_json::{Value, json};
use support::managed::*;

const ENGINE_UNKNOWN_TEXT: &str = "The outcome is unknown. Check Open Orders";

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

#[derive(Default)]
struct RecordingTransport {
    writes: Vec<HttpRequest>,
    events: Vec<safe_cex_engine::log::LogEvent>,
}

impl RecordingTransport {
    fn poll(&mut self, engine: &mut Driver) -> Vec<Action> {
        let out = actions(engine);
        self.events
            .extend(out.iter().filter_map(|action| match action {
                Action::Notice(Notice::Event(event)) => Some(event.clone()),
                _ => None,
            }));
        self.writes
            .extend(out.iter().filter_map(|action| match action {
                Action::Http(request) if request.method != Method::Get => Some(request.clone()),
                _ => None,
            }));
        out
    }

    fn next(&mut self, engine: &mut Driver) -> Result<HttpRequest> {
        let before = self.writes.len();
        self.poll(engine);
        assert_eq!(self.writes.len(), before + 1, "one next TS write");
        self.writes
            .last()
            .cloned()
            .ok_or_else(|| "missing write".into())
    }

    fn assert_sequence(&self, expected: &[(Method, &str, Value)]) -> Result {
        assert_eq!(self.writes.len(), expected.len());
        for (request, (method, path, body)) in self.writes.iter().zip(expected) {
            assert_eq!(&request.method, method);
            assert!(request.url.ends_with(path), "{}", request.url);
            assert_eq!(
                serde_json::from_str::<Value>(request.body.as_deref().ok_or("missing body")?)?,
                *body
            );
        }
        Ok(())
    }
}

fn amend(engine: &mut Driver, amount: Option<f64>) {
    engine.handle(
        2000.,
        Event::Command(Command::AmendOrder {
            command: 901,
            id: "123".into(),
            price: Some(142.),
            amount,
        }),
    );
}

fn failed(out: &[Action]) {
    assert_eq!(
        out.iter()
            .filter(|action| matches!(
                action,
                Action::Notice(Notice::CommandFailed { command: 901, .. })
            ))
            .count(),
        1
    );
}

// Legacy TS: packages/safe-cex/src/__tests__/reference/backpack/backpack.exchange.ts:1110-1141, failed cancel prevents replacement.
// KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: backpack | AmendOrder pre-cancel wire validation and class-A restore
#[test]
fn amend_cancel_failure_stops_without_replacement_or_restore() -> Result {
    let (mut engine, _) = ready_orders(&format!("[{ORDER}]"));
    let mut transport = RecordingTransport::default();
    amend(&mut engine, None);
    let cancel = transport.next(&mut engine)?;
    reply(
        &mut engine,
        &cancel,
        400,
        r#"{"code":"INVALID_ORDER","message":"bad cancel"}"#,
    );
    failed(&transport.poll(&mut engine));
    transport.poll(&mut engine);
    transport.assert_sequence(&[(
        Method::Delete,
        "/api/v1/order",
        json!({"symbol":SYMBOL,"orderId":"123"}),
    )])
}

// Legacy TS: packages/safe-cex/src/__tests__/reference/backpack/backpack.exchange.ts:1110-1113, nonpositive total refuses before cancel.
#[test]
fn amend_invalid_amount_refuses_before_any_io() -> Result {
    for amount in [0., -1.] {
        let (mut engine, _) = ready_orders(&format!("[{ORDER}]"));
        let mut transport = RecordingTransport::default();
        amend(&mut engine, Some(amount));
        let out = transport.poll(&mut engine);
        failed(&out);
        assert!(out.iter().all(|action| !matches!(action, Action::Http(_))));
        transport.assert_sequence(&[])?;
    }
    Ok(())
}

// Legacy TS: packages/safe-cex/src/__tests__/reference/backpack/backpack.exchange.ts:1203-1204, caller IDs validated before the loop.
#[test]
fn invalid_caller_id_refuses_before_any_io() -> Result {
    for id in ["0", "01", "4294967296", "-1", "1.0", "test"] {
        let (mut engine, _) = ready();
        let mut transport = RecordingTransport::default();
        engine.handle(2000., Event::Command(Command::PlaceOrders {
            command: 901,
            orders: serde_json::from_value(json!([
                {"symbol":SYMBOL,"type":"limit","side":"buy","amount":1,"price":140},
                {"symbol":SYMBOL,"type":"limit","side":"buy","amount":1,"price":141,"clientOrderId":id}
            ]))?,
        }));
        let out = transport.poll(&mut engine);
        failed(&out);
        assert!(out.iter().all(|action| !matches!(action, Action::Http(_))));
        transport.assert_sequence(&[])?;
    }
    Ok(())
}

// Legacy TS: packages/safe-cex/src/__tests__/reference/backpack/backpack.exchange.ts:1145-1166, failed PATCH skips positions refresh.
#[test]
fn leverage_failure_stops_after_global_patch() -> Result {
    let (mut engine, _) = ready();
    let mut transport = RecordingTransport::default();
    engine.handle(
        2000.,
        Event::Command(Command::SetLeverage {
            command: 901,
            symbol: SYMBOL.into(),
            leverage: 20.,
            side: None,
        }),
    );
    let patch = transport.next(&mut engine)?;
    reply(
        &mut engine,
        &patch,
        400,
        r#"{"code":"INVALID_ORDER","message":"bad leverage"}"#,
    );
    let out = transport.poll(&mut engine);
    failed(&out);
    assert!(out.iter().all(|action| !matches!(action, Action::Http(_))));
    transport.poll(&mut engine);
    transport.assert_sequence(&[(
        Method::Patch,
        "/api/v1/account",
        json!({"leverageLimit":"20"}),
    )])
}

// Legacy TS: packages/safe-cex/src/__tests__/reference/backpack/backpack.exchange.ts:1145-1160.
#[test]
fn leverage_success_sends_one_account_wide_write() -> Result {
    let (mut engine, _) = ready();
    let mut transport = RecordingTransport::default();
    engine.handle(
        2000.,
        Event::Command(Command::SetLeverage {
            command: 901,
            symbol: SYMBOL.into(),
            leverage: 20.,
            side: None,
        }),
    );
    let patch = transport.next(&mut engine)?;
    reply(&mut engine, &patch, 200, "{}");
    let out = transport.poll(&mut engine);
    assert!(out.iter().any(|action| matches!(
        action,
        Action::Notice(Notice::CommandDone { command: 901, .. })
    )));
    transport.poll(&mut engine);
    transport.assert_sequence(&[(
        Method::Patch,
        "/api/v1/account",
        json!({"leverageLimit":"20"}),
    )])
}

// PARITY-EXCEPTION: stop-after-UNKNOWN (Sean/daxx, 2026-11-08); TS continues at packages/safe-cex/src/__tests__/reference/backpack/backpack.exchange.ts:543-588.
// KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: backpack | background missing-trigger-source compensating cancel/re-place
#[test]
fn repair_unknown_halts_before_next_candidate() -> Result {
    for fail_cancel in [true, false] {
        let (mut engine, socket) = ready();
        engine.handle(1000., Event::WsOpened { socket });
        let snapshot = requests(&mut engine)
            .into_iter()
            .find(|request| request.url.ends_with("/orders"))
            .ok_or("missing snapshot")?;
        let old = ORDER.replace(
            "\"orderType\":\"Limit\"",
            "\"orderType\":\"Market\",\"triggerPrice\":\"130\",\"triggerQuantity\":\"5\"",
        );
        let second = old
            .replace("\"123\"", "\"456\"")
            .replace("\"clientId\":42", "\"clientId\":43");
        let prefix = old.replace("\"123\"", "\"099\"");
        reply(
            &mut engine,
            &snapshot,
            200,
            &format!("[{prefix},{old},{second}]"),
        );
        let mut transport = RecordingTransport::default();
        let cancel_prefix = transport.next(&mut engine)?;
        reply(
            &mut engine,
            &cancel_prefix,
            200,
            r#"{"id":"099","status":"Cancelled","executedQuantity":"0"}"#,
        );
        let place_prefix = transport.next(&mut engine)?;
        reply(&mut engine, &place_prefix, 200, r#"{"id":"prefix-placed"}"#);
        let first = transport.next(&mut engine)?;
        let mut expected = vec![
            (
                Method::Delete,
                "/api/v1/order",
                json!({"symbol":SYMBOL,"orderId":"099"}),
            ),
            (
                Method::Post,
                "/api/v1/order",
                json!({"symbol":SYMBOL,"side":"Bid","orderType":"Market","triggerPrice":"130.00","triggerQuantity":"5.00","triggerBy":"MarkPrice"}),
            ),
            (
                Method::Delete,
                "/api/v1/order",
                json!({"symbol":SYMBOL,"orderId":"123"}),
            ),
        ];
        if fail_cancel {
            reply(
                &mut engine,
                &first,
                400,
                r#"{"code":"INVALID_ORDER","message":"bad cancel"}"#,
            );
        } else {
            reply(
                &mut engine,
                &first,
                200,
                r#"{"id":"123","status":"Cancelled","executedQuantity":"0"}"#,
            );
            let replacement = transport.next(&mut engine)?;
            expected.push((Method::Post, "/api/v1/order", json!({"symbol":SYMBOL,"side":"Bid","orderType":"Market","triggerPrice":"130.00","triggerQuantity":"5.00","triggerBy":"MarkPrice"})));
            reply(
                &mut engine,
                &replacement,
                400,
                r#"{"code":"INVALID_ORDER","message":"bad replacement"}"#,
            );
        }
        let out = transport.poll(&mut engine);
        assert_eq!(
            transport.writes.len(),
            expected.len(),
            "Unknown halts this repair action"
        );
        let event = out
            .iter()
            .find_map(|action| match action {
                Action::Notice(Notice::Event(event)) if event.code == "backpack.repair.unknown" => {
                    Some(event)
                }
                _ => None,
            })
            .ok_or("missing settled repair journal telemetry")?;
        assert!(event.message.contains(ENGINE_UNKNOWN_TEXT));
        let prefix_event = transport
            .events
            .iter()
            .find(|event| {
                event.code == "backpack.repair.placed"
                    && event.fields.iter().any(|(key, value)| {
                        *key == "orderId" && value.as_str() == Some("prefix-placed")
                    })
            })
            .ok_or("missing accepted repair prefix")?;
        assert_eq!(
            event
                .fields
                .iter()
                .find(|(key, _)| *key == "actionGeneration")
                .map(|(_, value)| value),
            prefix_event
                .fields
                .iter()
                .find(|(key, _)| *key == "actionGeneration")
                .map(|(_, value)| value),
        );
        assert!(
            event
                .fields
                .iter()
                .any(|(key, value)| *key == "cancelOutcome"
                    && value.as_str() == Some(if fail_cancel { "Unknown" } else { "Accepted" }))
        );
        assert!(
            event
                .fields
                .iter()
                .any(|(key, value)| *key == "placeOutcome"
                    && value.as_str() == Some(if fail_cancel { "NotSent" } else { "Unknown" }))
        );
        assert!(
            event
                .fields
                .iter()
                .any(|(key, value)| *key == "targetOrderId" && value.as_str() == Some("123"))
        );
        transport.poll(&mut engine);
        transport.assert_sequence(&expected)?;
    }
    Ok(())
}
