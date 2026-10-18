#[cfg(feature = "health")]
use safe_cex_exchange::health::*;
use safe_cex_exchange::{http::*, proxy_token::*, runtime::*};
use safe_cex_transport::{
    PrivateProxy, ResolvedRouting, RestAdapter, RestAdapterContext, RestAuthorizer, RestRequest,
    RestResponse, RestTransport, RestTransportContext, RoutingOptions, TransportError,
    TransportFuture, TransportProvider, WsConnection,
};
use safe_cex_types::QueryValue as Q;
use safe_cex_types::{ExchangeName, ExchangeOptions};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    future::{Future, poll_fn},
    pin::Pin,
    rc::Rc,
    sync::{Arc, Mutex},
    task::{Context, Poll, Waker},
};
type Task = (Rc<Cell<bool>>, Pin<Box<dyn Future<Output = ()>>>);
#[derive(Default)]
struct Runtime {
    now: Rc<Cell<f64>>,
    tasks: Rc<RefCell<Vec<Task>>>,
    fail_sleep: Rc<Cell<bool>>,
    inline_spawn: Cell<bool>,
}
struct Handle(Rc<Cell<bool>>);
impl ExchangeTask for Handle {
    fn cancel(&self) {
        self.0.set(true);
    }
}
impl Runtime {
    fn tick(&self, now: f64) {
        self.now.set(now);
        for _ in 0..3 {
            let tasks = std::mem::take(&mut *self.tasks.borrow_mut());
            let waker = Waker::noop().clone();
            let mut cx = Context::from_waker(&waker);
            for (cancel, mut future) in tasks {
                if !cancel.get() && future.as_mut().poll(&mut cx).is_pending() {
                    self.tasks.borrow_mut().push((cancel, future));
                }
            }
        }
    }
}
impl ExchangeRuntime for Runtime {
    fn random(&self) -> f64 {
        0.5
    }
    fn report_raw_exception(
        &self,
        _: &safe_cex_exchange::js_runtime::value::Js,
        _: &serde_json::Value,
    ) {
        panic!("unexpected raw exception report in this test runtime");
    }
    fn report_exception(&self, _: &ExchangeError, _: &serde_json::Value) {}
    fn execute_worker_request(
        &self,
        _: serde_json::Value,
    ) -> ExchangeFuture<'static, serde_json::Value> {
        Box::pin(async { Ok(serde_json::Value::Null) })
    }

    fn monotonic_now_ms(&self) -> f64 {
        self.now.get()
    }
    fn now_ms(&self) -> f64 {
        self.now.get()
    }
    fn sleep(&self, delay: u64) -> ExchangeFuture<'static, ()> {
        let now = self.now.clone();
        let end = now.get() + delay as f64;
        let fail = self.fail_sleep.clone();
        Box::pin(poll_fn(move |_| {
            if fail.get() {
                Poll::Ready(Err(ExchangeError::new("sleep cancelled")))
            } else if now.get() >= end {
                Poll::Ready(Ok(()))
            } else {
                Poll::Pending
            }
        }))
    }
    fn spawn_local(&self, mut future: Pin<Box<dyn Future<Output = ()>>>) -> Box<dyn ExchangeTask> {
        let canceled = Rc::new(Cell::new(false));
        if self.inline_spawn.get()
            && future
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop()))
                .is_ready()
        {
            return Box::new(Handle(canceled));
        }
        self.tasks.borrow_mut().push((canceled.clone(), future));
        Box::new(Handle(canceled))
    }
    fn console_values(&self, _: &str, _: &[safe_cex_exchange::js_runtime::value::Js]) {}
    fn unique_id(&self) -> String {
        "id".into()
    }
    fn is_worker(&self) -> bool {
        false
    }
}
fn run<F: Future>(future: F) -> F::Output {
    let waker = Waker::noop().clone();
    let mut cx = Context::from_waker(&waker);
    let mut future = std::pin::pin!(future);
    match future.as_mut().poll(&mut cx) {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("pending"),
    }
}
#[cfg(feature = "health")]
#[test]
fn health_order_severity_timer_and_latched_counts() {
    let runtime = Rc::new(Runtime::default());
    runtime.tick(100000.);
    let monitor = ExchangeHealthMonitor::new(
        "bybit",
        ExchangeHealthMonitorOptions::default(),
        runtime.clone(),
    );
    assert_eq!(monitor.process_message("order price 10005.5", false), None);
    assert_eq!(
        monitor.process_message("retCode=10010", true),
        Some(ErrorCategory::AuthPermissions)
    );
    assert!(monitor.has_auth_error());
    monitor.reset();
    assert_eq!(
        monitor.process_message("url is not allowed", true),
        Some(ErrorCategory::UnsupportedBackend)
    );
    runtime.tick(130000.);
    monitor.process_message("rate limit", true);
    runtime.tick(160000.);
    assert!(monitor.is_healthy());
    assert!(monitor.has_error(ErrorCategory::RateLimit));
    monitor.process_message("invalid api key", false);
    runtime.tick(300000.);
    assert_eq!(monitor.get_state().status, HealthStatus::Error);
    assert_eq!(
        monitor.process_message("Remote WS signing unavailable: disconnected", true),
        Some(ErrorCategory::Unknown)
    );
    assert!(monitor.has_auth_error());
}
#[cfg(feature = "health")]
#[test]
fn health_first_severity_and_suppression() {
    let first = ErrorPattern {
        category: ErrorCategory::AuthInvalid,
        severity: ErrorSeverity::Info,
        patterns: vec![ErrorMatcher::Text("a".into())],
    };
    let second = ErrorPattern {
        category: ErrorCategory::AuthInvalid,
        severity: ErrorSeverity::Critical,
        patterns: vec![ErrorMatcher::Text("b".into())],
    };
    let merged = merge_patterns(&[&[first], &[second]]);
    assert_eq!(merged[0].severity, ErrorSeverity::Info);
    assert_eq!(merged[0].patterns.len(), 2);
    let monitor = ExchangeHealthMonitor::new(
        "bitunix",
        ExchangeHealthMonitorOptions {
            max_recent_errors: Some(0),
            ..Default::default()
        },
        Rc::new(Runtime::default()),
    );
    assert_eq!(
        monitor.process_message("missing positionId invalid api key", true),
        Some(ErrorCategory::Suppressed)
    );
    assert!(monitor.is_healthy());
    assert!(monitor.get_state().recent_errors.is_empty());
}
#[test]
fn stats_inclusive_boundary_and_status_helpers() {
    let mut stats = HttpRequestStatsTracker::default();
    stats.record_request("bybit", 100000.);
    assert_eq!(stats.get_requests_per_minute(160000.), 1);
    assert_eq!(stats.get_requests_per_minute(160001.), 0);
    stats.record_error();
    assert_eq!(
        stats.get_stats(160001.),
        serde_json::json!({"perMinute":0,"total":1,"errors":1})
    );
    assert!(is_tealstreet_proxy_auth_failure_body(
        &serde_json::json!({"message":"Invalid JWT"})
    ));
    assert!(!is_tealstreet_proxy_auth_failure_body(
        &serde_json::json!({"message":"invalid jwt"})
    ));
    assert_eq!(stringify_response_body(&serde_json::json!("text")), "text");
}
type BroadcastReceiver = Rc<dyn Fn(BroadcastMessage)>;
#[derive(Default)]
struct TokenHost {
    main: Cell<bool>,
    has_self: Cell<bool>,
    calls: Cell<u64>,
    broadcasts: RefCell<Vec<BroadcastMessage>>,
    receive: RefCell<Option<BroadcastReceiver>>,
    runtime: Rc<Runtime>,
    token: RefCell<String>,
}
impl ProxyTokenHost for TokenHost {
    fn is_main_window(&self) -> bool {
        self.main.get()
    }
    fn has_self(&self) -> bool {
        self.has_self.get()
    }
    fn origin(&self) -> Option<String> {
        Some("https://preview.tealstreet.io".into())
    }
    fn virtual_clock(&self) -> safe_cex_store::virtual_clock::VirtualClock {
        let now = self.runtime.now_ms();
        let mut clock = safe_cex_store::virtual_clock::VirtualClock::default();
        clock.now = std::sync::Arc::new(move || now);
        clock
    }
    fn initialize_broadcast(
        &self,
        channel: &str,
        receive: Rc<dyn Fn(BroadcastMessage)>,
    ) -> Result<bool, String> {
        assert_eq!(channel, "proxy-token-channel");
        *self.receive.borrow_mut() = Some(receive);
        Ok(true)
    }
    fn broadcast(&self, message: BroadcastMessage) -> Result<(), String> {
        self.broadcasts.borrow_mut().push(message);
        Ok(())
    }
    fn fetch<'a>(&'a self, url: String, credentials: bool) -> ExchangeFuture<'a, Option<String>> {
        assert!(credentials);
        assert_eq!(url, "https://preview.tealstreet.io/api/proxykey");
        self.calls.set(self.calls.get() + 1);
        Box::pin(async move {
            self.runtime.sleep(100).await?;
            Ok(Some(self.token.borrow().clone()))
        })
    }
}
fn jwt(exp: u64) -> String {
    use base64::Engine;
    format!(
        "e30.{}.sig",
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(format!("{{\"exp\":{exp}}}"))
    )
}
#[test]
fn token_singleflight_worker_fallback_refresh_and_silent_set() {
    let runtime = Rc::new(Runtime::default());
    runtime.tick(100000.);
    let host = Rc::new(TokenHost {
        has_self: Cell::new(true),
        runtime: runtime.clone(),
        token: RefCell::new(jwt(500)),
        ..Default::default()
    });
    let client = ProxyTokenClient::new(host.clone(), runtime.clone());
    let notifications = Rc::new(Cell::new(0));
    let count = notifications.clone();
    let unsubscribe = client.on_token_update(Rc::new(move |_| {
        count.set(count.get() + 1);
        Ok(())
    }));
    client.set_token(jwt(1));
    assert_eq!(notifications.get(), 0);
    let mut one = Box::pin(client.get_proxy_token());
    let mut two = client.fetch_proxy_token();
    let waker = Waker::noop().clone();
    let mut cx = Context::from_waker(&waker);
    assert!(one.as_mut().poll(&mut cx).is_pending());
    assert!(two.as_mut().poll(&mut cx).is_pending());
    runtime.tick(100000.);
    assert_eq!(host.calls.get(), 1);
    drop(two);
    runtime.tick(100100.);
    assert_eq!(one.as_mut().poll(&mut cx), Poll::Ready(jwt(500)));
    assert_eq!(notifications.get(), 1);
    assert!(matches!(
        host.broadcasts.borrow()[0],
        BroadcastMessage::ProxyTokenRequest
    ));
    unsubscribe();
    client.set_token(jwt(200));
    assert_eq!(notifications.get(), 1);
    let mut refresh = Box::pin(client.refresh_proxy_token());
    assert!(refresh.as_mut().poll(&mut cx).is_pending());
    runtime.tick(100100.);
    runtime.tick(100200.);
    assert_eq!(refresh.as_mut().poll(&mut cx), Poll::Ready(jwt(500)));
}
#[test]
fn token_native_and_expiry_boundary() {
    let runtime = Rc::new(Runtime::default());
    runtime.tick(100000.);
    let host = Rc::new(TokenHost {
        runtime: runtime.clone(),
        ..Default::default()
    });
    let client = ProxyTokenClient::new(host.clone(), runtime);
    client.set_token(jwt(100));
    assert_eq!(run(client.get_proxy_token()), jwt(100));
    assert_eq!(host.calls.get(), 0);
    assert_eq!(ProxyTokenClient::get_token_expiration("garbage"), 0.);
}
struct Adapter;
impl RestAdapter for Adapter {
    fn execute(&self, request: RestRequest) -> safe_cex_transport::RestFuture<'_, RestResponse> {
        Box::pin(async move {
            Err(HttpFailure::from_transport(
                TransportError::Host("unreachable".into()),
                request,
            ))
        })
    }
}
#[derive(Default)]
struct HttpTestHost {
    response_bodies: RefCell<Vec<safe_cex_transport::RestResponseBody>>,
    failure_bodies: RefCell<Vec<Option<safe_cex_transport::RestResponseBody>>>,
    response_configs: RefCell<Vec<RestRequest>>,
    signing_inputs: RefCell<Vec<serde_json::Value>>,
    sign_raw: Cell<bool>,
    signing_headers: RefCell<Vec<serde_json::Value>>,
    rewrite_header: Cell<bool>,
    sticky_timestamp: Cell<bool>,
    retry_rewrite: Cell<bool>,
    signing_timestamps: RefCell<Vec<String>>,
    drive_adapter: Cell<bool>,
    erase_provenance: Cell<bool>,
    transport_error: RefCell<Option<TransportError>>,
    events: Rc<RefCell<Vec<String>>>,
    status: Cell<u16>,
    capture: Cell<bool>,
}
impl HttpHost for HttpTestHost {
    fn execute(&self, adapter: Arc<dyn RestAdapter>, mut request: RestRequest) -> HttpFuture<'_> {
        if self.drive_adapter.get() {
            self.signing_headers
                .borrow_mut()
                .push(serde_json::to_value(&request.headers).unwrap());
            if self.rewrite_header.get() {
                request.headers.insert("ApiKey".into(), "signed".into());
            }
            if self.sign_raw.get() {
                match request
                    .body
                    .as_ref()
                    .map(safe_cex_transport::RestBody::untransformed)
                    .transpose()
                {
                    Ok(Some(value)) => self
                        .signing_inputs
                        .borrow_mut()
                        .push(value.clone().into_value().unwrap()),
                    Ok(None) => {}
                    Err(error) => {
                        return Box::pin(async move {
                            Err(HttpFailure::pre_transmit(error.to_string(), request))
                        });
                    }
                }
            }
            if self.sticky_timestamp.get() {
                let candidate = (999 + self.signing_inputs.borrow().len()).to_string();
                let timestamp = request
                    .headers
                    .entry("X-Time".into())
                    .or_insert(candidate)
                    .clone();
                self.signing_timestamps.borrow_mut().push(timestamp);
                if self.retry_rewrite.get() && self.signing_inputs.borrow().len() == 2 {
                    request.body = Some(
                        request
                            .body
                            .as_ref()
                            .unwrap()
                            .with_signed_data(
                                safe_cex_types::retained_json::RetainedJsonInput::from_value(
                                    serde_json::json!("second raw text"),
                                )
                                .unwrap(),
                            )
                            .unwrap(),
                    );
                }
            }
            return Box::pin(async move { adapter.execute_with_receipt(request).await });
        }
        self.events.borrow_mut().push("execute".into());
        if let Some(error) = self.transport_error.borrow().clone() {
            return Box::pin(async move {
                Err(HttpFailure {
                    code: None,
                    message: error.to_string(),
                    transport_error: Some(Box::new(error)),
                    response: None,
                    request: Box::new(request),
                    context: None,
                })
            });
        }
        let response = RestResponse {
            status: self.status.get(),
            reason: None,
            headers: BTreeMap::new(),
            body: safe_cex_transport::RestResponseBody::Encoded(
                b"{\"message\":\"Invalid JWT\"}".to_vec(),
            ),
        };
        Box::pin(async move {
            if response.status >= 400 {
                Err(HttpFailure {
                    code: None,
                    transport_error: None,
                    message: "failed".into(),
                    response: Some(Box::new(response)),
                    request: Box::new(request),
                    context: None,
                })
            } else {
                Ok(safe_cex_transport::RestExecution {
                    response,
                    config: request,
                })
            }
        })
    }
    fn enhance_error(&self, mut error: HttpFailure, _: &ExchangeName) -> HttpFailure {
        if self.erase_provenance.get() {
            error.transport_error = None;
        }
        error
    }
    fn emit(&self, event: &str, _: &str) -> Result<(), String> {
        self.events.borrow_mut().push(event.into());
        Ok(())
    }
    fn note_response(&self, response: &RestResponse, request: &RestRequest) {
        self.response_bodies
            .borrow_mut()
            .push(response.body.clone());
        self.response_configs.borrow_mut().push(request.clone());
        self.events.borrow_mut().push("response".into());
    }
    fn note_success(
        &self,
        _: &ExchangeName,
        _: &RestResponse,
        _request: &safe_cex_transport::RestRequest,
    ) {
        self.events.borrow_mut().push("success".into());
    }
    fn note_failure(&self, _: &ExchangeName, error: &HttpFailure) {
        self.failure_bodies.borrow_mut().push(
            error
                .response
                .as_ref()
                .map(|response| response.body.clone()),
        );
        self.events.borrow_mut().push("failure".into());
    }
    fn mark_start(&self, _: &mut RestRequest) {
        self.events.borrow_mut().push("start".into());
    }
    fn signing_capture(&self, _: &RestRequest) -> bool {
        self.capture.get()
    }
    fn is_development(&self) -> bool {
        true
    }
    fn is_main_window(&self) -> bool {
        true
    }
}
struct Authorizer(Rc<RefCell<Vec<String>>>);
impl RestAuthorizer for Authorizer {
    fn authorize<'a>(&'a self, _: &'a mut RestRequest) -> TransportFuture<'a, ()> {
        self.0.borrow_mut().push("authorize".into());
        Box::pin(async { Ok(()) })
    }
}
struct Provider(Rc<RefCell<Vec<String>>>);
#[expect(clippy::arc_with_non_send_sync)]
impl TransportProvider for Provider {
    fn name(&self) -> &str {
        "fake"
    }
    fn rest(&self, c: RestTransportContext<'_>) -> Result<RestTransport, TransportError> {
        Ok(RestTransport {
            base_url: c.config.base_url.clone(),
            proxied: true,
            authorize: Some(Arc::new(Authorizer(self.0.clone()))),
        })
    }
    fn rest_adapter(&self, _: RestAdapterContext<'_>) -> Option<Arc<dyn RestAdapter>> {
        None
    }
    fn proxy_prefix(&self, _: RestTransportContext<'_>) -> String {
        String::new()
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
    fn ws_routing(&self, e: &ExchangeName, o: &RoutingOptions) -> ResolvedRouting {
        safe_cex_transport::DirectTransport.ws_routing(e, o)
    }
    fn ws_connection(&self, _: &str) -> Option<WsConnection> {
        None
    }
}
fn http_options(
    host: Rc<HttpTestHost>,
    runtime: Rc<Runtime>,
    stats: Arc<Mutex<HttpRequestStatsTracker>>,
) -> HttpClientOptions {
    HttpClientOptions {
        response_transform: None,
        params_serializer: None,
        exchange: ExchangeName::Bybit,
        options: serde_json::from_value(serde_json::json!({"key":"k","secret":"s"})).unwrap(),
        request: RestRequest::default(),
        provider: None,
        signer: None,
        account_name: None,
        adapter: Arc::new(Adapter),
        host,
        runtime,
        store: None,
        stats: Some(stats),
        logs: None,
        every: vec![],
        periodic: vec![],
        next_id: Arc::new(|| "id".into()),
        signing_now: Arc::new(|| 0),
    }
}
#[test]
#[expect(clippy::arc_with_non_send_sync)]
fn http_request_order_capture_and_error_count_asymmetry() {
    let host = Rc::new(HttpTestHost {
        status: Cell::new(401),
        ..Default::default()
    });
    let runtime = Rc::new(Runtime::default());
    let stats = Arc::default();
    let mut options = http_options(host.clone(), runtime.clone(), stats);
    options.provider = Some(Arc::new(Provider(host.events.clone())));
    let client =
        safe_cex_axios_create(options, &safe_cex_transport::TransportRegistry::default()).unwrap();
    assert!(run(client.execute(RestRequest::default())).is_err());
    assert_eq!(
        &host.events.borrow()[..4],
        &["grafana-http-request", "authorize", "start", "execute"]
    );
    let stats = Arc::new(Mutex::new(HttpRequestStatsTracker::default()));
    host.capture.set(true);
    host.events.borrow_mut().clear();
    let client = safe_cex_axios_create_no_proxy(http_options(host.clone(), runtime, stats.clone()));
    assert!(run(client.execute(RestRequest::default())).is_err());
    assert!(!host.events.borrow().iter().any(|s| s == "failure"));
    assert_eq!(stats.lock().unwrap().get_stats(0.)["errors"], 1);
}

#[cfg(feature = "health")]
fn native_health_category(label: &str) -> ErrorCategory {
    match label {
        "auth_invalid" => ErrorCategory::AuthInvalid,
        "auth_expired" => ErrorCategory::AuthExpired,
        "auth_permissions" => ErrorCategory::AuthPermissions,
        "rate_limit" => ErrorCategory::RateLimit,
        "network" => ErrorCategory::Network,
        "exchange_down" => ErrorCategory::ExchangeDown,
        "time_sync" => ErrorCategory::TimeSync,
        "order_rejected" => ErrorCategory::OrderRejected,
        "order_warning" => ErrorCategory::OrderWarning,
        "unsupported_backend" => ErrorCategory::UnsupportedBackend,
        "suppressed" => ErrorCategory::Suppressed,
        "unknown" => ErrorCategory::Unknown,
        other => panic!("unreviewed source category {other}"),
    }
}
#[cfg(feature = "health")]
fn assert_native_health_entry(actual: &ErrorEntry, expected: &serde_json::Value) {
    assert_eq!(
        actual.category,
        native_health_category(expected["category"].as_str().unwrap()),
        "native health entry category identity"
    );
    let severity = match expected["severity"].as_str().unwrap() {
        "critical" => ErrorSeverity::Critical,
        "warning" => ErrorSeverity::Warning,
        "info" => ErrorSeverity::Info,
        other => panic!("unreviewed source severity {other}"),
    };
    assert_eq!(actual.severity, severity, "native health severity identity");
    assert_eq!(
        actual.message,
        expected["message"].as_str().unwrap(),
        "native health message identity"
    );
    assert_eq!(
        actual.timestamp, 100000.0,
        "native health timestamp identity"
    );
}
#[cfg(feature = "health")]
#[test]
fn health_generated_typescript_oracle() {
    let cases: serde_json::Value =
        serde_json::from_str(include_str!("health_oracle.json")).unwrap();
    let rows = cases["cases"].as_array().unwrap();
    assert_eq!(rows.len(), 686, "HEALTH-CARDINALITY source cases");
    assert_eq!(
        rows.iter()
            .map(|row| row["state"]["recentErrors"].as_array().unwrap().len())
            .sum::<usize>(),
        672,
        "HEALTH-CARDINALITY source total recent errors"
    );
    for case in rows {
        let runtime = Rc::new(Runtime::default());
        runtime.tick(100000.);
        let monitor = ExchangeHealthMonitor::new(
            case["exchange"].as_str().unwrap(),
            ExchangeHealthMonitorOptions::default(),
            runtime,
        );
        let category = monitor.process_message(case["message"].as_str().unwrap(), true);
        assert_eq!(
            category,
            case["category"].as_str().map(native_health_category),
            "native health category identity"
        );
        let native = monitor.get_state();
        let status = match case["state"]["status"].as_str().unwrap() {
            "healthy" => HealthStatus::Healthy,
            "degraded" => HealthStatus::Degraded,
            "error" => HealthStatus::Error,
            other => panic!("unreviewed source status {other}"),
        };
        assert_eq!(native.status, status, "native health status identity");
        assert_eq!(
            native.last_updated, 100000.0,
            "native health lastUpdated identity"
        );
        assert_eq!(
            native.last_error.is_some(),
            case["state"].get("lastError").is_some()
        );
        if let Some(entry) = &native.last_error {
            assert_native_health_entry(entry, &case["state"]["lastError"]);
        }
        let recent = case["state"]["recentErrors"].as_array().unwrap();
        assert_eq!(
            recent.len(),
            usize::from(case["category"].is_string()),
            "HEALTH-CARDINALITY source per-case recent errors"
        );
        assert_eq!(
            native.recent_errors.len(),
            recent.len(),
            "HEALTH-CARDINALITY native recent errors"
        );
        for (entry, expected) in native.recent_errors.iter().zip(recent) {
            assert_native_health_entry(entry, expected);
        }
        let counts = case["state"]["errorCounts"].as_object().unwrap();
        assert_eq!(native.error_counts.len(), counts.len());
        for (label, count) in counts {
            let category = native_health_category(label);
            assert_eq!(
                native
                    .error_counts
                    .iter()
                    .find(|(key, _)| *key == category)
                    .map(|(_, value)| *value),
                count.as_u64(),
                "native health count identity: {label}"
            );
        }

        assert_eq!(
            serde_json::to_value(category).unwrap(),
            case["category"],
            "{case}"
        );
        let mut expected = case["state"].clone();
        expected["lastUpdated"] = serde_json::json!(100000.0);
        if expected.get("lastError").is_some() {
            expected["lastError"]["timestamp"] = serde_json::json!(100000.0);
        }
        for entry in expected["recentErrors"].as_array_mut().unwrap() {
            entry["timestamp"] = serde_json::json!(100000.0);
        }
        assert_eq!(
            serde_json::to_value(monitor.get_state()).unwrap(),
            expected,
            "{case}"
        );
        assert_eq!(
            monitor.get_summary(),
            case["summary"].as_str().unwrap(),
            "{case}"
        );
    }
}

#[test]
fn http_factories_default_to_global_stats() {
    let host = Rc::new(HttpTestHost {
        status: Cell::new(200),
        ..Default::default()
    });
    let runtime = Rc::new(Runtime::default());
    let mut options = http_options(host, runtime, Arc::default());
    options.stats = None;
    let before = HTTP_STATS.lock().unwrap().get_stats(0.)["total"]
        .as_u64()
        .unwrap();
    let client = safe_cex_axios_create_no_proxy(options);
    assert!(run(client.execute(RestRequest::default())).is_ok());
    assert_eq!(
        HTTP_STATS.lock().unwrap().get_stats(0.)["total"]
            .as_u64()
            .unwrap(),
        before + 1
    );
}

#[test]
fn token_fetch_callbacks_read_live_cache_but_broadcast_callbacks_use_payload() {
    let runtime = Rc::new(Runtime::default());
    runtime.tick(100000.);
    let host = Rc::new(TokenHost {
        main: Cell::new(true),
        runtime: runtime.clone(),
        token: RefCell::new(jwt(500)),
        ..Default::default()
    });
    let client = ProxyTokenClient::new(host.clone(), runtime.clone());
    let reentrant = client.clone();
    let remove_first = client.on_token_update(Rc::new(move |_| {
        reentrant.set_token(jwt(700));
        Ok(())
    }));
    let received = Rc::new(RefCell::new(Vec::new()));
    let captured = received.clone();
    let remove_second = client.on_token_update(Rc::new(move |token| {
        captured.borrow_mut().push(token.to_owned());
        Ok(())
    }));
    let mut request = client.fetch_proxy_token();
    let mut context = Context::from_waker(Waker::noop());
    assert!(request.as_mut().poll(&mut context).is_pending());
    runtime.tick(100000.);
    runtime.tick(100100.);
    assert_eq!(
        request.as_mut().poll(&mut context),
        Poll::Ready(Ok(jwt(700)))
    );
    assert_eq!(received.borrow()[0], jwt(700));
    let receive = host.receive.borrow().clone().unwrap();
    receive(BroadcastMessage::ProxyTokenUpdate {
        token: jwt(900),
        expires_at: 900000.,
    });
    assert_eq!(received.borrow()[1], jwt(900));
    remove_first();
    remove_second();
}

#[test]
fn dropping_proxy_fetch_host_task_settles_waiters_and_allows_retry() {
    for poll_task in [false, true] {
        for cached in [String::new(), jwt(1)] {
            let runtime = Rc::new(Runtime::default());
            runtime.tick(100000.);
            let host = Rc::new(TokenHost {
                runtime: runtime.clone(),
                token: RefCell::new(jwt(500)),
                ..Default::default()
            });
            let client = ProxyTokenClient::new(host.clone(), runtime.clone());
            client.set_token(cached.clone());
            let mut first = client.fetch_proxy_token();
            let mut second = client.fetch_proxy_token();
            let mut cx = Context::from_waker(Waker::noop());
            assert!(first.as_mut().poll(&mut cx).is_pending());
            assert!(second.as_mut().poll(&mut cx).is_pending());
            if poll_task {
                runtime.tick(100000.);
            }
            let discarded = std::mem::take(&mut *runtime.tasks.borrow_mut());
            drop(discarded);
            assert_eq!(run(first).unwrap(), cached);
            assert_eq!(run(second).unwrap(), cached);
            runtime.tick(130000.);
            let retry = client.fetch_proxy_token();
            runtime.tick(130000.);
            runtime.tick(130100.);
            assert_eq!(run(retry).unwrap(), jwt(500));
        }
    }
}

#[test]
fn http_failure_preserves_transport_provenance_without_guessing_from_missing_response() {
    let host = Rc::new(HttpTestHost::default());
    host.erase_provenance.set(true);
    let runtime = Rc::new(Runtime::default());
    let client =
        safe_cex_axios_create_no_proxy(http_options(host.clone(), runtime, Arc::default()));
    for error in [
        TransportError::Network(safe_cex_transport::NetworkFailure::from_code(
            "first reset".into(),
            Some("ECONNRESET".into()),
        )),
        TransportError::Network(safe_cex_transport::NetworkFailure::from_code(
            "last reset".into(),
            Some("ECONNRESET".into()),
        )),
        TransportError::Network(safe_cex_transport::NetworkFailure::from_code(
            "not retryable".into(),
            Some("ECONNABORTED".into()),
        )),
        TransportError::Timeout,
        TransportError::Host("signing failed before any request".into()),
    ] {
        *host.transport_error.borrow_mut() = Some(error.clone());
        let actual = run(client.execute(RestRequest::default())).unwrap_err();
        assert!(actual.response.is_none());
        assert_eq!(actual.message, error.to_string());
        assert_eq!(actual.transport_error.as_deref(), Some(&error));
    }
    *host.transport_error.borrow_mut() = None;
    host.status.set(401);
    let actual = run(client.execute(RestRequest::default())).unwrap_err();
    assert!(actual.response.is_some());
    assert_eq!(actual.transport_error, None);
}

#[derive(Default)]
struct WireRecorder(Mutex<Vec<RestRequest>>);
impl RestAdapter for WireRecorder {
    fn execute(&self, request: RestRequest) -> safe_cex_transport::RestFuture<'_, RestResponse> {
        self.0.lock().unwrap().push(request);
        Box::pin(async {
            Ok(RestResponse {
                status: 200,
                reason: None,
                headers: BTreeMap::new(),
                body: safe_cex_transport::RestResponseBody::Encoded(vec![]),
            })
        })
    }
}
struct TunnelProvider(Arc<WireRecorder>);
impl TransportProvider for TunnelProvider {
    fn name(&self) -> &str {
        "recorder-tunnel"
    }
    fn rest(&self, c: RestTransportContext<'_>) -> Result<RestTransport, TransportError> {
        safe_cex_transport::DirectTransport.rest(c)
    }
    fn rest_adapter(&self, _: RestAdapterContext<'_>) -> Option<Arc<dyn RestAdapter>> {
        Some(self.0.clone())
    }
    fn proxy_prefix(&self, _: RestTransportContext<'_>) -> String {
        String::new()
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
    fn ws_routing(&self, e: &ExchangeName, o: &RoutingOptions) -> ResolvedRouting {
        safe_cex_transport::DirectTransport.ws_routing(e, o)
    }
    fn ws_connection(&self, _: &str) -> Option<WsConnection> {
        None
    }
}
struct HedgeSigner {
    baked: bool,
    calls: Arc<Mutex<Vec<String>>>,
}
impl safe_cex_transport::signing::RemoteSigner for HedgeSigner {
    fn sign(
        &self,
        request: safe_cex_transport::signing::RemoteSignerRequest,
    ) -> TransportFuture<'_, safe_cex_transport::signing::RemoteSignerResponse> {
        let safe_cex_transport::signing::RemoteSignerRequest::Http(request) = request else {
            return Box::pin(async { Err(TransportError::Host("WS auth unsupported".into())) });
        };
        assert_eq!(
            request.config.data,
            Some(
                safe_cex_types::retained_json::RetainedJsonInput::from_value(
                    serde_json::json!({"hedge":true})
                )
                .unwrap()
            ),
            "signer must see original body"
        );
        assert_eq!(
            request.config.params,
            Some(serde_json::json!({"unsigned":true})),
            "serialize must follow signing"
        );
        self.calls.lock().unwrap().push("sign".into());
        let config = safe_cex_transport::signing::SignedConfig {
            url: Some(
                if self.baked {
                    "/order?ids%5B%5D=A%20B&ids%5B%5D=C&signature=fixture"
                } else {
                    "/order#discard"
                }
                .into(),
            ),
            params: if self.baked {
                None
            } else {
                Some(serde_json::json!({"signed":"A B"}))
            },
            data: None,
            headers: Default::default(),
        };
        Box::pin(async move {
            Ok(safe_cex_transport::signing::RemoteSignerResponse::Http(
                safe_cex_transport::signing::HttpSigningResponse {
                    id: request.id,
                    config: Some(config),
                    error: None,
                },
            ))
        })
    }
}
#[test]
fn remote_serializer_after_tunnel_preserves_hedge_body_omission_and_baked_bytes() {
    for baked in [false, true] {
        let calls = Arc::new(Mutex::new(vec![]));
        let host = Rc::new(HttpTestHost {
            drive_adapter: Cell::new(true),
            ..Default::default()
        });
        let mut options = http_options(host, Rc::default(), Arc::default());
        options.signer = Some(Arc::new(HedgeSigner {
            baked,
            calls: calls.clone(),
        }));
        let serializer_calls = calls.clone();
        options.params_serializer = Some(Arc::new(move |params| {
            assert_eq!(
                params,
                &serde_json::json!({"signed":"A B"}),
                "serialize the signed params exactly once"
            );
            serializer_calls.lock().unwrap().push("serialize".into());
            Ok("signed=A%20B&signature=fixture".into())
        }));
        let account = options.options.clone();
        let client = safe_cex_axios_create_no_proxy(options);
        let wire = Arc::new(WireRecorder::default());
        let provider: Arc<dyn TransportProvider> = Arc::new(TunnelProvider(wire.clone()));
        install_transport_adapter(
            &client,
            &Default::default(),
            &account,
            Some(&provider),
            safe_cex_transport::RestAdapterVariant::Default,
        );
        run(client.execute(RestRequest {
            method: "POST".into(),
            url: "/unsigned".into(),
            params: Some(serde_json::json!({"unsigned":true})),
            body: Some(safe_cex_transport::RestBody::Untransformed(
                safe_cex_types::retained_json::RetainedJsonInput::from_value(
                    serde_json::json!({"hedge":true}),
                )
                .unwrap(),
            )),
            ..Default::default()
        }))
        .unwrap();
        let sent = wire.0.lock().unwrap();
        assert_eq!(sent.len(), 1, "one tunnel send");
        assert_eq!(
            sent[0].body, None,
            "signed body omission must remain authoritative"
        );
        assert_eq!(
            sent[0].params, None,
            "host cannot reserialize consumed params"
        );
        assert_eq!(
            sent[0].url,
            if baked {
                "/order?ids%5B%5D=A%20B&ids%5B%5D=C&signature=fixture"
            } else {
                "/order?signed=A%20B&signature=fixture"
            },
            "wire query must equal signer encoding"
        );
        assert_eq!(
            *calls.lock().unwrap(),
            if baked {
                vec!["sign"]
            } else {
                vec!["sign", "serialize"]
            }
        );
    }
}
#[test]
fn serializer_separator_fragment_empty_and_refusal() {
    for (url, query, expected) in [
        ("/path#fragment", "a=1", "/path?a=1"),
        ("/path?old=2#fragment", "a=1", "/path?old=2&a=1"),
        ("/path?#fragment", "a=1", "/path?&a=1"),
        ("/path?old=2#fragment", "", "/path?old=2#fragment"),
    ] {
        let host = Rc::new(HttpTestHost {
            drive_adapter: Cell::new(true),
            ..Default::default()
        });
        let mut options = http_options(host, Rc::default(), Arc::default());
        let wire = Arc::new(WireRecorder::default());
        options.adapter = wire.clone();
        options.params_serializer = Some(Arc::new(move |_| Ok(query.into())));
        let client = safe_cex_axios_create_no_proxy(options);
        run(client.execute(RestRequest {
            url: url.into(),
            params: Some(serde_json::json!({})),
            ..Default::default()
        }))
        .unwrap();
        assert_eq!(wire.0.lock().unwrap()[0].url, expected);
    }
    let host = Rc::new(HttpTestHost {
        drive_adapter: Cell::new(true),
        ..Default::default()
    });
    let mut options = http_options(host, Rc::default(), Arc::default());
    options.params_serializer = Some(Arc::new(|_| {
        Err(TransportError::Network(
            safe_cex_transport::NetworkFailure::from_code(
                "codec refusal".into(),
                Some("ECONNRESET".into()),
            ),
        ))
    }));
    let account = options.options.clone();
    let client = safe_cex_axios_create_no_proxy(options);
    let wire = Arc::new(WireRecorder::default());
    let provider: Arc<dyn TransportProvider> = Arc::new(TunnelProvider(wire.clone()));
    install_transport_adapter(
        &client,
        &Default::default(),
        &account,
        Some(&provider),
        safe_cex_transport::RestAdapterVariant::Default,
    );
    let result = run(client.execute(RestRequest {
        params: Some(serde_json::json!({"x":1})),
        ..Default::default()
    }));
    assert!(
        wire.0.lock().unwrap().is_empty(),
        "serializer refusal must produce zero recorder sends"
    );
    let failure = result.unwrap_err();
    assert_eq!(failure.message, "codec refusal");
    assert_eq!(
        failure.transport_error.as_deref(),
        None,
        "pre-transmit refusal cannot inherit network retry provenance"
    );
}

