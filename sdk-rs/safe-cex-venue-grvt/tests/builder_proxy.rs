#![cfg(not(target_arch = "wasm32"))]
use safe_cex_transport::*;
use safe_cex_types::{ExchangeName, ExchangeOptions};
use safe_cex_venue_grvt::builder::*;
use std::{
    collections::BTreeMap,
    future::Future,
    sync::{Arc, Mutex},
    task::{Context, Poll, Waker},
};

struct Proxy {
    url: String,
    effects: Arc<Mutex<Vec<String>>>,
    fail: bool,
}
impl PrivateProxy for Proxy {
    fn url(&self) -> &str {
        &self.url
    }
    fn resolve_headers(&self) -> TransportFuture<'_, BTreeMap<String, String>> {
        self.effects.lock().unwrap().push("headers".into());
        Box::pin(async {
            if self.fail {
                return Err(TransportError::Host("credential failure".into()));
            }
            Ok(BTreeMap::from([
                ("Zulu".into(), "last".into()),
                ("Alpha".into(), "first".into()),
            ]))
        })
    }
}
struct Provider {
    proxy: Option<Arc<Proxy>>,
    effects: Arc<Mutex<Vec<String>>>,
}
impl TransportProvider for Provider {
    fn name(&self) -> &str {
        "probe"
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
    fn private_proxy(&self, exchange: &ExchangeName) -> Option<Arc<dyn PrivateProxy>> {
        assert_eq!(exchange, &ExchangeName::Grvt, "BUILDER-PROXY venue");
        self.effects
            .lock()
            .unwrap()
            .extend(["get".into(), "grvt".into()]);
        self.proxy
            .clone()
            .map(|proxy| proxy as Arc<dyn PrivateProxy>)
    }
    fn require_proxy(&self, o: ExchangeOptions) -> ExchangeOptions {
        o
    }
    fn server_proxy_url(&self, _: &ExchangeName) -> Option<String> {
        None
    }
    fn ws_routing(&self, e: &ExchangeName, o: &RoutingOptions) -> ResolvedRouting {
        DirectTransport.ws_routing(e, o)
    }
    fn ws_connection(&self, _: &str) -> Option<WsConnection> {
        None
    }
}
fn ready<T>(future: impl Future<Output = T>) -> T {
    match std::pin::pin!(future)
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("fixture future unexpectedly pending"),
    }
}
#[test]
fn builder_proxy_matches_actual_source() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/builder-proxy.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let capture: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let rows = capture["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 10, "BUILDER-PROXY source total");
    let mut compared = 0;
    for (index, (url, testnet)) in [
        None,
        Some(""),
        Some("https://proxy"),
        Some("https://proxy/"),
        Some("代理/😀"),
    ]
    .into_iter()
    .flat_map(|url| [false, true].map(|testnet| (url, testnet)))
    .enumerate()
    {
        let row = &rows[index];
        assert_eq!(
            row["proxyUrl"],
            serde_json::json!(url),
            "BUILDER-PROXY source input url"
        );
        assert_eq!(
            row["testnet"], testnet,
            "BUILDER-PROXY source input network"
        );
        let effects = Arc::new(Mutex::new(Vec::new()));
        let proxy = url.map(|url| {
            Arc::new(Proxy {
                url: url.into(),
                effects: effects.clone(),
                fail: false,
            })
        });
        let registry = TransportRegistry::default();
        registry.set_default_transport(Arc::new(Provider {
            proxy: proxy.clone(),
            effects: effects.clone(),
        }));
        match (grvt_proxy(&registry), &proxy) {
            (Some(actual), Some(expected)) => assert!(
                Arc::ptr_eq(&actual, &(expected.clone() as Arc<dyn PrivateProxy>)),
                "BUILDER-PROXY native identity"
            ),
            (None, None) => {}
            _ => panic!("BUILDER-PROXY native presence"),
        }
        assert_eq!(row["same"], true, "BUILDER-PROXY source identity");
        assert_eq!(
            via_grvt_proxy(&registry, "base/?q=1"),
            row["via"],
            "BUILDER-PROXY native url"
        );
        assert_eq!(
            get_grvt_edge_api_url(&registry, testnet),
            row["edge"],
            "BUILDER-PROXY native edge"
        );
        assert_eq!(
            get_grvt_trading_api_url(&registry, testnet),
            row["trading"],
            "BUILDER-PROXY native trading"
        );
        let pending = resolve_grvt_proxy_headers(&registry);
        assert_eq!(
            effects.lock().unwrap().len(),
            if url.is_some() { 11 } else { 10 },
            "BUILDER-PROXY eager resolver invocation"
        );
        assert_eq!(
            row["atInvocation"].as_array().unwrap().len(),
            if url.is_some() { 11 } else { 10 },
            "BUILDER-PROXY source eager headers"
        );
        assert_eq!(
            *effects.lock().unwrap(),
            row["atInvocation"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap().to_owned())
                .collect::<Vec<_>>(),
            "BUILDER-PROXY native invocation prefix"
        );
        let headers = ready(pending).unwrap();
        assert_eq!(
            headers.len(),
            if url.is_some() { 2 } else { 0 },
            "BUILDER-PROXY native header count"
        );
        assert_eq!(
            headers.keys().map(String::as_str).collect::<Vec<_>>(),
            if url.is_some() {
                vec!["Alpha", "Zulu"]
            } else {
                vec![]
            },
            "BUILDER-PROXY native header order"
        );
        assert_eq!(
            row["headerOrder"],
            if url.is_some() {
                serde_json::json!(["Zulu", "Alpha"])
            } else {
                serde_json::json!([])
            },
            "BUILDER-PROXY source header order"
        );
        for key in ["Alpha", "Zulu"] {
            assert_eq!(
                headers.get(key).map(String::as_str),
                row["headers"].get(key).map(|v| v.as_str().unwrap()),
                "BUILDER-PROXY native header identity"
            );
        }
        assert_eq!(
            serde_json::json!(*effects.lock().unwrap()),
            row["effects"],
            "BUILDER-PROXY native effects"
        );
        compared += 1;
    }
    assert_eq!(compared, 10, "BUILDER-PROXY compared rows");
}
#[test]
fn proxy_rejection_and_registry_replacement_survive() {
    let registry = TransportRegistry::default();
    let effects = Arc::new(Mutex::new(Vec::new()));
    let proxy = Arc::new(Proxy {
        url: "".into(),
        effects: effects.clone(),
        fail: true,
    });
    registry.set_default_transport(Arc::new(Provider {
        proxy: Some(proxy),
        effects: effects.clone(),
    }));
    let pending = resolve_grvt_proxy_headers(&registry);
    registry.set_default_transport(Arc::new(DirectTransport));
    assert_eq!(
        ready(pending),
        Err(TransportError::Host("credential failure".into())),
        "BUILDER-PROXY rejection identity"
    );
    assert_eq!(*effects.lock().unwrap(), ["get", "grvt", "headers"]);
    registry.set_default_transport(Arc::new(DirectTransport));
    assert_eq!(
        via_grvt_proxy(&registry, "next"),
        "next",
        "BUILDER-PROXY replacement"
    );
    assert!(
        ready(resolve_grvt_proxy_headers(&registry))
            .unwrap()
            .is_empty()
    );
}

