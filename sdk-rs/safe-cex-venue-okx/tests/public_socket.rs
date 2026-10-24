#[path = "support/cardinality.rs"]
mod cardinality;
#[path = "support/ws.rs"]
mod support;
use safe_cex_exchange::{
    runtime::ExchangeError,
    websocket::{BaseWebSocket, HandlerList, MessageHandler},
};
use safe_cex_num::{JsJsonValue, js_json_stringify};
use safe_cex_transport::socket::SocketFrame;
use safe_cex_venue_okx::public_socket::on_message;
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    rc::Rc,
};
use support::*;
#[test]
#[ignore = "requires isolated TypeScript compiler; mandatory venue gate"]
fn maintained_typescript_public_dispatch_and_sequence_recovery() {
    let oracle = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/public-socket-oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        oracle.status.success(),
        "{}",
        String::from_utf8_lossy(&oracle.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&oracle.stdout).unwrap();
    assert_eq!(cases.len(), 144);
    cardinality::assert_groups(
        &cases,
        "public_socket profiles",
        |case| {
            case["input"]["frames"]
                .as_array()
                .unwrap()
                .len()
                .to_string()
        },
        &[("5", 1), ("1", 143)],
    );
    let mut pong_assertions = 0;
    for (index, case) in cases.iter().enumerate() {
        let input = &case["input"];
        let runtime = Rc::new(Runtime::default());
        let parent = Parent::new();
        parent.disposed.set(input["disposed"] == true);
        runtime.io.ready.set(1);
        runtime
            .io
            .fail_send_at
            .set(input["failSendAt"].as_u64().map(|n| n as usize));
        let socket = BaseWebSocket::new(1, parent.clone(), runtime.clone(), Rc::new(Protocol));
        if input["connected"] != false && !parent.disposed.get() {
            ready(socket.create_websocket(
                "wss://ws.okx.com/ws/v5/public".into(),
                vec![],
                None,
            ))
            .unwrap();
        }
        let sequences = Rc::new(RefCell::new(
            input
                .get("sequences")
                .and_then(Value::as_array)
                .map(|rows| {
                    rows.iter()
                        .map(|row| {
                            (
                                row[0].as_str().unwrap().into(),
                                row[1].as_str().unwrap().into(),
                            )
                        })
                        .collect()
                })
                .unwrap_or_else(HashMap::<String, String>::new),
        ));
        let recovering = Rc::new(RefCell::new(
            input
                .get("recovering")
                .and_then(Value::as_array)
                .map(|rows| {
                    rows.iter()
                        .map(|row| row.as_str().unwrap().into())
                        .collect()
                })
                .unwrap_or_else(HashSet::<String>::new),
        ));
        let topics = Rc::new(RefCell::new(
            input
                .get("topics")
                .and_then(Value::as_object)
                .map(|object| {
                    object
                        .iter()
                        .map(|(key, topics)| {
                            (
                                key.clone(),
                                topics
                                    .as_array()
                                    .unwrap()
                                    .iter()
                                    .map(|topic| {
                                        (
                                            topic["channel"].as_str().unwrap().into(),
                                            topic["instId"].as_str().unwrap().into(),
                                        )
                                    })
                                    .collect(),
                            )
                        })
                        .collect()
                })
                .unwrap_or_else(HashMap::<String, Vec<(String, String)>>::new),
        ));
        let calls = Rc::new(RefCell::new(Vec::<Value>::new()));
        let mut keys = vec![
            "tickers",
            "mark-price",
            "index-tickers",
            "open-interest",
            "funding-rate",
        ];
        if let Some(handlers) = input["handlers"].as_array() {
            keys.extend(handlers.iter().map(|key| key.as_str().unwrap()));
        }
        for key in keys {
            let callbacks: HandlerList = Rc::default();
            for callback_index in 0..input["count"].as_u64().unwrap_or(1) {
                let weak = Rc::downgrade(&socket);
                let key = key.to_owned();
                let calls = calls.clone();
                let remove = input["removeKey"] == true;
                let throwing = input["throwFirst"] == true;
                let callback: MessageHandler = Rc::new(move |data| {
                    let socket = weak.upgrade().unwrap();
                    let (count, had) = {
                        let stats = socket.message_handlers_stats.borrow();
                        let stat = stats.get(&key).expect("stats must precede callback");
                        assert_eq!(stat.last_message_timestamp, 123.);
                        (stat.last_callbacks_count.clone(), stat.had_handlers)
                    };
                    calls.borrow_mut().push(json!({"key":key,"index":callback_index,"data":js_json_stringify(&JsJsonValue::from(safe_cex_exchange::js_runtime::value::to_json(data).unwrap())).unwrap().unwrap(),"stats":{"count":count,"had":had}}));
                    if remove && callback_index == 0 {
                        socket.message_handlers.borrow_mut().shift_remove(&key);
                    }
                    if throwing && callback_index == 0 {
                        let mut error = ExchangeError::new("callback failed");
                        error.name = "TypeError".into();
                        return Err(error);
                    }
                    Ok(())
                });
                callbacks.borrow_mut().push(callback);
            }
            socket
                .message_handlers
                .borrow_mut()
                .insert(key.into(), callbacks);
        }
        if let Some(frame) = input["reenter"].as_str() {
            let frame = frame.to_owned();
            let weak = Rc::downgrade(&socket);
            let sequences = sequences.clone();
            let recovering = recovering.clone();
            let topics = topics.clone();
            *runtime.io.reenter.borrow_mut() = Some(Box::new(move || {
                on_message(
                    &weak.upgrade().unwrap(),
                    &SocketFrame::Text(frame.clone()),
                    &sequences,
                    &recovering,
                    &topics,
                )
            }));
        }
        let results: Vec<Value> = input["frames"]
            .as_array()
            .unwrap()
            .iter()
            .map(|frame| {
                match on_message(
                    &socket,
                    &SocketFrame::Text(frame.as_str().unwrap().into()),
                    &sequences,
                    &recovering,
                    &topics,
                ) {
                    Ok(()) => Value::Null,
                    Err(error) => json!({"name":error.name,"message":error.message}),
                }
            })
            .collect();
        runtime.pump();
        assert_eq!(json!(results), case["results"], "frame result {index}");
        assert_eq!(
            json!(*calls.borrow()),
            case["calls"],
            "matched-key callbacks/data/stats {index}"
        );
        assert_eq!(
            json!(*runtime.io.sent.borrow()),
            case["sent"],
            "recovery frames {index}"
        );
        let stats: Value = socket
            .message_handlers_stats
            .borrow()
            .iter()
            .map(|(key, stat)| {
                (
                    key.clone(),
                    json!({"count":stat.last_callbacks_count,"had":stat.had_handlers}),
                )
            })
            .collect::<serde_json::Map<_, _>>()
            .into();
        assert_eq!(stats, case["stats"], "matched-key stats {index}");
        assert_eq!(
            socket.last_message_timestamp().is_some(),
            case["tracked"].as_u64().unwrap() > 0,
            "receive tracking {index}"
        );
        let mut actual: Vec<_> = sequences
            .borrow()
            .iter()
            .map(|(k, v)| json!([k, v]))
            .collect();
        actual.sort_by_key(Value::to_string);
        let mut expected = case["sequences"].as_array().unwrap().clone();
        expected.sort_by_key(Value::to_string);
        assert_eq!(actual, expected, "sequence state {index}");
        let mut actual: Vec<_> = recovering.borrow().iter().cloned().collect();
        actual.sort();
        let mut expected: Vec<_> = case["recovering"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_owned())
            .collect();
        expected.sort();
        assert_eq!(actual, expected, "recovering state {index}");
        if input["frames"]
            .as_array()
            .unwrap()
            .iter()
            .any(|frame| frame == "pong")
            && !parent.disposed.get()
        {
            assert!(
                runtime.sleeps.borrow().contains(&5000),
                "source default pong delay"
            );
            pong_assertions += 1;
        }
        socket.dispose();
        runtime.discard_tasks();
        let weak = Rc::downgrade(&socket);
        drop(socket);
        assert!(
            weak.upgrade().is_none(),
            "no handler/socket/task cycle {index}"
        );
    }
    assert_eq!(pong_assertions, 1, "PONG-BRANCH assertion executions");
    for (mutation, expected) in [
        ("--mutate-gap-guard", "sequence gap"),
        (
            "--mutate-prototype",
            "source positive-arity callback defect",
        ),
    ] {
        let child = std::process::Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/public-socket-oracle.mjs"
            ))
            .arg(mutation)
            .output()
            .unwrap();
        assert_eq!(child.status.code(), Some(1));
        assert!(
            String::from_utf8_lossy(&child.stderr).contains(expected),
            "control failed for wrong reason: {}",
            String::from_utf8_lossy(&child.stderr)
        );
    }
}

