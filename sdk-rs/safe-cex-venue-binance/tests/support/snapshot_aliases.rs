//! Concrete exchange callbacks; only the network terminal and scheduler are scripted.
use super::*;
use safe_cex_transport::{RestAdapter, RestFuture, RestRequest, RestResponse, RestResponseBody};
use safe_cex_venue_binance::{
    exchange_classes::Method, position_snapshots::PositionSnapshotBindings,
};
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct SnapshotRuntime {
    inner: Runtime,
    reports: RefCell<Vec<EventValue>>,
}
impl ExchangeRuntime for SnapshotRuntime {
    fn now_ms(&self) -> f64 {
        self.inner.now_ms()
    }
    fn monotonic_now_ms(&self) -> f64 {
        self.inner.monotonic_now_ms()
    }
    fn sleep(&self, delay: u64) -> ExchangeFuture<'static, ()> {
        self.inner.sleep(delay)
    }
    fn spawn_local(&self, f: Pin<Box<dyn Future<Output = ()>>>) -> Box<dyn ExchangeTask> {
        self.inner.spawn_local(f)
    }
    fn console_values(&self, _: &str, _: &[EventValue]) {}
    fn unique_id(&self) -> String {
        self.inner.unique_id()
    }
    fn is_worker(&self) -> bool {
        false
    }
    fn random(&self) -> f64 {
        0.5
    }
    fn report_raw_exception(
        &self,
        error: &EventValue,
        _: &safe_cex_types::retained_json::RetainedJsonInput,
    ) {
        self.reports.borrow_mut().push(error.clone());
    }
    fn report_exception(
        &self,
        error: &ExchangeError,
        context: &safe_cex_types::retained_json::RetainedJsonInput,
    ) {
        self.inner.report_exception(error, context);
    }
    fn execute_worker_request(&self, value: Value) -> ExchangeFuture<'static, Value> {
        self.inner.execute_worker_request(value)
    }
}
struct Terminal {
    account: bool,
    requests: Arc<Mutex<Vec<String>>>,
}
impl RestAdapter for Terminal {
    fn execute(&self, request: RestRequest) -> RestFuture<'_, RestResponse> {
        let path = request.url.split('?').next().unwrap();
        let body = if path.ends_with("/account") {
            assert!(self.account);
            br#"{"assets":[{"asset":"USDT","walletBalance":"12.5","availableBalance":"10","initialMargin":"2.5","unrealizedProfit":"-0"}],"positions":[]}"#.to_vec()
        } else if path.ends_with("/symbolConfig") {
            b"[]".to_vec()
        } else {
            assert!(path.ends_with("/positionRisk"));
            b"null".to_vec()
        };
        self.requests.lock().unwrap().push(request.url);
        Box::pin(async move {
            Ok(RestResponse {
                status: 200,
                reason: None,
                headers: Default::default(),
                body: RestResponseBody::Encoded(body),
            })
        })
    }
}
fn constructed(
    venue: Venue,
    account: bool,
) -> (
    Rc<ExchangeOwner>,
    Rc<SnapshotRuntime>,
    Arc<Mutex<Vec<String>>>,
) {
    let runtime = Rc::new(SnapshotRuntime::default());
    let requests = Arc::new(Mutex::new(vec![]));
    let dependencies = safe_cex_venue_binance::native_host::native_dependencies(
        runtime.clone(),
        Arc::new(Terminal {
            account,
            requests: requests.clone(),
        }),
        Rc::new(|_, _| panic!("unexpected external provider")),
        None,
        safe_cex_exchange::native::NativeHttpEvents::default(),
    );
    let name = if venue == Venue::PM {
        safe_cex_types::ExchangeName::Binancepm
    } else {
        safe_cex_types::ExchangeName::Binance
    };
    let owner = ExchangeOwner::new(
        venue,
        name,
        serde_json::from_value(json!({"key":"fixture-key","secret":"fixture-secret"})).unwrap(),
        Rc::new(safe_cex_exchange::native_store::DefaultStore::new()),
        dependencies,
    )
    .unwrap();
    (owner, runtime, requests)
}
fn signed(requests: &Arc<Mutex<Vec<String>>>) {
    let requests = requests.lock().unwrap();
    assert!(!requests.is_empty());
    for url in requests.iter() {
        assert!(
            url.contains("timestamp=") && url.contains("signature="),
            "actual signed HTTP: {url}"
        );
    }
}
#[test]
fn emit_alias_reports_native_position_failure_and_propagates_listener_error() {
    for venue in [Venue::Futures, Venue::PM] {
        let (owner, runtime, requests) = constructed(venue, false);
        let errors = Rc::new(RefCell::new(vec![]));
        let seen = errors.clone();
        owner.base.on(
            "error",
            Rc::new(move |_, value| {
                seen.borrow_mut().push(value.clone());
                Ok(())
            }),
        );
        let result = run(&runtime.inner, owner.invoke(Method::Positions, vec![])).unwrap();
        assert!(matches!(result, EventValue::Array(ref rows) if rows.is_empty()));
        assert_eq!(
            errors.borrow().len(),
            1,
            "captured Emit reaches the real Base emitter"
        );
        assert!(errors.borrow()[0].text().unwrap().contains("null"));
        assert_eq!(&*runtime.inner.reported.borrow(), &["ExchangeError"]);
        assert!(
            runtime.reports.borrow().is_empty(),
            "string failures follow the typed reporter branch"
        );
        signed(&requests);
        let emit: safe_cex_venue_binance::position_snapshots::Emit =
            owner.state.capture_emit().unwrap();
        let alias = emit.clone();
        assert!(Rc::ptr_eq(&emit, &alias));
        let listener = owner.base.on(
            "error",
            Rc::new(|_, _| Err(ExchangeError::new("emit listener sentinel"))),
        );
        let failure = alias(string("explicit alias failure")).unwrap_err();
        assert_eq!(failure.name, "Error");
        assert_eq!(failure.message, "emit listener sentinel");
        owner.base.off("error", Some(&listener));
        owner.dispose().unwrap();
    }
}
#[test]
fn log_alias_delivers_balance_severity_once_and_propagates_listener_error() {
    let (owner, runtime, requests) = constructed(Venue::Futures, true);
    let logs = Rc::new(RefCell::new(vec![]));
    let seen = logs.clone();
    owner.base.on_args(
        "log",
        Rc::new(move |_, args| {
            seen.borrow_mut().push(args.to_vec());
            Ok(())
        }),
    );
    for _ in 0..2 {
        let result = run(
            &runtime.inner,
            owner.invoke(Method::PositionsBalance, vec![]),
        )
        .unwrap();
        assert_eq!(
            result
                .read("balance")
                .unwrap()
                .read("total")
                .unwrap()
                .number()
                .unwrap()
                .to_bits(),
            12.5f64.to_bits()
        );
    }
    signed(&requests);
    assert_eq!(
        logs.borrow().len(),
        1,
        "captured Log publishes aggregation only once"
    );
    assert_eq!(logs.borrow()[0][1].text().unwrap(), "info");
    assert!(
        logs.borrow()[0][0]
            .text()
            .unwrap()
            .contains("Stablecoins: USDT(12.5)")
    );
    let log: safe_cex_venue_binance::position_snapshots::Log = owner.state.capture_log().unwrap();
    let alias = log.clone();
    assert!(Rc::ptr_eq(&log, &alias));
    alias("explicit callback".into(), LogSeverity::Warning).unwrap();
    assert_eq!(logs.borrow()[1][1].text().unwrap(), "warning");
    assert!(
        logs.borrow()[1][0]
            .text()
            .unwrap()
            .ends_with("explicit callback")
    );
    let listener = owner.base.on(
        "log",
        Rc::new(|_, _| Err(ExchangeError::new("log listener sentinel"))),
    );
    let failure = alias("callback refuses".into(), LogSeverity::Error).unwrap_err();
    assert_eq!(failure.name, "Error");
    assert_eq!(failure.message, "log listener sentinel");
    owner.base.off("log", Some(&listener));
    owner.dispose().unwrap();
}

impl WsRuntime for SnapshotRuntime {
    fn transport(&self) -> Arc<dyn safe_cex_transport::TransportProvider> {
        self.inner.transport()
    }
    fn tunnel_host(&self) -> Rc<dyn safe_cex_transport::socket::TunnelHost> {
        self.inner.tunnel_host()
    }
    fn dial(
        &self,
        url: String,
        protocols: Vec<String>,
        options: WebSocketConnectionOptions,
        private: bool,
    ) -> ExchangeFuture<'static, Rc<dyn WsSocket>> {
        self.inner.dial(url, protocols, options, private)
    }
    fn headers_capable(&self) -> bool {
        false
    }
    fn enable_reconnecting(&self) -> bool {
        false
    }
    fn min_reconnection_delay_ms(&self) -> f64 {
        1000.0
    }
    fn library_min_reconnection_delay_ms(&self) -> f64 {
        1000.0
    }
    fn diagnostics_enabled(&self) -> bool {
        false
    }
    fn lifecycle_logs_enabled(&self) -> bool {
        false
    }
}
