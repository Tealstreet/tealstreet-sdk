#[path = "support/cardinality.rs"]
mod cardinality;
#[path = "support/ws.rs"]
mod support;
use safe_cex_exchange::websocket::BaseWebSocket;
use safe_cex_venue_okx::public_socket::ping;
use serde_json::Value;
use std::rc::Rc;
use support::*;

#[test]
#[ignore = "requires isolated TypeScript compiler; mandatory venue gate"]
fn maintained_ping_clock_before_send_and_disposal() {
    let script = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/ping-oracle.mjs");
    let output = std::process::Command::new("node")
        .arg(script)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(cases.len(), 20);
    cardinality::assert_groups(
        &cases,
        "ping profiles",
        |case| format!("{}:{}", case["disposed"], case["fail"]),
        &[
            ("false:false", 5),
            ("false:true", 5),
            ("true:false", 5),
            ("true:true", 5),
        ],
    );
    for case in cases {
        let runtime = Rc::new(Runtime::default());
        runtime.io.ready.set(1);
        let parent = Parent::new();
        let socket = BaseWebSocket::new(1, parent.clone(), runtime.clone(), Rc::new(Protocol));
        ready(socket.create_websocket("wss://ws.okx.com/ws/v5/public".into(), vec![], None))
            .unwrap();
        let time =
            f64::from_bits(u64::from_str_radix(case["timeBits"].as_str().unwrap(), 16).unwrap());
        runtime.monotonic_time.set(Some(time));
        runtime.io.effects.borrow_mut().clear();
        runtime
            .io
            .fail_send_at
            .set((case["fail"] == true).then_some(0));
        parent.disposed.set(case["disposed"] == true);
        let result = ping(&socket);
        assert_eq!(
            serde_json::json!(*runtime.io.effects.borrow()),
            case["effects"],
            "ping clock before send effects"
        );
        if case["error"].is_null() {
            result.unwrap();
        } else {
            let error = result.unwrap_err();
            assert_eq!(
                error.name, case["error"]["name"],
                "ping send error name identity"
            );
            assert_eq!(
                error.message, case["error"]["message"],
                "ping send error identity"
            );
        }
        if !parent.disposed.get() {
            runtime.monotonic_time.set(Some(time + 20.0));
            socket.handle_pong_event(5000).unwrap();
            let expected: Vec<f64> = if time == 0.0 { vec![] } else { vec![10.0] };
            assert_eq!(
                *parent.latencies.borrow(),
                expected,
                "ping timestamp remains committed before failed send"
            );
        }
        runtime.pump();
        runtime.discard_tasks();
    }
    let source = std::process::Command::new("node")
        .arg(script)
        .arg("--closed-log-witness")
        .output()
        .unwrap();
    assert!(
        source.status.success(),
        "{}",
        String::from_utf8_lossy(&source.stderr)
    );
    let source: Value = serde_json::from_slice(&source.stdout).unwrap();
    assert_eq!(
        source["error"]["name"], "TypeError",
        "source skipped heartbeat listener error"
    );
    let runtime = Rc::new(Runtime::default());
    let parent = Parent::new();
    let mut expected = safe_cex_exchange::runtime::ExchangeError::new("log listener failure");
    expected.name = "TypeError".into();
    expected.store_subscriber_ids = vec![7, 3];
    *parent.log_error.borrow_mut() = Some(expected.clone());
    *parent.log_io.borrow_mut() = Some(runtime.io.clone());
    let socket = BaseWebSocket::new(1, parent.clone(), runtime.clone(), Rc::new(Protocol));
    let error = ping(&socket).unwrap_err();
    assert_eq!(
        error, expected,
        "skipped heartbeat retains listener error and subscriber identities"
    );
    assert_eq!(error.name, source["error"]["name"]);
    assert_eq!(error.message, source["error"]["message"]);
    assert_eq!(
        serde_json::json!(*runtime.io.effects.borrow()),
        source["effects"],
        "skipped heartbeat clock before listener error"
    );
    assert!(runtime.io.sent.borrow().is_empty());
    assert!(
        runtime.dials.borrow().is_empty(),
        "heartbeat must not reconnect"
    );
    runtime.pump();
    runtime.discard_tasks();
    let control = std::process::Command::new("node")
        .arg(script)
        .arg("--mutate-send-first")
        .output()
        .unwrap();
    assert_eq!(control.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&control.stderr).contains("ping clock before send effects"));
}
