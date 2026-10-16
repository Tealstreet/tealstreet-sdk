use super::*;

pub(super) fn pump(runtime: &Runtime) {
    let tasks = runtime.tasks.borrow().clone();
    for task in tasks {
        let future = task.borrow_mut().take();
        if let Some(mut future) = future
            && future
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop()))
                .is_pending()
        {
            *task.borrow_mut() = Some(future);
        }
    }
}

#[test]
#[ignore = "requires frozen workspace Node dependencies; mandatory venue-oracle:bybit CI step"]
fn owned_reconnect_sequence_matches_source_branches_and_synchronous_prefix() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/reconnect-fixtures.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(cases.len(), 12);
    for inline in [false, true] {
        for case in &cases {
            assert_eq!(
                case["authAfterDispose"], true,
                "source cancellation divergence witness"
            );
            let (socket, parent, runtime) = fixture();
            runtime.inline.set(inline);
            runtime.sleeps.borrow_mut().clear();
            let trace = Rc::new(RefCell::new(Vec::<String>::new()));
            let gate = Rc::new(Cell::new(false));
            let sync_trace = trace.clone();
            let sync_gate = gate.clone();
            let auth_trace = trace.clone();
            let auth_runtime = runtime.clone();
            let result = case["result"].as_str().unwrap().to_owned();
            let disposed = case["disposed"].as_bool().unwrap();
            if disposed {
                parent.disposed.set(true);
                socket.dispose();
            }
            // One more silent connection would advance the chain without disregardSilence.
            socket
                .direct_fallback
                .borrow_mut()
                .resolve_url("direct", &[Some("fallback")]);
            socket
                .direct_fallback
                .borrow_mut()
                .resolve_url("direct", &[Some("fallback")]);
            crate::ws_private::send_reconnect_sequence(
                &socket,
                runtime.clone(),
                move || {
                    sync_trace.borrow_mut().push("sync".into());
                    Box::pin(std::future::poll_fn(move |_| {
                        if sync_gate.get() {
                            Poll::Ready(Ok(()))
                        } else {
                            Poll::Pending
                        }
                    }))
                },
                move || {
                    assert_eq!(
                        *auth_runtime.sleeps.borrow(),
                        [25],
                        "sleep must precede auth"
                    );
                    auth_trace.borrow_mut().push("auth".into());
                    Box::pin(async move {
                        match result.as_str() {
                            "yes" => Ok(true),
                            "no" => Ok(false),
                            _ => Err(ExchangeError::new("signer rejected")),
                        }
                    })
                },
                case["reconnecting"].as_bool().unwrap(),
                true,
            );
            assert_eq!(
                json!(*trace.borrow()),
                case["prefix"],
                "synchronous prefix {case} inline={inline}"
            );
            pump(&runtime);
            assert!(
                runtime.wire.0.borrow().is_empty(),
                "pending clock must send nothing"
            );
            gate.set(true);
            pump(&runtime);
            if disposed {
                assert!(runtime.sleeps.borrow().is_empty());
                continue;
            }
            assert_eq!(*trace.borrow(), ["sync", "auth"]);
            assert!(runtime.sleeps.borrow().contains(&25));
            let success = case["trace"]
                .as_array()
                .unwrap()
                .contains(&json!("subscribe"));
            assert_eq!(
                runtime.wire.1.get(),
                usize::from(!success),
                "auth refusal must initiate real native reconnect"
            );
            let wire = runtime.wire.0.borrow();
            if success {
                assert_eq!(
                    *wire,
                    [
                        r#"{"op":"subscribe","args":["order","position"]}"#,
                        r#"{"op":"ping"}"#
                    ]
                );
            } else {
                assert!(
                    wire.is_empty(),
                    "auth refusal must send neither subscribe nor ping"
                );
                assert_eq!(
                    socket
                        .direct_fallback
                        .borrow_mut()
                        .resolve_url("direct", &[Some("fallback")]),
                    "direct",
                    "own silence must not advance fallback"
                );
            }
            let errors: Vec<String> = parent
                .events
                .borrow()
                .iter()
                .filter(|(kind, _)| kind == "error")
                .map(|(_, message)| format!("error:{message}"))
                .collect();
            let source_errors: Vec<String> = case["trace"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(|v| {
                    v.as_str()
                        .filter(|s| s.starts_with("error:"))
                        .map(str::to_owned)
                })
                .collect();
            assert_eq!(errors, source_errors);
            assert_eq!(
                parent
                    .events
                    .borrow()
                    .iter()
                    .any(|(kind, _)| kind == "snapshot"),
                case["trace"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("snapshot"))
            );
        }
    }
}

#[test]
fn reconnect_sequence_disposal_cancels_unpolled_pending_and_inline_pending_tail() {
    for mode in ["unpolled", "pending", "inline-pending"] {
        let (socket, _, runtime) = fixture();
        runtime.inline.set(mode == "inline-pending");
        let auth_calls = Rc::new(Cell::new(0));
        let calls = auth_calls.clone();
        crate::ws_private::send_reconnect_sequence(
            &socket,
            runtime.clone(),
            || Box::pin(std::future::pending()),
            move || {
                calls.set(calls.get() + 1);
                Box::pin(async { Ok(true) })
            },
            false,
            false,
        );
        if mode == "pending" {
            pump(&runtime);
        }
        let weak = Rc::downgrade(&socket);
        socket.dispose();
        drop(socket);
        pump(&runtime);
        assert!(
            weak.upgrade().is_none(),
            "core-owned reauth must release socket: {mode}"
        );
        assert_eq!(auth_calls.get(), 0);
        assert!(runtime.wire.0.borrow().is_empty());
    }
}
