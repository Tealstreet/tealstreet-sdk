#[path = "support/ws.rs"]
mod support;

use safe_cex_exchange::{runtime::LogSeverity, websocket::BaseWebSocket};
use safe_cex_venue_woo::private_socket::ping;
use serde_json::{Value, json};
use std::{
    future::Future,
    rc::Rc,
    task::{Context, Poll, Waker},
};
use support::*;

fn ready<F: Future>(future: F) -> F::Output {
    match std::pin::pin!(future)
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("heartbeat fixture dial unexpectedly pending"),
    }
}

#[test]
fn private_ping_matches_source_through_core_send_with_log() {
    let cases: Vec<Value> =
        serde_json::from_str(include_str!("fixtures/private-heartbeats.json")).unwrap();
    assert_eq!(cases.len(), 40);
    for case in cases {
        let input = &case["input"];
        let runtime = Rc::new(Runtime::default());
        let parent = Parent::new();
        let socket = BaseWebSocket::new(1, parent.clone(), runtime.clone(), Rc::new(Protocol));
        if let Some(state) = input["ready"].as_u64() {
            runtime.io.ready.set(state as u8);
            ready(socket.create_websocket("wss://example.test/ws".into(), vec![], None)).unwrap();
        }
        runtime.monotonic.set(7.0);
        socket.mark_ping_sent();
        runtime.monotonic.set(123.0);
        parent.disposed.set(input["disposed"] == true);
        runtime
            .io
            .fail_send_at
            .set((input["fail"] == true).then_some(0));
        runtime.diagnostics.set(input["diagnostics"] == true);
        runtime.diagnostic_checks.set(0);
        let result = ping(&socket);
        assert_eq!(
            json!(runtime.diagnostic_checks.get()),
            case["diagnosticChecks"],
            "private diagnostic call order {input}"
        );
        let error = result
            .err()
            .map(|e| json!({"name":e.name,"message":e.message}));
        assert_eq!(json!(error), case["error"], "heartbeat error {input}");
        assert_eq!(
            json!(*runtime.io.sent.borrow()),
            case["sent"],
            "heartbeat payload {input}"
        );
        let logs: Vec<Value> = parent.logs.borrow().iter().map(|(message,severity)| json!({
            "message":message,
            "severity":match severity {LogSeverity::Debug=>"debug",LogSeverity::Warning=>"warning",_=>"unexpected"}
        })).collect();
        assert_eq!(json!(logs), case["logs"], "heartbeat logs {input}");
        assert_eq!(
            parent.health_syncs.get(),
            0,
            "heartbeat must not enter reconnect/notification {input}"
        );
        assert_eq!(case["reconnects"], 0);

        // Observe the private ping timestamp via core's public latency operation,
        // before disposing the fixture. This distinguishes the disposed guard
        // and timestamp-before-send ordering, even when the host send throws.
        parent.disposed.set(false);
        runtime.monotonic.set(223.0);
        socket
            .handle_pong_event(5000)
            .expect("fixture latency notification must succeed");
        let expected = ((223.0 - case["pingAt"].as_f64().unwrap()) / 2.0).round();
        assert_eq!(
            *parent.latencies.borrow(),
            vec![expected],
            "heartbeat ping timestamp {input}"
        );
        runtime.pump();
        runtime.discard_tasks();
    }
}

#[test]
fn private_timestamp_matches_source_number_coercion_and_floor_bits() {
    use safe_cex_venue_woo::private_socket::parse_timestamp_to_seconds;
    let cases: Vec<Value> =
        serde_json::from_str(include_str!("fixtures/private-timestamps.json")).unwrap();
    assert_eq!(cases.len(), 125);
    let mut native_nan = 0;
    for case in cases {
        let input = &case["input"];
        // Native NaN is outside the chosen JSON input surface, not a JSON null.
        if input.get("$number") == Some(&json!("NaN")) {
            assert_eq!(case["ok"], json!({"$undefined":true}));
            native_nan += 1;
            continue;
        }
        let raw = if input.get("$undefined") == Some(&json!(true)) {
            None
        } else {
            Some(match input.get("$number").and_then(Value::as_str) {
                Some("Infinity") => serde_json::from_str("1e400").unwrap(),
                Some("-Infinity") => serde_json::from_str("-1e400").unwrap(),
                Some("-0") => serde_json::from_str("-0").unwrap(),
                _ => input.clone(),
            })
        };
        let actual = parse_timestamp_to_seconds(raw.as_ref());
        if let Some(error) = case.get("error") {
            let actual = actual.unwrap_err();
            assert_eq!(actual.name, error["name"], "timestamp error name {input}");
            assert_eq!(
                actual.message, error["message"],
                "timestamp error message {input}"
            );
        } else if case["ok"].get("$undefined") == Some(&json!(true)) {
            assert_eq!(actual.unwrap(), None, "timestamp absence {input}");
        } else {
            assert_eq!(
                actual.unwrap().unwrap().to_bits(),
                case["ok"].as_f64().unwrap().to_bits(),
                "timestamp output bits {input}"
            );
        }
    }
    assert_eq!(native_nan, 1, "native NaN boundary stays explicit");
}
