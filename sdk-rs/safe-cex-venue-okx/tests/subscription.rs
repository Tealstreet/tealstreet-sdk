#[path = "support/cardinality.rs"]
mod cardinality;
#[path = "support/ws.rs"]
mod support;
use safe_cex_exchange::websocket::BaseWebSocket;
use safe_cex_venue_okx::public_socket::subscribe;
use serde_json::Value;
use std::rc::Rc;
use support::*;
#[test]
#[ignore = "requires isolated TypeScript compiler; mandatory venue gate"]
fn maintained_source_subscription_batches_order_and_send_failure() {
    let script = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/subscription-oracle.mjs");
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
    assert_eq!(cases.len(), 35);
    cardinality::assert_groups(
        &cases,
        "subscription profiles",
        |case| case["sent"].as_array().unwrap().len().to_string(),
        &[("0", 3), ("1", 22), ("2", 8), ("3", 1), ("16", 1)],
    );
    for (index, case) in cases.iter().enumerate() {
        let runtime = Rc::new(Runtime::default());
        runtime.io.ready.set(1);
        runtime
            .io
            .fail_send_at
            .set(case["failSendAt"].as_u64().map(|n| n as usize));
        let socket = BaseWebSocket::new(1, Parent::new(), runtime.clone(), Rc::new(Protocol));
        ready(socket.create_websocket("wss://ws.okx.com/ws/v5/public".into(), vec![], None))
            .unwrap();
        let result = subscribe(&socket, case["topics"].as_object().unwrap());
        assert_eq!(
            serde_json::json!(*runtime.io.sent.borrow()),
            case["sent"],
            "subscription wire/order/count {index}"
        );
        match result {
            Ok(()) => assert!(
                case["error"].is_null(),
                "subscription expected error {index}"
            ),
            Err(e) => {
                assert_eq!(
                    e.name, case["error"]["name"],
                    "subscription error name {index}"
                );
                assert_eq!(
                    e.message, case["error"]["message"],
                    "subscription error message {index}"
                );
            }
        }
        runtime.pump();
        runtime.discard_tasks();
    }
    let alias = std::process::Command::new("node")
        .arg(script)
        .arg("--alias-witness")
        .output()
        .unwrap();
    assert!(
        alias.status.success(),
        "{}",
        String::from_utf8_lossy(&alias.stderr)
    );
    let alias: Value = serde_json::from_slice(&alias.stdout).unwrap();
    let runtime = Rc::new(Runtime::default());
    runtime.io.ready.set(1);
    let socket = BaseWebSocket::new(1, Parent::new(), runtime.clone(), Rc::new(Protocol));
    ready(socket.create_websocket("wss://ws.okx.com/ws/v5/public".into(), vec![], None))
        .unwrap();
    subscribe(&socket, alias["before"].as_object().unwrap()).unwrap();
    let source_later: Value = serde_json::from_str(alias["sent"][1].as_str().unwrap()).unwrap();
    let rust_later: Value = serde_json::from_str(&runtime.io.sent.borrow()[1]).unwrap();
    assert_eq!(
        source_later["args"][0]["instId"], "MUTATED-AFTER-FIRST-SEND",
        "source later frame observes alias mutation"
    );
    assert_eq!(
        rust_later["args"][0]["instId"], "BTC-100",
        "Rust immutable JSON receiver keeps original snapshot"
    );
    runtime.pump();
    runtime.discard_tasks();
    let witness = std::process::Command::new("node")
        .arg(script)
        .arg("--surrogate-witness")
        .output()
        .unwrap();
    assert!(
        witness.status.success(),
        "{}",
        String::from_utf8_lossy(&witness.stderr)
    );
    let witness: Value = serde_json::from_slice(&witness.stdout).unwrap();
    assert_eq!(
        witness["wire"], r#"{"op":"subscribe","args":[{"instId":"\ud800"}]}"#,
        "source preserves lone surrogate in subscription wire"
    );
    assert!(
        serde_json::from_str::<serde_json::Map<String, Value>>(
            r#"{"topic":[{"instId":"\ud800"}]}"#
        )
        .is_err(),
        "Rust UTF8 ingress refuses rather than substitutes source lone surrogate"
    );
    for (flag, expected) in [
        ("--mutate-batch-size", "subscription batch count"),
        ("--mutate-missing-package", "MODULE_NOT_FOUND"),
    ] {
        let output = std::process::Command::new("node")
            .arg(script)
            .arg(flag)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(1));
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(expected),
            "source control {flag}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