#[cfg(feature = "health")]
#[test]
fn health_discarded_tasks_recover_on_original_deadline_without_early_health() {
    for mode in ["unpolled", "pending", "sleep-error", "inline-error"] {
        let runtime = Rc::new(Runtime::default());
        runtime.now.set(100.0);
        runtime.fail_sleep.set(mode.ends_with("error"));
        runtime.inline_spawn.set(mode == "inline-error");
        let monitor = ExchangeHealthMonitor::new(
            "bybit",
            ExchangeHealthMonitorOptions {
                recovery_window_ms: Some(1000),
                ..Default::default()
            },
            runtime.clone(),
        );
        monitor.process_message("rate limit", true);
        if mode == "pending" || mode == "sleep-error" {
            runtime.tick(100.0);
        }
        let tasks = std::mem::take(&mut *runtime.tasks.borrow_mut());
        drop(tasks);
        runtime.fail_sleep.set(false);
        runtime.inline_spawn.set(false);
        runtime.now.set(1099.0);
        assert_eq!(
            monitor.get_state().status,
            HealthStatus::Degraded,
            "early recovery after {mode}"
        );
        monitor.process_message("rate limit", true);
        runtime.now.set(1100.0);
        assert!(monitor.is_healthy(), "orphaned recovery stuck after {mode}");
        assert_eq!(monitor.get_state().last_updated, 1100.0);
        monitor.process_message("rate limit", true);
        assert_eq!(
            runtime.tasks.borrow().len(),
            1,
            "new warning starts a fresh recovery"
        );
    }
}
#[cfg(feature = "health")]
#[test]
fn health_stale_task_discard_cannot_revive_critical_reset_or_disposed_state() {
    for action in ["critical", "reset", "dispose", "replace"] {
        let runtime = Rc::new(Runtime::default());
        let monitor = ExchangeHealthMonitor::new(
            "bybit",
            ExchangeHealthMonitorOptions {
                recovery_window_ms: Some(1000),
                ..Default::default()
            },
            runtime.clone(),
        );
        monitor.process_message("rate limit", true);
        let old = std::mem::take(&mut *runtime.tasks.borrow_mut());
        runtime.now.set(500.0);
        match action {
            "critical" => {
                monitor.process_message("invalid api key", true);
            }
            "reset" => monitor.reset(),
            "dispose" => monitor.dispose(),
            _ => monitor.set_status(HealthStatus::Degraded),
        }
        drop(old);
        runtime.now.set(1000.0);
        let expected = match action {
            "critical" => HealthStatus::Error,
            "reset" => HealthStatus::Healthy,
            _ => HealthStatus::Degraded,
        };
        assert_eq!(
            monitor.get_state().status,
            expected,
            "stale recovery changed {action}"
        );
        if action == "replace" {
            runtime.tick(1500.0);
            assert!(monitor.is_healthy());
        }
    }
}

