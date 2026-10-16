use super::*;
use safe_cex_exchange::{
    capabilities::ExchangeCapabilities,
    exchange::{BaseExchange, Exchange},
    js_runtime::sockets,
};
use safe_cex_types::PlaceOrderOpts;
use std::cell::Cell;

struct Venue {
    base: Rc<BaseExchange>,
    capabilities: ExchangeCapabilities,
}
impl Exchange for Venue {
    type PlaceOpts = PlaceOrderOpts;
    fn base(&self) -> &BaseExchange {
        &self.base
    }
    fn capabilities(&self) -> &ExchangeCapabilities {
        &self.capabilities
    }
}
fn poll_jobs(runtime: &Runtime) {
    for _ in 0..4 {
        let jobs = std::mem::take(&mut *runtime.jobs.borrow_mut());
        for mut job in jobs {
            if job
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop()))
                .is_pending()
            {
                runtime.jobs.borrow_mut().push(job);
            }
        }
    }
}
#[test]
fn socket_options_match_source_and_auth_preparation_precedes_every_dial() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/js_websocket_options_source.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let expected: Value = serde_json::from_slice(&output.stdout).unwrap();
    let assembled = assemble(json!({"key":"synthetic", "secret":"synthetic"})).unwrap();
    assembled.runtime.headers_capable.set(true);
    assembled.runtime.hold_sleep.set(true);
    let venue: Rc<dyn Exchange> = Rc::new(Venue {
        base: assembled.base,
        capabilities: Default::default(),
    });
    *assembled.context.venue.borrow_mut() = Some(Rc::downgrade(&venue));
    let socket = sockets::base_websocket(&assembled.context, assembled.this);
    let generation = Rc::new(Cell::new(0));
    let fail = Rc::new(Cell::new(false));
    let (counter, failure) = (generation.clone(), fail.clone());
    let provider = Js::function("authHeaders", move |_, _| {
        if failure.get() {
            return Err(Thrown::error("Error", "auth refused"));
        }
        counter.set(counter.get() + 1);
        Ok(Js::object(vec![(
            "ACCESS-TIMESTAMP",
            Js::str(&counter.get().to_string()),
        )]))
    });
    let options = Js::object(vec![
        ("headersRequired", Js::Bool(true)),
        ("headers", provider),
        ("minReconnectionDelay", Js::Number(17.0)),
    ]);
    let args = || {
        vec![
            Js::str("wss://synthetic.test/private"),
            Js::array(vec![Js::str("auth-v1")]),
            options.clone(),
        ]
    };
    for _ in 0..2 {
        socket.call_method("createWebsocket", args()).unwrap();
        poll_jobs(&assembled.runtime);
    }
    let actual: Vec<_> = assembled.runtime.dials.borrow().iter().map(|(url, protocols, options)| {
        let headers = match options.headers.as_ref().expect("required headers") {
            safe_cex_transport::relay::UpgradeHeaders::Static(headers) => headers.clone(),
            safe_cex_transport::relay::UpgradeHeaders::Fresh(provider) => provider(),
        };
        json!({"url":url,"protocols":protocols,"headers":headers,"minReconnectionDelay":options.min_reconnection_delay_ms.map(|value| format!("{:016x}", value.to_bits()))})
    }).collect();
    assert_eq!(actual.len(), 2, "ws_options::attempt_count");
    assert_eq!(json!(actual), expected, "ws_options::source_dial_inputs");
    assert_eq!(generation.get(), 2, "ws_options::fresh_auth");
    fail.set(true);
    socket.call_method("createWebsocket", args()).unwrap();
    poll_jobs(&assembled.runtime);
    assert_eq!(
        assembled.runtime.dials.borrow().len(),
        2,
        "ws_options::auth_failure_no_dial"
    );
    assembled.runtime.headers_capable.set(false);
    let error = socket.call_method("createWebsocket", args()).unwrap_err();
    assert_eq!(
        error.value.get("name").unwrap().to_display(),
        "HeaderCapableWebSocketUnavailableError"
    );
    assert_eq!(
        assembled.runtime.dials.borrow().len(),
        2,
        "ws_options::unsupported_no_dial"
    );
    socket.call_method("dispose", vec![]).unwrap();
}

#[cfg(all(feature = "native", not(target_arch = "wasm32")))]
#[test]
fn native_header_auth_dials_plain_socket_and_refuses_headerless_wrapper_requests() {
    use safe_cex_exchange::websocket::{WebSocketConnectionOptions, WsRuntime};
    use safe_cex_transport::relay::UpgradeHeaders;
    use std::{net::TcpListener, thread};
    use tokio_tungstenite::tungstenite::{
        accept_hdr,
        handshake::server::{Request, Response},
    };
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let url = format!("ws://{}/private", listener.local_addr().unwrap());
    let server = thread::spawn(move || {
        let mut observed = vec![];
        for _ in 0..2 {
            let started = std::time::Instant::now();
            let stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        assert!(
                            started.elapsed().as_secs() < 5,
                            "header witness accept deadline"
                        );
                        thread::sleep(std::time::Duration::from_millis(2));
                    }
                    Err(error) => panic!("{error}"),
                }
            };
            stream.set_nonblocking(false).unwrap();
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                .unwrap();
            let mut headers = None;
            let _socket = accept_hdr(stream, |request: &Request, response: Response| {
                headers = Some((
                    request.uri().path().to_owned(),
                    request.headers()["ACCESS-TIMESTAMP"]
                        .to_str()
                        .unwrap()
                        .to_owned(),
                ));
                Ok(response)
            })
            .unwrap();
            observed.push(headers.unwrap());
        }
        observed
    });
    let tokio = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    tokio::task::LocalSet::new().block_on(&tokio, async {
        let runtime = safe_cex_exchange::native::native_runtime();
        let count = Rc::new(Cell::new(0));
        let counter = count.clone();
        let options = WebSocketConnectionOptions {
            headers_required: true,
            headers: Some(UpgradeHeaders::Fresh(Rc::new(move || {
                counter.set(counter.get() + 1);
                [("ACCESS-TIMESTAMP".into(), counter.get().to_string())]
                    .into_iter()
                    .collect()
            }))),
            ..Default::default()
        };
        for _ in 0..2 {
            let socket = runtime
                .dial(url.clone(), vec![], options.clone(), true)
                .await
                .unwrap_or_else(|error| panic!("ws_options::native_header_dial {}", error.message));
            assert!(
                !socket.is_reconnecting(),
                "the core retains native reconnect ownership"
            );
            socket.close();
        }
        assert_eq!(count.get(), 2, "ws_options::native_fresh_headers");
        let refused = runtime.dial(url, vec![], Default::default(), true).await;
        let error = match refused {
            Ok(_) => panic!("header-less wrapper unexpectedly dialed"),
            Err(error) => error,
        };
        assert!(
            error.message.contains("plain sockets only"),
            "{}",
            error.message
        );
    });
    assert_eq!(
        server.join().unwrap(),
        [
            ("/private".into(), "1".into()),
            ("/private".into(), "2".into())
        ]
    );
}