#[test]
fn own_empty_handler_shadows_positive_arity_prototype() {
    let runtime = Rc::new(Runtime::default());
    let socket = BaseWebSocket::new(1, Parent::new(), runtime.clone(), Rc::new(Protocol));
    socket
        .message_handlers
        .borrow_mut()
        .insert("constructor".into(), Rc::default());
    safe_cex_venue_okx::public_socket::dispatch_message_handler(
        &socket,
        "constructor",
        &safe_cex_exchange::js_runtime::value::from_json(&json!({})),
        &RefCell::new(HashMap::new()),
        &RefCell::new(HashSet::new()),
        &|_| Ok(()),
    )
    .unwrap();
    assert!(
        socket.message_handlers_stats.borrow().is_empty(),
        "own empty list must return before inherited stats/error"
    );
    socket.dispose();
    runtime.discard_tasks();
}

#[test]
fn inherited_error_records_core_stats_before_returning() {
    let runtime = Rc::new(Runtime::default());
    let socket = BaseWebSocket::new(1, Parent::new(), runtime.clone(), Rc::new(Protocol));
    // An existing own stats record is observable without mutating a global referent.
    socket
        .message_handlers_stats
        .borrow_mut()
        .insert("constructor".into(), Default::default());
    let error = safe_cex_venue_okx::public_socket::dispatch_message_handler(
        &socket,
        "constructor",
        &safe_cex_exchange::js_runtime::value::from_json(&json!({})),
        &RefCell::new(HashMap::new()),
        &RefCell::new(HashSet::new()),
        &|_| Ok(()),
    )
    .unwrap_err();
    assert_eq!(error.name, "TypeError");
    assert_eq!(error.message, "callbacks is not iterable");
    let stats = socket.message_handlers_stats.borrow();
    let entry = stats.get("constructor").unwrap();
    assert_eq!(
        entry.last_callbacks_count,
        safe_cex_num::PrototypeScalar::from(Some(1)),
        "core inherited arity must be recorded before TypeError"
    );
    assert_eq!(entry.had_handlers, Some(true));
    drop(stats);
    socket.dispose();
    runtime.discard_tasks();
}