#[cfg(feature = "health")]
#[test]
fn health_orphan_observation_preserves_live_timer_and_new_warning_windows() {
    for observation in ["message", "summary"] {
        let runtime = Rc::new(Runtime::default());
        let monitor = ExchangeHealthMonitor::new(
            "bybit",
            ExchangeHealthMonitorOptions {
                recovery_window_ms: Some(1000),
                ..Default::default()
            },
            runtime.clone(),
        );
        monitor.process_message("rate limit", true);
        runtime.now.set(1001.0);
        assert!(
            !monitor.is_healthy(),
            "live unpolled timers remain host-owned"
        );
        let tasks = std::mem::take(&mut *runtime.tasks.borrow_mut());
        drop(tasks);
        if observation == "message" {
            monitor.process_message("rate limit", true);
            assert_eq!(
                runtime.tasks.borrow().len(),
                1,
                "expired orphan must start a new warning window"
            );
            assert_eq!(monitor.get_state().status, HealthStatus::Degraded);
            runtime.tick(2000.0);
            assert!(!monitor.is_healthy());
            runtime.tick(2001.0);
            assert!(monitor.is_healthy());
        } else {
            assert_eq!(
                monitor.get_summary(),
                "OK",
                "summary must reconcile orphan recovery"
            );
            assert_eq!(monitor.get_state().last_updated, 1001.0);
        }
    }
}

