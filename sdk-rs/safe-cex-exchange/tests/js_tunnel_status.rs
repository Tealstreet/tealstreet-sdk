//! The installed HOW adapter owns settlement; execute real native client/transport.
#![cfg(all(feature = "native", not(target_arch = "wasm32")))]
use safe_cex_exchange::{
    exchange::{BaseExchange, ExchangeSettings},
    js_runtime::{
        client::{self, factory},
        promise::await_js,
        value::{Js, arg, from_json, to_json},
    },
    native::native_tealstreet_runtime_with,
    websocket::WsRuntime,
};
use safe_cex_store::store::DefaultStore;
use safe_cex_transport::{
    RestAdapter, RestAdapterVariant, RestFuture, RestRequest, RestResponse, RestResponseBody,
    TransportError, TransportFuture,
    native::NativeTunnelHost,
    socket::{
        SocketCapabilities, SocketFrame, TunnelDialOptions, TunnelHost, TunnelSocket, TunnelTask,
    },
};
use safe_cex_types::ExchangeName;
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    future::Future,
    pin::Pin,
    rc::Rc,
    sync::Arc,
};

// Only network I/O is scripted: production HOW handles auth, nonce routing,
// response decoding, native scheduling and factory adapter installation.
#[derive(Default)]
struct Wire {
    status: Cell<u16>,
    requests: RefCell<Vec<Value>>,
    incoming: Rc<RefCell<VecDeque<SocketFrame>>>,
    available: Rc<tokio::sync::Notify>,
}
impl Wire {
    fn push(&self, frame: Value) {
        self.incoming
            .borrow_mut()
            .push_back(SocketFrame::Text(frame.to_string()));
        self.available.notify_one();
    }
}
impl TunnelSocket for Wire {
    fn send(&self, frame: SocketFrame) -> Result<(), TransportError> {
        let SocketFrame::Text(text) = frame else {
            panic!("text protocol");
        };
        let frame: Value = serde_json::from_str(&text).unwrap();
        match frame["type"].as_str() {
            Some("rpc.auth") => self.push(json!({"type":"rpc.auth","status":"success"})),
            Some("rpc.http") => {
                self.requests.borrow_mut().push(frame.clone());
                self.push(json!({"type":"rpc.http","nonce":frame["nonce"],"data":{
                    "status":self.status.get(),"status_text":"OK","config":{"url":frame["arg"]["url"]},"headers":{},"data":{"code":"-1102","msg":"venue response"}
                }}));
            }
            other => panic!("unexpected tunnel write {other:?}"),
        }
        Ok(())
    }
    fn receive(&self) -> TransportFuture<'static, SocketFrame> {
        let (queue, available) = (self.incoming.clone(), self.available.clone());
        Box::pin(async move {
            loop {
                if let Some(frame) = queue.borrow_mut().pop_front() {
                    return Ok(frame);
                }
                available.notified().await;
            }
        })
    }
    fn close(&self, _: Option<u16>, _: &str) -> Result<(), TransportError> {
        Ok(())
    }
}
struct WireHost(Rc<Wire>);
impl TunnelHost for WireHost {
    fn spawn_local(&self, future: Pin<Box<dyn Future<Output = ()>>>) -> Box<dyn TunnelTask> {
        NativeTunnelHost.spawn_local(future)
    }
    fn capabilities(&self) -> SocketCapabilities {
        NativeTunnelHost.capabilities()
    }
    fn now_ms(&self) -> u64 {
        NativeTunnelHost.now_ms()
    }
    fn sleep_until(&self, deadline: u64) -> TransportFuture<'static, ()> {
        NativeTunnelHost.sleep_until(deadline)
    }
    fn connect(
        &self,
        _: String,
        _: TunnelDialOptions,
    ) -> TransportFuture<'static, Rc<dyn TunnelSocket>> {
        let socket = self.0.clone();
        socket.push(json!({"type":"serverTime","serverTime":1_700_000_000_000u64}));
        Box::pin(async move { Ok(socket as Rc<dyn TunnelSocket>) })
    }
    fn proxy_token(&self) -> TransportFuture<'_, String> {
        panic!("native token host supplies token")
    }
    fn is_token_expired(&self, token: &str) -> bool {
        NativeTunnelHost.is_token_expired(token)
    }
    fn sync_server_time(&self, _: u64) {}
}
struct Http500;
impl RestAdapter for Http500 {
    fn execute(&self, _: RestRequest) -> RestFuture<'_, RestResponse> {
        Box::pin(async {
            Ok(RestResponse {
                status: 500,
                reason: None,
                headers: Default::default(),
                body: RestResponseBody::Untransformed(
                    safe_cex_types::retained_json::RetainedJsonInput::from_value(
                        json!({"direct":true}),
                    )
                    .unwrap(),
                ),
            })
        })
    }
}

