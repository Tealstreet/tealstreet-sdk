//! Production Coinbase construction, client, signing and transport; only I/O is scripted.
#[path = "support/binding_harness.rs"]
#[allow(dead_code)]
mod harness;

use safe_cex_exchange::{
    exchange::Exchange,
    js_runtime::client::JsHttpHost,
    runtime::{ExchangeError, ExchangeFuture, ExchangeRuntime, ExchangeTask},
    websocket::{WebSocketConnectionOptions, WsRuntime, WsSocket},
};
use safe_cex_transport::{socket::TunnelHost, *};
use safe_cex_types::{ExchangeName, ExchangeOptions};
use safe_cex_venue_coinbase::{CoinbaseDependencies, CoinbaseExchange};
use serde_json::{Value, json};
use std::{
    future::Future,
    pin::Pin,
    rc::Rc,
    sync::{Arc, Mutex},
    task::{Context, Poll, Waker},
};

struct Runtime {
    inner: Rc<harness::Runtime>,
    transport: Arc<Provider>,
}
impl ExchangeRuntime for Runtime {
    fn monotonic_now_ms(&self) -> f64 {
        self.inner.monotonic_now_ms()
    }
    fn now_ms(&self) -> f64 {
        self.inner.now_ms()
    }
    fn sleep(&self, ms: u64) -> ExchangeFuture<'static, ()> {
        self.inner.sleep(ms)
    }
    fn spawn_local(&self, future: Pin<Box<dyn Future<Output = ()>>>) -> Box<dyn ExchangeTask> {
        self.inner.spawn_local(future)
    }
    fn console_values(&self, severity: &str, message: &[safe_cex_exchange::js_runtime::value::Js]) {
        self.inner.console_values(severity, message);
    }
    fn unique_id(&self) -> String {
        self.inner.unique_id()
    }
    fn is_worker(&self) -> bool {
        self.inner.is_worker()
    }
    fn random(&self) -> f64 {
        self.inner.random()
    }
    fn report_exception(
        &self,
        error: &ExchangeError,
        context: &safe_cex_types::retained_json::RetainedJsonInput,
    ) {
        self.inner.report_exception(error, context);
    }
    fn report_raw_exception(
        &self,
        error: &safe_cex_exchange::js_runtime::value::Js,
        context: &safe_cex_types::retained_json::RetainedJsonInput,
    ) {
        self.inner.report_raw_exception(error, context);
    }
    fn execute_worker_request(&self, request: Value) -> ExchangeFuture<'static, Value> {
        self.inner.execute_worker_request(request)
    }
}
impl WsRuntime for Runtime {
    fn transport(&self) -> Arc<dyn TransportProvider> {
        self.transport.clone()
    }
    fn tunnel_host(&self) -> Rc<dyn TunnelHost> {
        panic!("REST regression must not dial a tunnel socket")
    }
    fn dial(
        &self,
        url: String,
        protocols: Vec<String>,
        options: WebSocketConnectionOptions,
        reconnecting: bool,
    ) -> ExchangeFuture<'static, Rc<dyn WsSocket>> {
        self.inner.dial(url, protocols, options, reconnecting)
    }
    fn headers_capable(&self) -> bool {
        self.inner.headers_capable()
    }
    fn enable_reconnecting(&self) -> bool {
        self.inner.enable_reconnecting()
    }
    fn min_reconnection_delay_ms(&self) -> f64 {
        self.inner.min_reconnection_delay_ms()
    }
    fn library_min_reconnection_delay_ms(&self) -> f64 {
        self.inner.library_min_reconnection_delay_ms()
    }
    fn diagnostics_enabled(&self) -> bool {
        self.inner.diagnostics_enabled()
    }
    fn lifecycle_logs_enabled(&self) -> bool {
        self.inner.lifecycle_logs_enabled()
    }
}

