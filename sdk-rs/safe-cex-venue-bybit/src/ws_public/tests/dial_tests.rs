use super::*;
use safe_cex_transport::{
    PrivateProxy, ResolvedRouting, ResolvedWs, RestAdapter, RestAdapterContext, RestTransport,
    RestTransportContext, RoutingOptions, TransportError, TransportFuture, WsConnection,
    socket::{SocketCapabilities, SocketRuntime, TunnelDialOptions, TunnelSocket, TunnelTask},
};
use safe_cex_types::ExchangeName;
use std::sync::Mutex;

#[derive(Default)]
pub(super) struct PendingTunnelHost {
    calls: RefCell<Vec<Value>>,
}
impl TunnelHost for PendingTunnelHost {
    fn spawn_local(&self, _: Pin<Box<dyn Future<Output = ()>>>) -> Box<dyn TunnelTask> {
        panic!("relay task must stay core-owned")
    }
    fn capabilities(&self) -> SocketCapabilities {
        SocketCapabilities {
            runtime: SocketRuntime::Native,
            upgrade_headers: true,
            alpn: true,
        }
    }
    fn now_ms(&self) -> u64 {
        1000
    }
    fn sleep_until(&self, _: u64) -> TransportFuture<'static, ()> {
        Box::pin(std::future::pending())
    }
    fn connect(
        &self,
        url: String,
        options: TunnelDialOptions,
    ) -> TransportFuture<'static, Rc<dyn TunnelSocket>> {
        self.calls
            .borrow_mut()
            .push(json!({"url":url,"headers":options.headers(),"protocols":options.protocols()}));
        Box::pin(std::future::pending())
    }
    fn proxy_token(&self) -> TransportFuture<'_, String> {
        Box::pin(async { Ok("token".into()) })
    }
    fn is_token_expired(&self, _: &str) -> bool {
        false
    }
    fn sync_server_time(&self, _: u64) {}
}
struct Provider {
    route: ResolvedWs,
    connections: Mutex<Vec<String>>,
    relay_kind: bool,
}
impl TransportProvider for Provider {
    fn name(&self) -> &str {
        "fixture"
    }
    fn rest(&self, c: RestTransportContext<'_>) -> Result<RestTransport, TransportError> {
        DirectTransport.rest(c)
    }
    fn rest_adapter(&self, c: RestAdapterContext<'_>) -> Option<Arc<dyn RestAdapter>> {
        DirectTransport.rest_adapter(c)
    }
    fn proxy_prefix(&self, c: RestTransportContext<'_>) -> String {
        DirectTransport.proxy_prefix(c)
    }
    fn public_cors_proxy(&self) -> Option<String> {
        None
    }
    fn private_proxy(&self, _: &ExchangeName) -> Option<Arc<dyn PrivateProxy>> {
        None
    }
    fn require_proxy(&self, o: ExchangeOptions) -> ExchangeOptions {
        o
    }
    fn server_proxy_url(&self, _: &ExchangeName) -> Option<String> {
        None
    }
    fn ws_routing(&self, name: &ExchangeName, options: &RoutingOptions) -> ResolvedRouting {
        let mut route = DirectTransport.ws_routing(name, options);
        route.ws_public = self.route.clone();
        route.ws_private = self.route.clone();
        route
    }
    fn ws_connection(&self, url: &str) -> Option<WsConnection> {
        self.connections.lock().unwrap().push(url.into());
        (self.relay_kind && url.contains("relay")).then(|| WsConnection::Relay {
            headers: Default::default(),
        })
    }
}
fn poll_tasks(runtime: &Runtime) {
    // Snapshot task identities, not futures; spawning inside a poll must not borrow this list.
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
fn dialing_matches_live_source_and_core_owns_cancellation() {
    let run = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/ws-dial-fixtures.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let cases: Value = serde_json::from_slice(&run.stdout).unwrap();
    assert_eq!(
        cases.as_array().unwrap().len(),
        720,
        "DIAL-CARDINALITY source total"
    );
    for (index, case) in cases.as_array().unwrap().iter().enumerate() {
        for inline in [false, true] {
            let (_, old_parent, _) = fixture();
            let parent = Rc::new(Parent {
                failure: RefCell::default(),
                health_writes: Cell::new(0),
                options: serde_json::from_value(case["options"].clone()).unwrap(),
                ..Rc::try_unwrap(old_parent).ok().unwrap()
            });
            let provider = Arc::new(Provider {
                route: serde_json::from_value(case["route"].clone()).unwrap(),
                connections: Mutex::default(),
                relay_kind: case["relayKind"] == true,
            });
            let runtime = Rc::new(Runtime::default());
            runtime.inline.set(inline);
            // The clock does not advance in this dial-only corpus. A ready timer
            // would repeatedly expire core relay opening/retry deadlines inline.
            runtime.pending_timers.set(true);
            *runtime.provider.borrow_mut() = Some(provider.clone());
            let socket = BaseWebSocket::new(7, parent.clone(), runtime.clone(), Rc::new(Protocol));
            parent.disposed.set(case["disposed"] == true);
            let mut actual = Vec::new();
            for _ in 0..6 {
                runtime.dials.borrow_mut().clear();
                runtime.relay_host.calls.borrow_mut().clear();
                if case["private"] == true {
                    crate::ws_private::connect_and_subscribe(
                        &socket,
                        runtime.as_ref(),
                        case["demo"] == true,
                        case["override"].as_str(),
                        case["signer"] == true,
                        case["relayEnabled"] == true,
                    )
                    .unwrap();
                } else {
                    connect_and_subscribe(
                        &socket,
                        runtime.as_ref(),
                        case["category"].as_str().unwrap(),
                        case["demo"] == true,
                        case["override"].as_str(),
                    )
                    .unwrap();
                }
                poll_tasks(&runtime);
                poll_tasks(&runtime);
                let queries = std::mem::take(&mut *provider.connections.lock().unwrap());
                if let Some(url) = queries.last() {
                    assert_eq!(
                        queries.len(),
                        if case["private"] == true { 2 } else { 1 },
                        "core query distinct from preflight {index}"
                    );
                    let headers = if url.contains("relay") {
                        let calls = runtime.relay_host.calls.borrow();
                        assert_eq!(calls.len(), 1, "exactly one real relay host start {index}");
                        assert_eq!(calls[0]["url"], "wss://relay/ws");
                        let mut headers = calls[0]["headers"].as_object().unwrap().clone();
                        headers.remove("Origin");
                        headers.remove("User-Agent");
                        Value::Object(headers)
                    } else {
                        let dials = runtime.dials.borrow();
                        assert_eq!(dials.len(), 1, "exactly one real direct host start {index}");
                        assert_eq!(dials[0]["url"], *url);
                        dials[0]["headers"].clone()
                    };
                    actual.push(json!({"url":url,"headers":headers}));
                }
            }
            assert_eq!(
                json!(actual),
                case["calls"],
                "case {index} inline={inline}: {case}"
            );
            assert_eq!(
                json!(*parent.events.borrow()),
                case["events"],
                "events case {index}"
            );
            let weak = Rc::downgrade(&socket);
            socket.dispose();
            drop(socket);
            assert!(
                weak.upgrade().is_none(),
                "dial must remain core-owned {index}"
            );
            assert!(
                runtime
                    .tasks
                    .borrow()
                    .iter()
                    .all(|task| task.borrow().is_none()),
                "all tasks canceled {index}"
            );
        }
    }
}

#[test]
fn disposing_before_dial_is_polled_releases_venue_socket() {
    for private in [false, true] {
        for inline in [false, true] {
            for pending in [true, false] {
                let (_, parent, _) = fixture();
                let runtime = Rc::new(Runtime::default());
                runtime.inline.set(inline);
                runtime.pending_dial.set(pending);
                runtime.pending_timers.set(true);
                let original_owners = Rc::strong_count(&runtime.dial_owner);
                let socket = BaseWebSocket::new(8, parent, runtime.clone(), Rc::new(Protocol));
                if private {
                    crate::ws_private::connect_and_subscribe(
                        &socket,
                        runtime.as_ref(),
                        false,
                        None,
                        false,
                        true,
                    )
                    .unwrap();
                } else {
                    connect_and_subscribe(&socket, runtime.as_ref(), "linear", false, None)
                        .unwrap();
                }
                let weak = Rc::downgrade(&socket);
                // Both maintained connectAndSubscribe members call createWebsocket
                // synchronously. No host task poll is needed to start the real dial.
                let dials = runtime.dials.borrow().clone();
                assert_eq!(
                    dials.len(),
                    1,
                    "BYBIT-EAGER-DIAL before any host pump private={private} inline={inline} pending={pending}"
                );
                assert_eq!(
                    dials[0]["url"],
                    if private {
                        "wss://stream.bybit.com/v5/private?requestId=fixture"
                    } else {
                        "wss://stream.bybit.com/v5/public/linear?requestId=fixture"
                    }
                );
                assert_eq!(dials[0]["protocols"], json!([]));
                assert_eq!(
                    Rc::strong_count(&runtime.dial_owner),
                    original_owners + usize::from(pending),
                    "BYBIT-DIAL-OWNER actual pending I/O is retained"
                );
                socket.dispose();
                assert_eq!(
                    Rc::strong_count(&runtime.dial_owner),
                    original_owners,
                    "BYBIT-DIAL-CANCEL actual I/O drops before host pump"
                );
                assert!(
                    !socket.has_websocket(),
                    "disposed dial cannot leave a handle"
                );
                drop(socket);
                assert!(
                    weak.upgrade().is_none(),
                    "cancelled dial must release venue socket before host pump"
                );
                // The canonical Promise driver can serve unrelated jobs. Disposal
                // cancels the actual owned body now; the next pump reaps its driver.
                poll_tasks(&runtime);
                assert!(
                    runtime
                        .tasks
                        .borrow()
                        .iter()
                        .all(|task| task.borrow().is_none()),
                    "cancelled driver reaped after host pump"
                );
                poll_tasks(&runtime);
                assert_eq!(
                    *runtime.dials.borrow(),
                    dials,
                    "disposed dial must not run again later"
                );
            }
        }
    }
}