struct PanicOnSend;
impl RestAdapter for PanicOnSend {
    fn execute(&self, _: RestRequest) -> safe_cex_transport::RestFuture<'_, RestResponse> {
        panic!("pre-transmit refusal reached transport")
    }
}
#[test]
fn preflight_and_serializer_refusals_preserve_distinct_source_stats_without_network_retry() {
    let host = Rc::new(HttpTestHost {
        drive_adapter: Cell::new(true),
        ..Default::default()
    });
    let stats = Arc::new(Mutex::new(HttpRequestStatsTracker::default()));
    let mut options = http_options(host, Rc::new(Runtime::default()), stats.clone());
    options.adapter = Arc::new(PanicOnSend);
    options.params_serializer = Some(Arc::new(|_| {
        Err(TransportError::Network(
            safe_cex_transport::NetworkFailure::from_code(
                "codec refused".into(),
                Some("ECONNRESET".into()),
            ),
        ))
    }));
    let client = safe_cex_axios_create_no_proxy(options);
    let preflight = client
        .refuse_before_transmit(RestRequest::default(), "signer refused".into())
        .unwrap_err();
    assert_eq!(
        stats.lock().unwrap().get_stats(0.)["total"],
        0,
        "signer rejects before common stats"
    );
    let serializer = run(client.execute(RestRequest {
        params: Some(serde_json::json!({"x":1})),
        ..Default::default()
    }))
    .unwrap_err();
    assert_eq!(
        stats.lock().unwrap().get_stats(0.)["total"],
        1,
        "serializer runs in adapter after common stats"
    );
    for error in [preflight, serializer] {
        assert_eq!(
            error.transport_error, None,
            "refusal has no network provenance"
        );
        assert!(
            !error.is_retryable_network(),
            "refusal is not a network retry"
        );
    }
}

