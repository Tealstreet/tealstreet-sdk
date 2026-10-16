#[path = "support/ws.rs"]
mod support;
use safe_cex_exchange::{runtime::ExchangeError, websocket::BaseWebSocket};
use safe_cex_venue_okx::logged_socket::{business_ping, private_ping};
use serde_json::{Value, json};
use std::rc::Rc;
use support::*;

#[test]
fn private_and_business_ping_native_before_wire() {
    let mut native = Vec::new();
    for (role, ping) in [
        ("private", private_ping as fn(&Rc<BaseWebSocket>) -> _),
        ("business", business_ping),
    ] {
        for time in [0.0_f64, -0.0, 1.25, 123.0, 2000.5] {
            for disposed in [false, true] {
                for outcome in ["success", "send-error", "closed-log-error"] {
                    let runtime = Rc::new(Runtime::default());
                    let parent = Parent::new();
                    let socket =
                        BaseWebSocket::new(1, parent.clone(), runtime.clone(), Rc::new(Protocol));
                    if outcome != "closed-log-error" {
                        runtime.io.ready.set(1);
                        ready(socket.create_websocket("wss://fixture".into(), vec![], None))
                            .unwrap();
                    }
                    runtime.monotonic_time.set(Some(777.0));
                    socket.mark_ping_sent();
                    runtime.monotonic_time.set(Some(time));
                    runtime.io.effects.borrow_mut().clear();
                    parent.disposed.set(disposed);
                    if outcome == "send-error" {
                        runtime.io.fail_send_at.set(Some(0));
                    }
                    let mut listener_error = ExchangeError::new("log listener failure");
                    listener_error.name = "TypeError".into();
                    listener_error.store_subscriber_ids = vec![7, 3];
                    if outcome == "closed-log-error" {
                        *parent.log_error.borrow_mut() = Some(listener_error.clone());
                        *parent.log_io.borrow_mut() = Some(runtime.io.clone());
                    }
                    let result = ping(&socket);
                    assert_eq!(
                        runtime.io.ready.get(),
                        u8::from(outcome != "closed-log-error"),
                        "OKX-LOGGED {role} native socket remains open or absent"
                    );
                    let expected_effects = if disposed {
                        vec![]
                    } else if outcome == "closed-log-error" {
                        vec!["clock", "log-listener"]
                    } else {
                        vec!["clock", "send:ping"]
                    };
                    assert_eq!(
                        *runtime.io.effects.borrow(),
                        expected_effects,
                        "OKX-LOGGED {role} native clock before send {time:?}/{disposed}/{outcome}"
                    );
                    let expected_error = if disposed || outcome == "success" {
                        None
                    } else if outcome == "send-error" {
                        Some(ExchangeError::new("send failure"))
                    } else {
                        Some(listener_error)
                    };
                    assert_eq!(
                        result.as_ref().err(),
                        expected_error.as_ref(),
                        "OKX-LOGGED {role} native complete error"
                    );
                    let sent = if disposed || outcome == "closed-log-error" {
                        vec![]
                    } else {
                        vec!["ping"]
                    };
                    assert_eq!(
                        *runtime.io.sent.borrow(),
                        sent,
                        "OKX-LOGGED {role} native payload"
                    );
                    let row = json!({"role":role, "timeBits":format!("{:016x}",time.to_bits()), "disposed":disposed, "outcome":outcome,
                        "effects":*runtime.io.effects.borrow(), "error":result.err().map(|e| json!({"name":e.name,"message":e.message}))});
                    let retained = if disposed { 777.0 } else { time };
                    runtime.monotonic_time.set(Some(retained + 20.0));
                    socket.handle_pong_event(5000).unwrap();
                    let latencies = if retained == 0.0 { vec![] } else { vec![10.0] };
                    assert_eq!(
                        *parent.latencies.borrow(),
                        latencies,
                        "OKX-LOGGED {role} native retained timestamp"
                    );
                    assert_eq!(
                        runtime.dials.borrow().len(),
                        usize::from(outcome != "closed-log-error"),
                        "OKX-LOGGED {role} native dial count"
                    );
                    runtime.pump();
                    runtime.discard_tasks();
                    native.push(row);
                }
            }
        }
    }
    assert_eq!(native.len(), 60, "declared profile floor");
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/logged-ping-oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(source.len(), 60, "source profile floor");
    assert_eq!(
        native, source,
        "live private and business methods after native assertions"
    );
}