#[test]
fn source_empty_url_mutation_fails_by_name() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "builder_proxy_matches_actual_source",
            "--nocapture",
        ])
        .env("GRVT_PROXY_MUTATION", "empty")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(101));
    let log = String::from_utf8_lossy(&output.stderr);
    assert!(log.contains("BUILDER-PROXY native url"), "{log}");
}

struct DeferredProxy {
    effects: Arc<Mutex<Vec<String>>>,
    ready: Arc<std::sync::atomic::AtomicBool>,
    waiter: Arc<Mutex<Option<Waker>>>,
}
struct DeferredHeaders {
    effects: Arc<Mutex<Vec<String>>>,
    ready: Arc<std::sync::atomic::AtomicBool>,
    waiter: Arc<Mutex<Option<Waker>>>,
}
impl Future for DeferredHeaders {
    type Output = Result<BTreeMap<String, String>, TransportError>;
    fn poll(self: std::pin::Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if self.ready.load(std::sync::atomic::Ordering::SeqCst) {
            self.effects.lock().unwrap().push("continued".into());
            Poll::Ready(Ok(BTreeMap::from([("Zulu".into(), "last".into())])))
        } else {
            *self.waiter.lock().unwrap() = Some(cx.waker().clone());
            Poll::Pending
        }
    }
}
impl PrivateProxy for DeferredProxy {
    fn url(&self) -> &str {
        "deferred"
    }
    fn resolve_headers(&self) -> TransportFuture<'_, BTreeMap<String, String>> {
        self.effects.lock().unwrap().push("headers".into());
        Box::pin(DeferredHeaders {
            effects: self.effects.clone(),
            ready: self.ready.clone(),
            waiter: self.waiter.clone(),
        })
    }
}
struct CountingWake(std::sync::atomic::AtomicUsize);
impl std::task::Wake for CountingWake {
    fn wake(self: Arc<Self>) {
        self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }
    fn wake_by_ref(self: &Arc<Self>) {
        self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }
}
struct DeferredProvider(Arc<DeferredProxy>);
impl TransportProvider for DeferredProvider {
    fn name(&self) -> &str {
        "deferred"
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
    fn private_proxy(&self, e: &ExchangeName) -> Option<Arc<dyn PrivateProxy>> {
        assert_eq!(e, &ExchangeName::Grvt);
        Some(self.0.clone())
    }
    fn require_proxy(&self, o: ExchangeOptions) -> ExchangeOptions {
        o
    }
    fn server_proxy_url(&self, _: &ExchangeName) -> Option<String> {
        None
    }
    fn ws_routing(&self, e: &ExchangeName, o: &RoutingOptions) -> ResolvedRouting {
        DirectTransport.ws_routing(e, o)
    }
    fn ws_connection(&self, _: &str) -> Option<WsConnection> {
        None
    }
}
#[test]
fn eager_pending_headers_retain_selection_and_register_caller_waker() {
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    let effects = Arc::new(Mutex::new(vec![]));
    let ready = Arc::new(AtomicBool::new(false));
    let waiter = Arc::new(Mutex::new(None));
    let proxy = Arc::new(DeferredProxy {
        effects: effects.clone(),
        ready: ready.clone(),
        waiter: waiter.clone(),
    });
    let registry = TransportRegistry::default();
    registry.set_default_transport(Arc::new(DeferredProvider(proxy)));
    let mut future = resolve_grvt_proxy_headers(&registry);
    assert_eq!(
        *effects.lock().unwrap(),
        ["headers"],
        "BUILDER-PROXY eager pending resolver"
    );
    registry.set_default_transport(Arc::new(DirectTransport));
    let counter = Arc::new(CountingWake(AtomicUsize::new(0)));
    let waker = Waker::from(counter.clone());
    assert!(
        future
            .as_mut()
            .poll(&mut Context::from_waker(&waker))
            .is_pending(),
        "BUILDER-PROXY retained pending continuation"
    );
    ready.store(true, Ordering::SeqCst);
    waiter.lock().unwrap().take().unwrap().wake();
    assert_eq!(
        counter.0.load(Ordering::SeqCst),
        1,
        "BUILDER-PROXY caller waker refreshed"
    );
    let Poll::Ready(Ok(headers)) = future.as_mut().poll(&mut Context::from_waker(&waker)) else {
        panic!("BUILDER-PROXY pending resolver did not complete")
    };
    assert_eq!(
        headers.get("Zulu").map(String::as_str),
        Some("last"),
        "BUILDER-PROXY pending native header value"
    );
    assert_eq!(
        headers.len(),
        1,
        "BUILDER-PROXY independent pending header floor"
    );
    assert_eq!(
        *effects.lock().unwrap(),
        ["headers", "continued"],
        "BUILDER-PROXY resolver invoked once"
    );
}

#[test]
fn borrowed_future_cancellation_boundary_is_measured() {
    use std::sync::atomic::{AtomicBool, Ordering};
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/builder-proxy.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let capture: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let effects = Arc::new(Mutex::new(vec![]));
    let ready = Arc::new(AtomicBool::new(false));
    let waiter = Arc::new(Mutex::new(None));
    let proxy = Arc::new(DeferredProxy {
        effects: effects.clone(),
        ready: ready.clone(),
        waiter: waiter.clone(),
    });
    let registry = TransportRegistry::default();
    registry.set_default_transport(Arc::new(DeferredProvider(proxy)));
    let future = resolve_grvt_proxy_headers(&registry);
    drop(future);
    ready.store(true, Ordering::SeqCst);
    assert_eq!(
        *effects.lock().unwrap(),
        ["headers"],
        "BUILDER-PROXY borrowed resolver dropped before continuation"
    );
    assert_eq!(
        capture["abandonedEffects"],
        serde_json::json!(["headers", "continued"]),
        "BUILDER-PROXY source discarded observer still continues"
    );
}
