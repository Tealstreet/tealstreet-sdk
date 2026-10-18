//! Real Bitget constructors, globals, shared client and Exchange methods; only I/O is scripted.
#[path = "support/binding_harness.rs"]
#[allow(dead_code)]
mod harness;

use safe_cex_exchange::{
    exchange::Exchange,
    js_runtime::{
        client::JsHttpHost,
        promise::JsFuture,
        value::{Js, Thrown, from_json},
    },
    runtime::{ExchangeError, ExchangeFuture, ExchangeRuntime, ExchangeTask},
    websocket::{WebSocketConnectionOptions, WsRuntime, WsSocket},
};
use safe_cex_transport::{socket::TunnelHost, *};
use safe_cex_types::{ExchangeName, ExchangeOptions};
use safe_cex_venue_bitget::{BitgetDependencies, BitgetExchange};
use serde_json::{Value, json};
use std::{
    cell::Cell,
    future::Future,
    pin::Pin,
    process::Command,
    rc::Rc,
    sync::{Arc, Mutex, OnceLock},
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
    fn console(&self, severity: &str, message: &str) {
        self.inner.console(severity, message);
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
    fn report_exception(&self, error: &ExchangeError, context: &Value) {
        self.inner.report_exception(error, context);
    }
    fn report_raw_exception(&self, error: &Value, context: &Value) {
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
        self.0.lock().unwrap().push(request);
        Box::pin(async {
            Ok(RestResponse {
                status: 200,
                reason: None,
                headers: Default::default(),
                body: RestResponseBody::Untransformed(
                    safe_cex_types::retained_json::RetainedJsonInput::from_value(
                        json!({"code":"00000","data":[],"requestTime":1}),
                    )
                    .unwrap(),
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
) -> Result<Rc<BitgetExchange>, ExchangeError> {
    let options = serde_json::from_value(
        json!({"key":"","secret":"","publicOnly":true,"tealstreetTunnel":tunnel}),
    )
    .unwrap();
    BitgetExchange::new(
        name,
        options,
        Rc::new(safe_cex_store::store::DefaultStore::new()),
        BitgetDependencies {
            runtime,
            http,
            external_public_data: None,
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
fn source() -> &'static Value {
    static SOURCE: OnceLock<Value> = OnceLock::new();
    SOURCE.get_or_init(|| {
        let output = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/scripts/transport-retry-source.cjs"
            ))
            .output()
            .expect("live TypeScript source process");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).unwrap()
    })
}

#[test]
fn real_tunnel_constructor_installs_provider_adapter_or_retains_host() {
    assert_eq!(source()["installs"].as_array().unwrap().len(), 3);
    for name in [
        ExchangeName::Bitget,
        ExchangeName::Bitgetusdc,
        ExchangeName::Bitgetinverse,
    ] {
        for available in [true, false] {
            let adapter = Arc::new(Adapter::default());
            let runtime = runtime(available.then(|| adapter.clone()));
            let http = Rc::new(harness::Http::default());
            *http.runtime.borrow_mut() = Some(runtime.inner.clone());
            http.ok("GET", "/api/v2/mix/market/contracts", json!([]));
            let venue = construct(name.clone(), true, runtime.clone(), http.clone());
            assert!(
                venue.is_ok(),
                "real tunnel constructor must succeed: {name:?}, adapter={available}: {:?}",
                venue.err()
            );
            let venue = venue.unwrap();
            assert!(drive(&runtime, venue.fetch_markets()).unwrap().is_empty());
            let routes = runtime.transport.routes.lock().unwrap();
            assert!(
                !routes.is_empty(),
                "real Axios factory must select provider routing"
            );
            assert!(
                routes
                    .iter()
                    .all(|route| route == &(name.clone(), Some(true)))
            );
            let selections = runtime.transport.selections.lock().unwrap();
            assert!(
                !selections.is_empty(),
                "real installer must select a provider adapter"
            );
            for (exchange, tunnel, variant) in selections.iter() {
                assert_eq!(exchange, &name);
                assert_eq!(*tunnel, Some(true));
                assert_eq!(*variant, RestAdapterVariant::Default);
            }
            let sent = adapter.0.lock().unwrap();
            assert_eq!(
                sent.is_empty(),
                !available,
                "real provider adapter receives HTTP"
            );
            assert_eq!(
                http.requests.borrow().is_empty(),
                available,
                "absent provider adapter leaves original HTTP host"
            );
            if available {
                assert!(
                    sent.iter()
                        .all(|request| request.url == "/api/v2/mix/market/contracts")
                );
                for request in sent.iter() {
                    assert_eq!(
                        request.base_url.as_deref(),
                        Some("https://scripted-route.invalid")
                    );
                    assert_eq!(request.timeout_ms, Some(20_000));
                }
            }
        }
    }
}

struct NetworkHttp {
    runtime: Rc<Runtime>,
    case: Value,
    attempts: Cell<usize>,
}
impl JsHttpHost for NetworkHttp {
    fn runtime(&self) -> Rc<dyn ExchangeRuntime> {
        self.runtime.clone()
    }
    fn send(&self, config: Js) -> JsFuture<'static> {
        let attempt = self.attempts.get();
        self.attempts.set(attempt + 1);
        let result = if attempt == 0 {
            let failed_config = config.spread_object().unwrap();
            failed_config
                .define("method", from_json(&self.case["method"]))
                .unwrap();
            let error = Js::object(vec![
                ("message", Js::str("synthetic network failure")),
                ("config", failed_config),
            ]);
            for field in ["code", "response"] {
                if let Some(value) = self.case.get(field) {
                    error.define(field, from_json(value)).unwrap();
                }
            }
            Err(Thrown::new(error))
        } else {
            Ok(Js::object(vec![
                ("status", Js::Number(200.0)),
                (
                    "data",
                    from_json(&json!({"code":"00000","data":[],"requestTime":1})),
                ),
            ]))
        };
        Box::pin(async move { result })
    }
}

#[test]
fn real_read_retry_matches_live_axios_exclusions_and_response_truthiness() {
    let cases = source()["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 54);
    for case in cases {
        assert_eq!(
            case["retry"], case["expected"],
            "independent source expectation"
        );
        let runtime = runtime(None);
        let http = Rc::new(NetworkHttp {
            runtime: runtime.clone(),
            case: case.clone(),
            attempts: Cell::new(0),
        });
        let venue = construct(ExchangeName::Bitget, false, runtime.clone(), http.clone()).unwrap();
        let _ = drive(&runtime, venue.fetch_markets());
        let expected = if case["retry"].as_bool().unwrap() {
            2
        } else {
            1
        };
        assert_eq!(
            http.attempts.get(),
            expected,
            "real network retry must match TS: {}",
            case["name"]
        );
    }
}
