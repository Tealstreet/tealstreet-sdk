#![cfg(all(feature = "native", not(target_arch = "wasm32")))]
use safe_cex_exchange::{
    capabilities::ExchangeCapabilities,
    exchange::{BaseExchange, Exchange, ExchangeSettings},
    js_runtime::{
        client, globals,
        promise::Promise,
        scope::ScopeShared,
        sockets::base_websocket,
        value::{Js, Thrown},
    },
    native::native_runtime,
    runtime::ExchangeRuntime,
};
use std::{cell::RefCell, net::TcpListener, process::Command, rc::Rc, sync::Arc, time::Duration};
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
fn native_socket_listener_failures_are_not_transport_errors() {
    if std::env::var_os("SAFE_CEX_SOCKET_LISTENER_CHILD").is_none() {
        let output = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "native_socket_listener_failures_are_not_transport_errors",
                "--nocapture",
            ])
            .env("SAFE_CEX_SOCKET_LISTENER_CHILD", "1")
            .output()
            .unwrap();
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            output.status.success(),
            "{}\n{stderr}",
            String::from_utf8_lossy(&output.stdout)
        );
        assert_eq!(
            stderr
                .matches("[safe-cex uncaught] Error: listener throw")
                .count(),
            4,
            "{stderr}"
        );
        assert_eq!(
            stderr
                .matches("[safe-cex uncaught] Error: listener rejection")
                .count(),
            8,
            "{stderr}"
        );
        assert!(!stderr.contains("WebSocket error:"), "{stderr}");
        return;
    }
    let oracle = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/js_socket_listener_oracle.cjs"
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
        let mut failures = Vec::new();
        for event in ["open", "message"] {
            for case in &cases {
                let grouped = case["grouped"].as_bool().unwrap();
                let rejection = case["rejection"].as_bool().unwrap();
                let deferred = case["deferred"].as_bool().unwrap();
                let listener = TcpListener::bind("127.0.0.1:0").unwrap();
                let url = format!("ws://{}", listener.local_addr().unwrap());
                let (release, released) = std::sync::mpsc::channel();
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
                                assert!(
                                    std::time::Instant::now() < deadline,
                                    "client did not dial"
                                );
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
                    ws.send(Message::Text("private-frame".into())).unwrap();
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
                        runtime.clone(),
                        ExchangeSettings::default(),
                    ),
                    ExchangeCapabilities::default(),
                ));
                *shared.sockets.venue.borrow_mut() = Some(Rc::downgrade(&owner));
                let logs = Rc::new(RefCell::new(Vec::new()));
                let captured = logs.clone();
                owner.base().on_args(
                    "log",
                    Rc::new(move |_, args| {
                        captured.borrow_mut().push(args.to_vec());
                        Ok(())
                    }),
                );
                let socket = base_websocket(&shared.sockets, Js::object(vec![]));
                let options = if grouped {
                    Js::object(vec![(
                        "headers",
                        Js::object(vec![("X-Witness", Js::str("listener"))]),
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
                let delivered = Rc::new(tokio::sync::Notify::new());
                let seen = delivered.clone();
                let first = trace.clone();
                let promise_runtime: Rc<dyn ExchangeRuntime> = runtime.clone();
                ws.call_method(
                    "addEventListener",
                    vec![
                        Js::str(event),
                        Js::function("first", move |_, _| {
                            first.borrow_mut().push("first".into());
                            seen.notify_one();
                            let thrown = Thrown::error(
                                "Error",
                                if rejection {
                                    "listener rejection"
                                } else {
                                    "listener throw"
                                },
                            );
                            if deferred {
                                let first = Promise::settled(&promise_runtime, Ok(Js::Undefined));
                                Ok(Js::Promise(Promise::spawn(
                                    &promise_runtime,
                                    Box::pin(async move {
                                        first.wait().await?;
                                        Err(thrown)
                                    }),
                                )))
                            } else if rejection {
                                Ok(Js::Promise(Promise::settled(&promise_runtime, Err(thrown))))
                            } else {
                                Err(thrown)
                            }
                        }),
                    ],
                )
                .unwrap();
                for (name, label) in [(event, "second"), ("error", "socket-error")] {
                    let trace = trace.clone();
                    ws.call_method(
                        "addEventListener",
                        vec![
                            Js::str(name),
                            Js::function(label, move |_, _| {
                                trace.borrow_mut().push(label.into());
                                Ok(Js::Undefined)
                            }),
                        ],
                    )
                    .unwrap();
                }
                tokio::time::timeout(Duration::from_secs(5), delivered.notified())
                    .await
                    .unwrap();
                let actual = serde_json::json!(*trace.borrow());
                let captured = logs.borrow().clone();
                socket.call_method("dispose", vec![]).unwrap();
                if actual != case["trace"] {
                    failures.push(format!("{event}: {case}; actual {actual}"));
                }
                if captured
                    .iter()
                    .flatten()
                    .any(|v| v.to_string().contains("WebSocket error:"))
                {
                    failures.push(format!(
                        "{event}: {case}; unexpected transport log {captured:?}"
                    ));
                }
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    });
}