struct StructuredRejection;
impl RestAdapter for StructuredRejection {
    fn execute(&self, request: RestRequest) -> safe_cex_transport::RestFuture<'_, RestResponse> {
        Box::pin(async move {
            Err(HttpFailure {
                code: Some("ECONNABORTED".into()),
                message: "terminal attempt venue rejection".into(),
                transport_error: None,
                response: Some(Box::new(RestResponse {
                    status: 400,
                    reason: Some("fixture reason".into()),
                    headers: BTreeMap::from([("x-id".into(), "response-id".into())]),
                    body: safe_cex_transport::RestResponseBody::Encoded(
                        br#"{"data":[{"sCode":"51603","sMsg":"Order does not exist"}]}"#.to_vec(),
                    ),
                })),
                request: Box::new(request),
                context: Some(Box::new(serde_json::json!({"attempt":4}))),
            })
        })
    }
}
#[test]
#[expect(clippy::arc_with_non_send_sync)]
fn secondary_http_client_retains_structured_venue_rejection_without_conversion() {
    let host = Rc::new(HttpTestHost {
        drive_adapter: Cell::new(true),
        ..Default::default()
    });
    let clients = safe_cex_transport::secondary::create_api_with_secondary_key(
        &serde_json::from_value(serde_json::json!({"key":"primary","secret":"fixture","secondary_key":"secondary","secondary_secret":"fixture2"}))
            .unwrap(),
        &safe_cex_transport::DirectTransport,
        None,
        |_| {
            let mut options =
                http_options(host.clone(), Rc::new(Runtime::default()), Arc::default());
            options.adapter = Arc::new(StructuredRejection);
            Ok(Arc::new(safe_cex_axios_create_no_proxy(options)))
        },
        None,
        None,
    )
    .unwrap();
    let failure = run(clients.get(RestRequest {
        url: "/lookup".into(),
        ..Default::default()
    }))
    .unwrap_err();
    assert_eq!(failure.message, "terminal attempt venue rejection");
    assert_eq!(
        failure.code.as_deref(),
        Some("ECONNABORTED"),
        "secondary boundary must retain general error code"
    );
    assert_eq!(failure.request.url, "/lookup");
    assert_eq!(failure.request.method, "get");
    assert_eq!(
        failure.context.as_deref(),
        Some(&serde_json::json!({"attempt":4}))
    );
    let response = failure
        .response
        .expect("secondary client must retain venue response");
    assert_eq!(response.status, 400);
    assert_eq!(response.reason.as_deref(), Some("fixture reason"));
    assert_eq!(response.headers["x-id"], "response-id");
    assert_eq!(
        response.body.json_value().unwrap()["data"][0]["sCode"],
        "51603"
    );
}

