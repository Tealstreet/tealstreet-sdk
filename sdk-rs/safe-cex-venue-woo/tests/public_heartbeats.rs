#[path = "support/ws.rs"]
mod support;

use safe_cex_exchange::{runtime::LogSeverity, websocket::BaseWebSocket};
use safe_cex_venue_woo::public_socket::{handle_ping_event, ping};
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
fn public_heartbeats_match_source_through_core_send() {
    let cases: Vec<Value> =
        serde_json::from_str(include_str!("fixtures/public-heartbeats.json")).unwrap();
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
        let result = if input["method"] == "ping" {
            ping(&socket)
        } else {
            handle_ping_event(&socket)
        };
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
fn pong_propagates_parent_latency_failure() {
    let runtime = Rc::new(Runtime::default());
    let parent = Parent::new();
    let socket = BaseWebSocket::new(1, parent.clone(), runtime.clone(), Rc::new(Protocol));
    let mut cause = safe_cex_exchange::runtime::ExchangeError::new("latency listener failed");
    cause.name = "LatencyNotificationError".into();
    cause.code = Some(safe_cex_exchange::js_runtime::value::Js::Number(732.0));
    *parent.hook_failure.borrow_mut() = Some(("latency", cause.clone()));
    runtime.monotonic.set(7.0);
    socket.mark_ping_sent();
    runtime.monotonic.set(23.0);
    assert_eq!(
        socket.handle_pong_event(5000),
        Err(cause),
        "pong retains latency failure identity"
    );
    assert_eq!(*parent.latencies.borrow(), vec![8.0]);
    runtime.discard_tasks();
}
