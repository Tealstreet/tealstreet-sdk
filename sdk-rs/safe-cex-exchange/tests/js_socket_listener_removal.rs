#![cfg(all(feature = "native", not(target_arch = "wasm32")))]
use safe_cex_exchange::{
    capabilities::ExchangeCapabilities,
    exchange::{BaseExchange, Exchange, ExchangeSettings},
    js_runtime::{client, globals, scope::ScopeShared, sockets::base_websocket, value::Js},
    native::native_runtime,
};
use std::{
    cell::{Cell, RefCell},
    net::TcpListener,
    process::Command,
    rc::Rc,
    sync::Arc,
    time::Duration,
};
use tokio_tungstenite::tungstenite::{Message, accept};

// Only carries the real base through the object-safe socket parent boundary.
struct Owner(BaseExchange, ExchangeCapabilities);
impl Exchange for Owner {
    type PlaceOpts = safe_cex_types::PlaceOrderOpts;
    fn base(&self) -> &BaseExchange {
        &self.0
    }
    fn capabilities(&self) -> &ExchangeCapabilities {
        &self.1
    }
}

struct Server {
    release: std::sync::mpsc::Sender<()>,
    task: Option<std::thread::JoinHandle<()>>,
}
impl Drop for Server {
    fn drop(&mut self) {
        self.release.send(()).expect("release live websocket peer");
        self.task.take().unwrap().join().expect("websocket peer");
    }
}

#[test]
fn native_socket_listener_removal_matches_source() {
    let oracle = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/js_socket_listener_removal_oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        oracle.status.success(),
        "{}",
        String::from_utf8_lossy(&oracle.stderr)
    );
    let cases: Vec<serde_json::Value> = serde_json::from_slice(&oracle.stdout).unwrap();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    tokio::task::LocalSet::new().block_on(&rt, async {
        let mut failures = vec![];
        for case in cases {
            let event = case["event"].as_str().unwrap();
            let grouped = case["grouped"].as_bool().unwrap();
            let kind = case["kind"].as_str().unwrap().to_owned();
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let url = format!("ws://{}", listener.local_addr().unwrap());
            let (release, released) = std::sync::mpsc::channel();
            let frames = if event == "message" { 2 } else { 0 };
            let task = std::thread::spawn(move || {
                listener.set_nonblocking(true).unwrap();
                let deadline = std::time::Instant::now() + Duration::from_secs(5);
                let stream = loop {
                    match listener.accept() {
                        Ok((stream, _)) => break stream,
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            if released.try_recv().is_ok() {
                                return;
                            }
                            assert!(std::time::Instant::now() < deadline, "client did not dial");
                            std::thread::sleep(Duration::from_millis(5));
                        }
                        Err(error) => panic!("accept websocket: {error}"),
                    }
                };
                stream.set_nonblocking(false).unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(10)))
                    .unwrap();
                let mut ws = accept(stream).unwrap();
                for _ in 0..frames {
                    ws.send(Message::Text("private-frame".into())).unwrap();
                }
                released.recv_timeout(Duration::from_secs(10)).unwrap();
            });
            let _server = Server {
                release,
                task: Some(task),
            };
            let runtime = native_runtime();
            let host = client::rest_http_host(
                runtime.clone(),
                Arc::new(safe_cex_transport::native::reqwest_adapter().unwrap()),
            );
            let store = Rc::new(safe_cex_store::store::DefaultStore::new());
            let shared = Rc::new(ScopeShared::new(
                runtime.clone(),
                host,
                store.clone(),
                Js::object(vec![]),
                (),
            ));
            globals::install_builtins(&shared.globals, Rc::downgrade(&shared));
            let owner: Rc<dyn Exchange> = Rc::new(Owner(
                BaseExchange::new(
                    safe_cex_types::ExchangeName::Coinbase,
                    serde_json::from_value(serde_json::json!({"key":"", "secret":""})).unwrap(),
                    store,
                    runtime,
                    ExchangeSettings::default(),
                ),
                ExchangeCapabilities::default(),
            ));
            *shared.sockets.venue.borrow_mut() = Some(Rc::downgrade(&owner));
            let socket = base_websocket(&shared.sockets, Js::object(vec![]));
            let options = if grouped {
                Js::object(vec![(
                    "headers",
                    Js::object(vec![("X-Witness", Js::str("removal"))]),
                )])
            } else {
                Js::Undefined
            };
            let ws = socket
                .call_method(
                    "createWebsocket",
                    vec![Js::str(&url), Js::Undefined, options],
                )
                .unwrap();
            let trace = Rc::new(RefCell::new(Vec::<String>::new()));
            let seen = trace.clone();
            let second = Js::function("second", move |_, _| {
                seen.borrow_mut().push("second".into());
                Ok(Js::Undefined)
            });
            let unrelated = Js::function("unrelated", |_, _| Ok(Js::Undefined));
            let seen = trace.clone();
            let action = ws.clone();
            let callback = second.clone();
            let missing = unrelated.clone();
            let behavior = kind.clone();
            let event_name = event.to_owned();
            let delivered = Rc::new(tokio::sync::Notify::new());
            let first_done = delivered.clone();
            let count_first = Cell::new(0);
            let target = if event == "message" { 2 } else { 1 };
            let first_time = Cell::new(true);
            let first = Js::function("first", move |_, _| {
                seen.borrow_mut().push("first".into());
                count_first.set(count_first.get() + 1);
                if behavior == "remove-add" && count_first.get() == target {
                    first_done.notify_one();
                }
                if first_time.replace(false) {
                    if behavior == "remove" || behavior == "remove-add" {
                        action.call_method(
                            "removeEventListener",
                            vec![Js::str(&event_name), callback.clone()],
                        )?;
                    }
                    if behavior == "remove-add" {
                        action.call_method(
                            "addEventListener",
                            vec![Js::str(&event_name), callback.clone()],
                        )?;
                    }
                    if behavior == "unrelated" {
                        action.call_method(
                            "removeEventListener",
                            vec![Js::str(&event_name), missing.clone()],
                        )?;
                    }
                }
                Ok(Js::Undefined)
            });
            let done = delivered.clone();
            let seen = trace.clone();
            let count = Cell::new(0);
            let third = Js::function("third", move |_, _| {
                seen.borrow_mut().push("third".into());
                count.set(count.get() + 1);
                if count.get() == target {
                    done.notify_one();
                }
                Ok(Js::Undefined)
            });
            let mut callbacks = vec![first.clone(), second.clone()];
            if kind != "remove-add" {
                callbacks.push(third.clone());
            }
            for callback in callbacks {
                ws.call_method("addEventListener", vec![Js::str(event), callback])
                    .unwrap();
            }
            if kind == "pre-remove" {
                ws.call_method("removeEventListener", vec![Js::str(event), second.clone()])
                    .unwrap();
            }
            tokio::time::timeout(Duration::from_secs(5), delivered.notified())
                .await
                .unwrap();
            let actual = serde_json::json!(*trace.borrow());
            for callback in [first, second, third] {
                ws.call_method("removeEventListener", vec![Js::str(event), callback])
                    .unwrap();
            }
            socket.call_method("dispose", vec![]).unwrap();
            if actual != case["trace"] {
                failures.push(format!("{case}; actual {actual}"));
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    });
}