#[test]
#[expect(clippy::arc_with_non_send_sync)]
fn structured_body_survives_http_and_secondary_selection_before_wire_transform() {
    use safe_cex_transport::{RestBody, secondary::create_api_with_secondary_key};
    let host = Rc::new(HttpTestHost {
        drive_adapter: Cell::new(true),
        sign_raw: Cell::new(true),
        ..Default::default()
    });
    let wire = Arc::new(WireRecorder::default());
    let account: ExchangeOptions = serde_json::from_value(serde_json::json!({
        "key":"primary", "secret":"secret", "secondary_key":"secondary"
    }))
    .unwrap();
    let clients = create_api_with_secondary_key(
        &account,
        &safe_cex_transport::DirectTransport,
        None,
        |_| {
            let mut options = http_options(host.clone(), Rc::default(), Arc::default());
            options.adapter = wire.clone();
            Ok(Arc::new(safe_cex_axios_create_no_proxy(options)))
        },
        None,
        None,
    )
    .unwrap();
    assert!(!clients.shares_primary_adapter());
    for secondary in [false, true] {
        for value in [serde_json::json!({"x":1}), serde_json::json!("{\"x\":1}")] {
            let request = RestRequest {
                method: "POST".into(),
                body: Some(RestBody::Untransformed(
                    safe_cex_types::retained_json::RetainedJsonInput::from_value(value).unwrap(),
                )),
                ..Default::default()
            };
            if secondary {
                run(clients.get(request))
                    .expect("secondary signing must receive untransformed data");
            } else {
                run(clients.request(request))
                    .expect("primary signing must receive untransformed data");
            }
        }
    }
    assert_eq!(
        *host.signing_inputs.borrow(),
        vec![
            serde_json::json!({"x":1}),
            serde_json::json!("{\"x\":1}"),
            serde_json::json!({"x":1}),
            serde_json::json!("{\"x\":1}"),
        ],
        "signing must retain String versus Object through both client selections"
    );
    let sent = wire.0.lock().unwrap();
    assert_eq!(sent.len(), 4);
    for request in sent.iter() {
        assert_eq!(
            request.body,
            Some(RestBody::Encoded(br#"{"x":1}"#.to_vec())),
            "only the final wire transformation may erase body origin"
        );
    }
    drop(sent);
    for bytes in [br#"{"x":1}"#.to_vec(), vec![255]] {
        let failure = run(clients.get(RestRequest {
            body: Some(RestBody::Encoded(bytes)),
            ..Default::default()
        }))
        .unwrap_err();
        assert_eq!(
            failure.message,
            "Signing requires untransformed request data"
        );
        assert!(!failure.is_retryable_network());
    }
    assert_eq!(
        wire.0.lock().unwrap().len(),
        4,
        "encoded signing refusal must send nothing"
    );
}

#[test]
fn json_content_type_transforms_only_after_signing_against_live_axios() {
    use safe_cex_transport::RestBody;
    let output = std::process::Command::new("node")
        .arg("scripts/body-transform-oracle.cjs")
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("live Axios oracle requires Node");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(cases.len(), 60, "BODY-CARDINALITY source cases");
    let host = Rc::new(HttpTestHost {
        drive_adapter: Cell::new(true),
        sign_raw: Cell::new(true),
        ..Default::default()
    });
    let wire = Arc::new(WireRecorder::default());
    let mut options = http_options(host.clone(), Rc::default(), Arc::default());
    options.adapter = wire.clone();
    let client = safe_cex_axios_create_no_proxy(options);
    for case in cases {
        let mut request = RestRequest {
            method: "POST".into(),
            body: Some(RestBody::Untransformed(
                safe_cex_types::retained_json::RetainedJsonInput::from_value(case["data"].clone())
                    .unwrap(),
            )),
            ..Default::default()
        };
        if let Some(content_type) = case["contentType"].as_str() {
            request
                .headers
                .insert("cOnTeNt-TyPe".into(), content_type.into());
        }
        run(client.execute(request)).unwrap();
        assert_eq!(
            host.signing_inputs.borrow().last(),
            Some(&case["signed"]),
            "signer must retain raw input before content-type transformation: {case}"
        );
        assert_eq!(
            wire.0.lock().unwrap().last().unwrap().body,
            Some(RestBody::Encoded(
                case["wire"].as_str().unwrap().as_bytes().to_vec()
            )),
            "wire body must match Axios stringifySafely after signing: {case}"
        );
    }
}

struct RetryBodyWire {
    requests: Mutex<Vec<RestRequest>>,
    escalate: bool,
}
impl RestAdapter for RetryBodyWire {
    fn execute(
        &self,
        mut request: RestRequest,
    ) -> safe_cex_transport::RestFuture<'_, RestResponse> {
        Box::pin(async move {
            self.requests.lock().unwrap().push(request.clone());
            if self.escalate {
                request.method = "post".into();
                request.url = "/orders".into();
            }
            Err(HttpFailure::from_transport(
                TransportError::Network(safe_cex_transport::NetworkFailure::from_code(
                    "attempt failed".into(),
                    Some("ECONNRESET".into()),
                )),
                request,
            ))
        })
    }
}
struct RetryBodyClock;
impl safe_cex_transport::rate_limit::RateLimitClock for RetryBodyClock {
    fn now_ms(&self) -> u64 {
        1000
    }
    fn sleep_ms(&self, _: u64) -> TransportFuture<'_, ()> {
        Box::pin(async { Ok(()) })
    }
}
#[test]
#[ignore = "live installed Axios retry data in mandatory isolated core gate"]
fn live_retry_body_preserves_signing_input_and_wire_bytes() {
    use safe_cex_transport::{
        RestBody,
        retry::{ClassifiedRequest, ReadClassification, network_error_on_read},
    };
    let output = std::process::Command::new("node")
        .arg("scripts/retry-body-oracle.cjs")
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rows: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        rows.as_array().unwrap().len(),
        8,
        "RETRY-BODY-CARDINALITY source cases"
    );
    for row in rows.as_array().unwrap() {
        let host = Rc::new(HttpTestHost::default());
        host.drive_adapter.set(true);
        host.sign_raw.set(true);
        host.sticky_timestamp.set(true);
        host.retry_rewrite.set(row["rewrite"].as_bool().unwrap());
        let wire = Arc::new(RetryBodyWire {
            requests: Mutex::default(),
            escalate: false,
        });
        let mut options = http_options(host.clone(), Rc::default(), Arc::default());
        options.adapter = wire.clone();
        options.params_serializer = Some(Arc::new(|_| Ok("a=1".into())));
        let client = safe_cex_axios_create_no_proxy(options);
        let request = RestRequest {
            method: "post".into(),
            url: "/token".into(),
            headers: [("Content-Type".into(), "application/json".into())]
                .into_iter()
                .collect(),
            body: Some(RestBody::Untransformed(
                safe_cex_types::retained_json::RetainedJsonInput::from_value(row["data"].clone())
                    .unwrap(),
            )),
            params: Some(serde_json::json!({"a":1})),
            ..Default::default()
        };
        let ClassifiedRequest::Read(read) =
            ReadClassification::PostedPaths(vec!["/token".into()]).classify(request)
        else {
            panic!("read token endpoint")
        };
        let result = run(network_error_on_read(read, 1).execute(&client, &RetryBodyClock));
        assert!(result.is_err());
        let expected_data: Vec<_> = row["signs"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s["data"].clone())
            .collect();
        assert_eq!(
            *host.signing_inputs.borrow(),
            expected_data,
            "retry signing input must be prior transformed data: {row}"
        );
        let expected_time: Vec<_> = row["signs"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s["timestamp"].as_str().unwrap().to_owned())
            .collect();
        assert_eq!(
            *host.signing_timestamps.borrow(),
            expected_time,
            "retry must retain signed header state"
        );
        let sent = wire.requests.lock().unwrap();
        let bodies: Vec<_> = sent
            .iter()
            .map(|r| String::from_utf8(r.body.as_ref().unwrap().wire_bytes().unwrap()).unwrap())
            .collect();
        assert_eq!(
            serde_json::json!(bodies),
            row["wires"],
            "retry wire bytes must follow Axios identity transform: {row}"
        );
        assert!(
            sent.iter().all(|r| r.url == "/token?a=1"),
            "retry must serialize original params exactly once"
        );
    }
}
#[test]
fn retry_revalidates_changed_target_against_original_read_classifier() {
    use safe_cex_transport::retry::{ClassifiedRequest, ReadClassification, network_error_on_read};
    let host = Rc::new(HttpTestHost::default());
    host.drive_adapter.set(true);
    let wire = Arc::new(RetryBodyWire {
        requests: Mutex::default(),
        escalate: true,
    });
    let mut options = http_options(host, Rc::default(), Arc::default());
    options.adapter = wire.clone();
    let client = safe_cex_axios_create_no_proxy(options);
    let ClassifiedRequest::Read(read) = ReadClassification::Methods.classify(RestRequest {
        method: "get".into(),
        url: "/account".into(),
        ..Default::default()
    }) else {
        panic!("read account")
    };
    let failure =
        run(network_error_on_read(read, 3).execute(&client, &RetryBodyClock)).unwrap_err();
    assert_eq!(
        wire.requests.lock().unwrap().len(),
        1,
        "changed retry classification must refuse write replay"
    );
    assert_eq!(failure.request.method, "post");
    assert_eq!(failure.request.url, "/orders");
}