#[test]
fn incoming_health_publication_errors_stop_frame_processing() {
    for pong in [false, true] {
        let runtime = Rc::new(Runtime::default());
        let parent = Parent::new();
        let mut expected = ExchangeError::new("health subscriber failed");
        expected.name = "TypeError".into();
        expected = expected.with_retained_response(json!({"cause":"retained"}).into());
        let socket = BaseWebSocket::new(1, parent.clone(), runtime.clone(), Rc::new(Protocol));
        if pong {
            socket.track_last_received().unwrap();
            socket.mark_ping_sent();
            *parent.latency_error.borrow_mut() = Some(expected.clone());
        } else {
            *parent.health_error.borrow_mut() = Some(expected.clone());
        }
        let frame = SocketFrame::Text(if pong {
            "pong".into()
        } else {
            "invalid json must not be parsed".into()
        });
        let result = on_message(
            &socket,
            &frame,
            &RefCell::default(),
            &RefCell::default(),
            &RefCell::default(),
        );
        assert_eq!(
            result.unwrap_err(),
            expected,
            "incoming health error identity {pong}"
        );
    }
}

#[test]
fn retained_handler_frames_cross_the_production_core_registry() {
    let runtime = Rc::new(Runtime::default());
    let parent = Parent::new();
    let socket = BaseWebSocket::new(1, parent, runtime, Rc::new(Protocol));
    let calls = Rc::new(RefCell::new(0));
    let seen = calls.clone();
    let callback: MessageHandler = Rc::new(move |_| {
        *seen.borrow_mut() += 1;
        Ok(())
    });
    socket
        .message_handlers
        .borrow_mut()
        .insert("tickers".into(), Rc::new(RefCell::new(vec![callback])));
    let sequences = RefCell::new(HashMap::new());
    let recovering = RefCell::new(HashSet::new());
    let topics = RefCell::new(HashMap::new());
    for marker in [r"\ud800", r"\udfff", r"\ud83e\udd80"] {
        let frame = format!(
            r#"{{"arg":{{"channel":"tickers"}},"unused":{{"{marker}":["{marker}"]}},"overflow":1e400,"zero":-0}}"#
        );
        on_message(
            &socket,
            &SocketFrame::Text(frame),
            &sequences,
            &recovering,
            &topics,
        )
        .expect("source-valid native graph reaches OKX dispatch");
    }
    on_message(
        &socket,
        &SocketFrame::Text(r#"{"arg":{"channel":"tickers"},"neighbor":true}"#.into()),
        &sequences,
        &recovering,
        &topics,
    )
    .unwrap();
    assert_eq!(
        *calls.borrow(),
        4,
        "all valid frames reach the shared registry"
    );
    socket.dispose();
}

#[cfg(test)]
mod retained_handler_support {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../safe-cex-exchange/tests/support/retained_handlers.rs"
    ));
}
#[test]
fn shared_core_handlers_retain_the_source_graph_and_native_fields() {
    let socket = BaseWebSocket::new(
        1,
        Parent::new(),
        Rc::new(Runtime::default()),
        Rc::new(Protocol),
    );
    let cases = retained_handler_support::source_frames("okx", "publicWebsocket");
    let (handlers, calls) = retained_handler_support::pair(&cases, false);
    socket
        .message_handlers
        .borrow_mut()
        .insert("retainedProbe".into(), handlers);
    let sequences = RefCell::new(HashMap::new());
    let recovering = RefCell::new(HashSet::new());
    let topics = RefCell::new(HashMap::new());
    for case in cases {
        on_message(
            &socket,
            &SocketFrame::Text(case["frame"].as_str().unwrap().into()),
            &sequences,
            &recovering,
            &topics,
        )
        .unwrap();
    }
    assert_eq!(calls.get(), 8);
    socket.dispose();
}