#[derive(Default)]
struct Adapter(Mutex<Vec<RestRequest>>);
impl RestAdapter for Adapter {
    fn query_in_url(&self) -> bool {
        false
    }
    fn execute(&self, request: RestRequest) -> RestFuture<'_, RestResponse> {
        let body = if request.url == "/api/v3/brokerage/portfolios" {
            portfolio()
        } else if request.url == "/api/v3/brokerage/orders" {
            json!({"success":true,"success_response":{"order_id":"wire-order","product_id":"BTC-USDC","side":"BUY","client_order_id":"wire-client"}})
        } else if request
            .url
            .starts_with("/api/v3/brokerage/orders/historical/batch")
        {
            json!({"orders":[],"has_next":false})
        } else {
            json!({})
        };
        self.0.lock().unwrap().push(request);
        Box::pin(async move {
            Ok(RestResponse {
                status: 200,
                reason: None,
                headers: Default::default(),
                body: RestResponseBody::Untransformed(
                    safe_cex_types::retained_json::RetainedJsonInput::from_value(body).unwrap(),
                ),
            })
        })
    }
}
struct Provider {
    adapter: Option<Arc<Adapter>>,
    routes: Mutex<Vec<(ExchangeName, Option<bool>)>>,
    selections: Mutex<Vec<(ExchangeName, Option<bool>, RestAdapterVariant)>>,
}
impl TransportProvider for Provider {
    fn name(&self) -> &str {
        "scripted-provider"
    }
    fn rest(&self, context: RestTransportContext<'_>) -> Result<RestTransport, TransportError> {
        self.routes
            .lock()
            .unwrap()
            .push((context.exchange.clone(), context.options.tealstreet_tunnel));
        Ok(RestTransport {
            base_url: Some("https://scripted-route.invalid".into()),
            proxied: false,
            authorize: None,
        })
    }
    fn rest_adapter(&self, context: RestAdapterContext<'_>) -> Option<Arc<dyn RestAdapter>> {
        self.selections.lock().unwrap().push((
            context.exchange.clone(),
            context.options.tealstreet_tunnel,
            context.variant,
        ));
        self.adapter
            .clone()
            .map(|adapter| adapter as Arc<dyn RestAdapter>)
    }
    fn proxy_prefix(&self, context: RestTransportContext<'_>) -> String {
        DirectTransport.proxy_prefix(context)
    }
    fn public_cors_proxy(&self) -> Option<String> {
        None
    }
    fn private_proxy(&self, _: &ExchangeName) -> Option<Arc<dyn PrivateProxy>> {
        None
    }
    fn require_proxy(&self, options: ExchangeOptions) -> ExchangeOptions {
        options
    }
    fn server_proxy_url(&self, _: &ExchangeName) -> Option<String> {
        None
    }
    fn ws_routing(&self, exchange: &ExchangeName, options: &RoutingOptions) -> ResolvedRouting {
        DirectTransport.ws_routing(exchange, options)
    }
    fn ws_connection(&self, _: &str) -> Option<WsConnection> {
        None
    }
}

fn runtime(adapter: Option<Arc<Adapter>>) -> Rc<Runtime> {
    let inner = Rc::new(harness::Runtime::default());
    inner.now.set(1_700_000_000_000.0);
    Rc::new(Runtime {
        inner,
        transport: Arc::new(Provider {
            adapter,
            routes: Mutex::default(),
            selections: Mutex::default(),
        }),
    })
}
fn construct(
    name: ExchangeName,
    tunnel: bool,
    runtime: Rc<Runtime>,
    http: Rc<dyn JsHttpHost>,
) -> Result<Rc<CoinbaseExchange>, ExchangeError> {
    let mut options = harness::options();
    options.tealstreet_tunnel = Some(tunnel);
    CoinbaseExchange::new(
        name,
        options,
        Rc::new(safe_cex_exchange::native_store::DefaultStore::new()),
        CoinbaseDependencies {
            runtime,
            http,
            external_public_data: None,
            signing_cache: Rc::new(Default::default()),
            local_wall_time_to_utc: Rc::new(|millis| millis),
        },
    )
}
fn drive<T>(runtime: &Runtime, mut future: ExchangeFuture<'_, T>) -> Result<T, ExchangeError> {
    for _ in 0..2000 {
        if let Poll::Ready(result) = future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
        {
            return result;
        }
        runtime.inner.pump();
        runtime.inner.advance(50.0);
    }
    panic!("real venue request did not settle")
}

fn portfolio() -> Value {
    json!({"portfolios":[{"name":"Synthetic tunnel portfolio","uuid":"portfolio-tunnel-fixture","type":"DEFAULT","deleted":false}]})
}

fn script(http: &harness::Http) {
    http.ok("GET", "/api/v3/brokerage/portfolios", portfolio());
    http.ok(
        "GET",
        "/api/v3/brokerage/portfolios/portfolio-tunnel-fixture",
        json!({}),
    );
    http.ok("GET", "/api/v3/brokerage/cfm/balance_summary", json!({}));
}

#[test]
fn real_coinbase_factory_uses_provider_routing() {
    for name in [ExchangeName::Coinbase, ExchangeName::Coinbasespot] {
        let runtime = runtime(None);
        let http = Rc::new(harness::Http::default());
        *http.runtime.borrow_mut() = Some(runtime.inner.clone());
        script(&http);
        let venue = construct(name.clone(), false, runtime.clone(), http.clone()).unwrap();
        let account = drive(&runtime, venue.get_account()).unwrap();
        assert_eq!(account.user_id, "portfolio-tunnel-fixture");
        let routes = runtime.transport.routes.lock().unwrap();
        assert!(
            !routes.is_empty(),
            "real Coinbase factory must consult provider routing"
        );
        assert!(
            routes
                .iter()
                .all(|route| route == &(name.clone(), Some(false)))
        );
        assert!(
            runtime.transport.selections.lock().unwrap().is_empty(),
            "source does not install adapter without tunnel"
        );
        assert_eq!(http.calls("/api/v3/brokerage/portfolios").len(), 1);
        let calls = http.calls("/api/v3/brokerage/portfolios");
        assert!(
            calls[0].headers["Coinbase-Authorization"]
                .as_str()
                .unwrap()
                .starts_with("Bearer eyJ")
        );
    }
}