#[test]
fn axios_header_pipeline_preserves_signer_and_wire_views() {
    use safe_cex_transport::RestBody;
    let output = std::process::Command::new("node")
        .arg("scripts/header-transform-oracle.cjs")
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rows: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(rows.len(), 72);
    for row in rows {
        let host = Rc::new(HttpTestHost {
            drive_adapter: Cell::new(true),
            sign_raw: Cell::new(true),
            rewrite_header: Cell::new(true),
            ..Default::default()
        });
        let wire = Arc::new(WireRecorder::default());
        let mut options = http_options(host.clone(), Rc::default(), Arc::default());
        options.adapter = wire.clone();
        options
            .request
            .headers
            .insert("apikey".into(), "old".into());
        if row["json"] == true {
            options
                .request
                .headers
                .insert("Content-Type".into(), "application/json".into());
        }
        let client = safe_cex_axios_create_no_proxy(options);
        let mut request = RestRequest {
            method: row["method"].as_str().unwrap().into(),
            body: row.get("data").cloned().map(|value| {
                RestBody::Untransformed(
                    safe_cex_types::retained_json::RetainedJsonInput::from_value(value).unwrap(),
                )
            }),
            ..Default::default()
        };
        request.headers.insert("APIKEY".into(), "request".into());
        run(client.execute(request)).unwrap();
        assert_eq!(
            host.signing_headers.borrow().last(),
            Some(&row["signed"]["headers"]),
            "pre-sign header normalization must match Axios: {row}"
        );
        assert_eq!(
            host.signing_inputs.borrow().last(),
            row["signed"].get("data"),
            "generated content type must not change signer data: {row}"
        );
        let sent = wire.0.lock().unwrap();
        let actual = sent.last().unwrap();
        assert_eq!(
            serde_json::to_value(&actual.headers).unwrap(),
            row["wire"]["headers"],
            "post-sign headers must match Axios: {row}"
        );
        let expected = row["wire"]
            .get("data")
            .filter(|value| !value.is_null())
            .map(|value| {
                let bytes = match value {
                    serde_json::Value::String(text) => text.as_bytes().to_vec(),
                    value => value.to_string().into_bytes(),
                };
                RestBody::Encoded(bytes)
            });
        assert_eq!(
            actual.body, expected,
            "null and generated-content-type wire body must match Axios: {row}"
        );
    }
}

#[test]
fn venue_interceptor_runs_after_defaults_without_reapplying_deleted_headers() {
    let output = std::process::Command::new("node")
        .arg("scripts/interceptor-order-oracle.cjs")
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rows: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(rows.len(), 3);
    for row in rows {
        let host = Rc::new(HttpTestHost {
            drive_adapter: Cell::new(true),
            ..Default::default()
        });
        let wire = Arc::new(WireRecorder::default());
        let stats = Arc::new(Mutex::new(HttpRequestStatsTracker::default()));
        let mut options = http_options(host, Rc::default(), stats.clone());
        options.adapter = wire.clone();
        options.request.base_url = Some("https://factory.example".into());
        options
            .request
            .headers
            .insert("x-default".into(), "factory".into());
        let client = safe_cex_axios_create_no_proxy(options);
        let mode = row["mode"].as_str().unwrap();
        let result = run(client.execute_with_interceptor(
            RestRequest {
                method: "get".into(),
                url: "/kline".into(),
                ..Default::default()
            },
            |mut request| {
                assert_eq!(
                    serde_json::to_value(&request.headers).unwrap(),
                    row["before"]["headers"],
                    "venue interceptor must observe assembled factory defaults"
                );
                assert_eq!(request.base_url.as_deref(), Some("https://factory.example"));
                assert_eq!(request.timeout_ms, Some(20000));
                async move {
                    if mode == "reject" {
                        return Err(HttpFailure::pre_transmit(
                            "interceptor refused".into(),
                            request,
                        ));
                    }
                    request.headers.clear();
                    if mode == "replace" {
                        request.headers.insert("accept".into(), "custom".into());
                    }
                    request.headers.insert("x-site-id".into(), "kline".into());
                    request.base_url = None;
                    request.timeout_ms = None;
                    Ok(request)
                }
            },
        ));
        assert_eq!(
            stats.lock().unwrap().get_stats(0.)["total"],
            row["total"],
            "interceptor refusal must precede common request stats"
        );
        let sent = wire.0.lock().unwrap();
        if mode == "reject" {
            let failure = result.unwrap_err();
            assert!(sent.is_empty(), "interceptor refusal must send nothing");
            assert!(!failure.is_retryable_network());
            assert!(failure.message.contains("interceptor refused"));
        } else {
            result.unwrap();
            assert_eq!(sent.len(), 1);
            assert_eq!(
                serde_json::to_value(&sent[0].headers).unwrap(),
                row["wire"]["headers"],
                "interceptor header deletion must survive without default reapplication"
            );
            assert!(sent[0].base_url.is_none());
            assert!(sent[0].timeout_ms.is_none());
        }
    }
}

#[test]
fn remote_echo_data_reaches_http_inner_adapter_without_a_second_transform() {
    use safe_cex_transport::{RestBody, signing::*};
    struct Echo;
    impl RemoteSigner for Echo {
        fn sign(&self, request: RemoteSignerRequest) -> TransportFuture<'_, RemoteSignerResponse> {
            Box::pin(async move {
                match request {
                    RemoteSignerRequest::Http(request) => {
                        Ok(RemoteSignerResponse::Http(HttpSigningResponse {
                            id: request.id,
                            config: Some(SignedConfig {
                                url: Some(request.config.url),
                                params: request.config.params,
                                data: request.config.data,
                                // Deliberate deletion: the inner adapter must not regenerate it.
                                headers: Default::default(),
                            }),
                            error: None,
                        }))
                    }
                    RemoteSignerRequest::WsAuth { .. } => panic!("HTTP fixture"),
                }
            })
        }
    }
    #[derive(Default)]
    struct Capture(Mutex<Vec<RestRequest>>);
    impl RestAdapter for Capture {
        fn execute(
            &self,
            request: RestRequest,
        ) -> safe_cex_transport::RestFuture<'_, RestResponse> {
            self.0.lock().unwrap().push(request);
            Box::pin(async {
                Ok(RestResponse {
                    status: 200,
                    reason: None,
                    headers: Default::default(),
                    body: safe_cex_transport::RestResponseBody::Encoded(vec![]),
                })
            })
        }
    }
    let host = Rc::new(HttpTestHost {
        drive_adapter: Cell::new(true),
        ..Default::default()
    });
    let capture = Arc::new(Capture::default());
    let mut options = http_options(host, Rc::new(Runtime::default()), Arc::default());
    options.signer = Some(Arc::new(Echo));
    options.adapter = capture.clone();
    let client = safe_cex_axios_create_no_proxy(options);
    let data = serde_json::json!({"x":1});
    run(client.execute(RestRequest {
        method: "post".into(),
        url: "/echo".into(),
        body: Some(RestBody::Untransformed(
            safe_cex_types::retained_json::RetainedJsonInput::from_value(data.clone()).unwrap(),
        )),
        ..Default::default()
    }))
    .unwrap();
    let requests = capture.0.lock().unwrap();
    use safe_cex_types::retained_json::RetainedJsonInput as Input;
    let echoed = requests[0].body.as_ref().unwrap().untransformed().unwrap();
    let Input::Object(fields) = echoed else {
        panic!("remote echo must remain an object")
    };
    assert_eq!(
        fields.len(),
        1,
        "HTTP inner adapter must preserve remote echoed object keys"
    );
    assert_eq!(fields[0].0, vec![u16::from(b'x')]);
    let Input::Float(value) = fields[0].1 else {
        panic!("remote JSON.parse must produce a native number")
    };
    assert_eq!(
        value.to_bits(),
        1.0_f64.to_bits(),
        "HTTP inner adapter must preserve remote echoed number bits"
    );
    assert_eq!(
        safe_cex_num::js_retained_json_stringify(echoed)
            .unwrap()
            .as_deref(),
        Some(r#"{"x":1}"#)
    );
    assert!(
        requests[0].headers.is_empty(),
        "HTTP inner adapter must preserve remote Content-Type deletion"
    );
}