#[test]
fn installed_tunnel_statuses_follow_source_fulfilled_interceptors() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/js_tunnel_status_source.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let expected: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        expected.len(),
        4,
        "independent maintained-source status cases"
    );
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    tokio::task::LocalSet::new().block_on(&rt, async {
        let wire = Rc::new(Wire::default());
        let runtime = native_tealstreet_runtime_with(
            Rc::new(|| Box::pin(async { Ok("e30.eyJleHAiOjQxMDI0NDQ4MDB9.x".into()) })),
            Rc::new(WireHost(wire.clone())),
        );
        let host = client::rest_http_host(runtime.clone(), Arc::new(Http500));
        let options = from_json(&json!({"key":"","secret":"","tealstreetTunnel":true}));
        let owner = Rc::new(BaseExchange::new(
            ExchangeName::Bitrue,
            serde_json::from_value(to_json(&options).unwrap()).unwrap(),
            Rc::new(DefaultStore::new()),
            runtime.clone(),
            ExchangeSettings::default(),
        ));
        let provider = runtime.transport();
        let xhr = factory::create(
            host.clone(),
            provider.as_ref(),
            from_json(&json!({"baseURL":"https://venue.invalid"})),
            &options,
            &owner,
        )
        .unwrap();
        factory::install_transport_adapter(
            host.clone(),
            provider.as_ref(),
            &xhr,
            &options,
            ExchangeName::Bitrue,
            RestAdapterVariant::Default,
        )
        .unwrap();
        let events: Rc<RefCell<Vec<&str>>> = Rc::default();
        let success = events.clone();
        let failure = events.clone();
        xhr.get("interceptors")
            .unwrap()
            .get("response")
            .unwrap()
            .call_method(
                "use",
                vec![
                    Js::function("fulfilled", move |_, args| {
                        success.borrow_mut().push("fulfilled");
                        Ok(arg(&args, 0))
                    }),
                    Js::function("rejected", move |_, args| {
                        failure.borrow_mut().push("rejected");
                        Err(safe_cex_exchange::js_runtime::value::Thrown::new(arg(
                            &args, 0,
                        )))
                    }),
                ],
            )
            .unwrap();
        for row in expected {
            let status = row["status"].as_u64().unwrap() as u16;
            wire.status.set(status);
            events.borrow_mut().clear();
            let response = tokio::time::timeout(
                std::time::Duration::from_secs(3),
                await_js(
                    &(runtime.clone() as Rc<dyn safe_cex_exchange::runtime::ExchangeRuntime>),
                    xhr.call_method(
                        "post",
                        vec![
                            Js::str("/order"),
                            Js::object(vec![]),
                            from_json(&json!({"marker":"request context"})),
                        ],
                    )
                    .unwrap(),
                ),
            )
            .await
            .unwrap();
            let response = response.unwrap_or_else(|error| {
                panic!(
                    "HOW status {status} must fulfill like maintained source: {}",
                    error.message()
                )
            });
            assert_eq!(
                response.get("status").unwrap().as_f64(),
                Some(f64::from(status))
            );
            assert_eq!(
                json!(*events.borrow()),
                row["events"],
                "HOW status {status} response chain"
            );
            assert_eq!(
                to_json(&response.get("data").unwrap()).unwrap(),
                row["data"]
            );
            assert_eq!(
                to_json(&response.get("config").unwrap().get("marker").unwrap()).unwrap(),
                row["marker"]
            );
        }
        assert_eq!(
            wire.requests.borrow().len(),
            4,
            "all requests reached actual HOW rpc.http"
        );
        assert!(
            wire.requests
                .borrow()
                .iter()
                .all(|frame| frame["arg"]["url"] == "https://venue.invalid/order"
                    && frame["arg"]["method"] == "POST")
        );
        let direct = client::create(host, Js::object(vec![]));
        let error = await_js(
            &(runtime.clone() as Rc<dyn safe_cex_exchange::runtime::ExchangeRuntime>),
            direct.call_method("get", vec![Js::str("/direct")]).unwrap(),
        )
        .await
        .unwrap_err();
        assert_eq!(
            error.message(),
            "Request failed with status code 500",
            "default HTTP adapter still owns its rejection"
        );
    });
}