#[test]
fn real_coinbase_tunnel_installs_adapter_or_retains_http_host() {
    for name in [ExchangeName::Coinbase, ExchangeName::Coinbasespot] {
        for available in [true, false] {
            let adapter = Arc::new(Adapter::default());
            let runtime = runtime(available.then(|| adapter.clone()));
            let http = Rc::new(harness::Http::default());
            *http.runtime.borrow_mut() = Some(runtime.inner.clone());
            script(&http);
            let venue = construct(name.clone(), true, runtime.clone(), http.clone()).unwrap();
            let failures = Rc::new(std::cell::RefCell::new(Vec::new()));
            let observed = failures.clone();
            venue.base().on(
                "error",
                Rc::new(move |_, value| {
                    observed.borrow_mut().push(value.clone());
                    Ok(())
                }),
            );
            let account = drive(&runtime, venue.get_account()).unwrap_or_else(|error| panic!("account failed {name:?} available={available}: {error:?}; urls={:?}; errors={:?}", adapter.0.lock().unwrap().iter().map(|r|r.url.clone()).collect::<Vec<_>>(), failures.borrow()));
            assert_eq!(account.user_id, "portfolio-tunnel-fixture");
            let selections = runtime.transport.selections.lock().unwrap();
            assert!(
                !selections.is_empty(),
                "real Coinbase tunnel must consult provider adapter"
            );
            assert!(
                selections
                    .iter()
                    .all(|s| s == &(name.clone(), Some(true), RestAdapterVariant::Default))
            );
            let sent = adapter.0.lock().unwrap();
            assert_eq!(
                sent.iter()
                    .filter(|r| r.url == "/api/v3/brokerage/portfolios")
                    .count(),
                usize::from(available),
                "real Coinbase tunnel uses selected adapter"
            );
            assert_eq!(
                http.calls("/api/v3/brokerage/portfolios").len(),
                usize::from(!available),
                "missing adapter retains original host"
            );
            if available {
                assert_eq!(sent[0].url, "/api/v3/brokerage/portfolios");
                assert_eq!(
                    sent[0].base_url.as_deref(),
                    Some("https://scripted-route.invalid")
                );
                assert_eq!(sent[0].timeout_ms, Some(20_000));
                let auth = sent[0]
                    .headers
                    .iter()
                    .find(|(key, _)| key.eq_ignore_ascii_case("Authorization"))
                    .unwrap();
                assert!(auth.1.starts_with("Bearer eyJ"));
                assert!(
                    !sent[0]
                        .headers
                        .keys()
                        .any(|key| key.eq_ignore_ascii_case("Coinbase-Authorization"))
                );
            } else {
                let calls = http.calls("/api/v3/brokerage/portfolios");
                assert!(
                    calls[0].headers["Authorization"]
                        .as_str()
                        .unwrap()
                        .starts_with("Bearer eyJ")
                );
            }
        }
    }
}

#[test]
fn maintained_source_executes_eight_transport_scenes() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/transport-binding-source.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rows: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(rows.as_array().unwrap().len(), 8);
}

#[test]
fn create_order_wire_places_leverage_before_configuration() {
    use safe_cex_store::{Store, StoreUpdate};
    for name in [ExchangeName::Coinbase, ExchangeName::Coinbasespot] {
        let adapter = Arc::new(Adapter::default());
        let runtime = runtime(Some(adapter.clone()));
        let http = Rc::new(harness::Http::default());
        *http.runtime.borrow_mut() = Some(runtime.inner.clone());
        let venue = construct(name, true, runtime.clone(), http).unwrap();
        Store::update(venue.base().store.as_ref(), StoreUpdate {
            markets: Some(vec![serde_json::from_value(json!({"id":"BTC-USDC","symbol":"BTC-USDC","base":"BTC","quote":"USDC","active":true,"precision":{"amount":0.001,"price":0.01},"limits":{"amount":{"min":0.001,"max":100},"leverage":{"min":1,"max":10}}})).unwrap()]),
            ..Default::default()
        }, false).unwrap();
        let opts: safe_cex_types::PlaceOrderOpts = serde_json::from_value(json!({"symbol":"BTC-USDC","type":"limit","side":"buy","amount":2.5,"price":123.45,"clientOrderId":"wire-client"})).unwrap();
        assert_eq!(
            drive(
                &runtime,
                Box::pin(venue.base().place_order(venue.as_ref(), opts))
            )
            .unwrap(),
            ["wire-order"]
        );
        let requests = adapter.0.lock().unwrap();
        let request = requests
            .iter()
            .find(|r| r.url == "/api/v3/brokerage/orders")
            .expect("production order POST reaches the adapter");
        assert_eq!(request.method.to_ascii_uppercase(), "POST");
        assert_eq!(
            String::from_utf8(request.body.as_ref().unwrap().wire_bytes().unwrap()).unwrap(),
            r#"{"client_order_id":"wire-client","product_id":"BTC-USDC","side":"BUY","leverage":"10","order_configuration":{"limit_limit_gtc":{"base_size":"2.5","limit_price":"123.45","post_only":false}}}"#,
            "real encoded body must retain source literal key order"
        );
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[path = "support/retained_info_native.rs"]
mod retained_info_native;