#[test]
fn query_factory_profile_precedes_interceptor_and_request_override_survives() {
    use safe_cex_transport::query::{QueryArrayFormat, QueryProfile};
    let indexed = QueryProfile {
        key_order: safe_cex_transport::query::QueryKeyOrder::Preserve,
        arrays: QueryArrayFormat::Indexed,
        encode: true,
    };
    let repeat = QueryProfile {
        key_order: safe_cex_transport::query::QueryKeyOrder::Preserve,
        arrays: QueryArrayFormat::Repeat,
        encode: false,
    };
    for (request_profile, expected_profile, expected_url) in [
        (
            None,
            indexed,
            "/query?list%5B0%5D=a%20b&list%5B1%5D=Infinity",
        ),
        (Some(repeat), repeat, "/query?list=a b&list=Infinity"),
    ] {
        let host = Rc::new(HttpTestHost {
            drive_adapter: Cell::new(true),
            ..Default::default()
        });
        let wire = Arc::new(WireRecorder::default());
        let mut options = http_options(host, Rc::default(), Arc::default());
        options.adapter = wire.clone();
        options.request.query_profile = Some(indexed);
        options.params_serializer = Some(Arc::new(|_| {
            panic!("QUERY explicit profile bypasses factory callback")
        }));
        let client = safe_cex_axios_create_no_proxy(options);
        run(client.execute_with_interceptor(
            RestRequest {
                method: "GET".into(),
                url: "/query".into(),
                query_profile: request_profile,
                query_params: Some(Q::Object(
                    [(
                        "list".into(),
                        Q::Array(vec![Q::String("a b".into()), Q::Number(f64::INFINITY)]),
                    )]
                    .into_iter()
                    .collect(),
                )),
                ..Default::default()
            },
            |request| {
                assert_eq!(
                    request.query_profile,
                    Some(expected_profile),
                    "QUERY factory profile visible before interceptor"
                );
                async { Ok(request) }
            },
        ))
        .unwrap();
        let sent = wire.0.lock().unwrap();
        assert_eq!(sent.len(), 1, "QUERY factory native request count");
        assert_eq!(
            sent[0].query_profile,
            Some(expected_profile),
            "QUERY factory native profile identity"
        );
        assert!(
            sent[0].query_params.is_none(),
            "QUERY factory consumes params once"
        );
        assert_eq!(
            sent[0].url, expected_url,
            "QUERY factory exact wire rendering"
        );
    }
}

struct EffectiveConfigAdapter;
impl RestAdapter for EffectiveConfigAdapter {
    fn execute(&self, _: RestRequest) -> safe_cex_transport::RestFuture<'_, RestResponse> {
        panic!("HTTP must retain effective adapter config")
    }
    fn execute_with_receipt(
        &self,
        mut request: RestRequest,
    ) -> safe_cex_transport::RestFuture<'_, safe_cex_transport::RestExecution> {
        Box::pin(async move {
            request.method = "head".into();
            request.url = "/returned-effective-target".into();
            request.headers.clear();
            request
                .headers
                .insert("Returned-Only".into(), "effective".into());
            request.body = Some(safe_cex_transport::RestBody::Untransformed(
                safe_cex_types::retained_json::RetainedJsonInput::from_value(
                    serde_json::json!({"inner":67}),
                )
                .unwrap(),
            ));
            Ok(safe_cex_transport::RestExecution {
                response: RestResponse {
                    status: 203,
                    reason: Some("effective".into()),
                    headers: BTreeMap::new(),
                    body: safe_cex_transport::RestResponseBody::Untransformed(
                        safe_cex_types::retained_json::RetainedJsonInput::from_value(
                            serde_json::json!({"answer":71}),
                        )
                        .unwrap(),
                    ),
                },
                config: request,
            })
        })
    }
}
#[test]
fn http_response_hook_observes_effective_adapter_config() {
    let host = Rc::new(HttpTestHost {
        drive_adapter: Cell::new(true),
        ..Default::default()
    });
    let mut options = http_options(host.clone(), Rc::new(Runtime::default()), Arc::default());
    options.adapter = Arc::new(EffectiveConfigAdapter);
    let client = safe_cex_axios_create_no_proxy(options);
    let execution = run(client.execute_with_receipt(RestRequest {
        method: "post".into(),
        url: "/original-target".into(),
        body: Some(safe_cex_transport::RestBody::Untransformed(
            safe_cex_types::retained_json::RetainedJsonInput::from_value(
                serde_json::json!({"outer":73}),
            )
            .unwrap(),
        )),
        ..Default::default()
    }))
    .unwrap();
    assert_eq!(
        execution.config.method, "head",
        "http_receipt_native_method"
    );
    assert_eq!(
        execution.config.url, "/returned-effective-target",
        "http_receipt_native_url"
    );
    assert_eq!(
        execution.config.headers.len(),
        1,
        "http_receipt_native_header_omission"
    );
    assert_eq!(execution.response.status, 203, "http_receipt_native_status");
    assert_eq!(
        execution
            .config
            .body
            .as_ref()
            .unwrap()
            .untransformed()
            .unwrap(),
        &safe_cex_types::retained_json::RetainedJsonInput::from_value(
            serde_json::json!({"inner":67})
        )
        .unwrap(),
        "http_receipt_native_inner_body"
    );
    assert_eq!(
        host.response_configs.borrow().len(),
        1,
        "http_receipt_native_hook_count"
    );
    assert_eq!(
        host.response_configs.borrow()[0],
        execution.config,
        "http_receipt_native_hook_config"
    );
}

#[test]
fn http_response_transform_precedes_success_and_rejection_hooks() {
    for status in [200, 403] {
        let host = Rc::new(HttpTestHost {
            status: Cell::new(status),
            ..Default::default()
        });
        let mut options = http_options(host.clone(), Rc::new(Runtime::default()), Arc::default());
        let events = host.events.clone();
        options.response_transform = Some(Rc::new(move |mut response, request| {
            assert_eq!(
                request.url, "/transform-order",
                "http_transform_native_config"
            );
            assert!(
                matches!(
                    response.body,
                    safe_cex_transport::RestResponseBody::Encoded(_)
                ),
                "http_transform_native_input_phase"
            );
            events.borrow_mut().push("transform".into());
            let original = response.clone();
            response.body = response.body.transform_axios_response().map_err(|error| {
                let mut failure = HttpFailure::from_transport(error, request.clone());
                failure.response = Some(Box::new(original));
                failure
            })?;
            Ok(response)
        }));
        let client = safe_cex_axios_create_no_proxy(options);
        let result = run(client.execute(RestRequest {
            method: "get".into(),
            url: "/transform-order".into(),
            ..Default::default()
        }));
        let expected = safe_cex_transport::RestResponseBody::Transformed(
            safe_cex_types::retained_json::RetainedJsonInput::from_value(
                serde_json::json!({"message":"Invalid JWT"}),
            )
            .unwrap(),
        );
        if status == 200 {
            let response = result.unwrap();
            assert_eq!(
                response.body, expected,
                "http_transform_native_success_body"
            );
            assert_eq!(
                *host.response_bodies.borrow(),
                [expected],
                "http_transform_native_success_hook_body"
            );
            assert!(
                host.failure_bodies.borrow().is_empty(),
                "http_transform_native_success_no_failure"
            );
        } else {
            let failure = result.unwrap_err();
            assert_eq!(
                failure.response.unwrap().body,
                expected,
                "http_transform_native_failure_body"
            );
            assert_eq!(
                *host.failure_bodies.borrow(),
                [Some(expected)],
                "http_transform_native_failure_hook_body"
            );
            assert!(
                host.response_bodies.borrow().is_empty(),
                "http_transform_native_failure_no_success"
            );
        }
        let observed = host
            .events
            .borrow()
            .iter()
            .filter(|event| {
                matches!(
                    event.as_str(),
                    "transform" | "response" | "success" | "failure"
                )
            })
            .cloned()
            .collect::<Vec<_>>();
        let expected = if status == 200 {
            vec!["transform", "response", "success"]
        } else {
            vec!["transform", "failure"]
        };
        assert_eq!(
            observed, expected,
            "http_transform_native_hook_order {status}"
        );
    }
}
