use safe_cex_exchange::{
    exchange::Exchange,
    http::{HttpClientOptions, HttpFailure, HttpFuture, HttpHost},
    runtime::{ExchangeError, ExchangeFuture, ExchangeRuntime, ExchangeTask},
};
use safe_cex_store::store::DefaultStore;
use safe_cex_transport::{
    RestAdapter, RestFuture, RestRequest, RestResponse, RestResponseBody, TransportRegistry,
};
use safe_cex_types::{ExchangeName, ExchangeOptions};
use safe_cex_venue_txflow::{api::create_api, exchange::TxFlowExchange, types::*};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    future::Future,
    pin::Pin,
    rc::Rc,
    sync::{Arc, Mutex},
    task::{Context, Poll, Waker},
};

fn run<F: Future>(future: F) -> F::Output {
    match std::pin::pin!(future)
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(v) => v,
        Poll::Pending => panic!("fixture unexpectedly yielded"),
    }
}
#[derive(Default)]
struct Runtime {
    console: RefCell<Vec<(String, String)>>,
}
impl ExchangeRuntime for Runtime {
    fn monotonic_now_ms(&self) -> f64 {
        1700000000123.0
    }
    fn now_ms(&self) -> f64 {
        1700000000123.0
    }
    fn sleep(&self, _: u64) -> ExchangeFuture<'static, ()> {
        Box::pin(async { Ok(()) })
    }
    fn spawn_local(&self, _: Pin<Box<dyn Future<Output = ()>>>) -> Box<dyn ExchangeTask> {
        panic!("TxFlow phase 0 must not spawn")
    }
    fn console_values(&self, severity: &str, message: &[safe_cex_exchange::js_runtime::value::Js]) {
        let rendered = safe_cex_exchange::runtime::console_text(message);
        let message = rendered.as_str();
        self.console
            .borrow_mut()
            .push((severity.into(), message.into()));
    }
    fn unique_id(&self) -> String {
        "fixture".into()
    }
    fn is_worker(&self) -> bool {
        true
    }
    fn random(&self) -> f64 {
        0.5
    }
    fn report_raw_exception(&self, _: &safe_cex_exchange::js_runtime::value::Js, _: &Value) {
        panic!("unexpected raw exception report")
    }
    fn report_exception(&self, _: &ExchangeError, _: &Value) {}
    fn execute_worker_request(&self, _: Value) -> ExchangeFuture<'static, Value> {
        panic!("TxFlow phase 0 must not dispatch worker")
    }
}
struct Host;
impl HttpHost for Host {
    fn execute(&self, adapter: Arc<dyn RestAdapter>, request: RestRequest) -> HttpFuture<'_> {
        Box::pin(async move { adapter.execute_with_receipt(request).await })
    }
    fn enhance_error(&self, e: HttpFailure, _: &ExchangeName) -> HttpFailure {
        e
    }
    fn emit(&self, _: &str, _: &str) -> Result<(), String> {
        Ok(())
    }
    fn note_success(&self, _: &ExchangeName, _: &RestResponse) {}
    fn note_failure(&self, _: &ExchangeName, _: &HttpFailure) {}
    fn mark_start(&self, _: &mut RestRequest) {}
    fn signing_capture(&self, _: &RestRequest) -> bool {
        false
    }
    fn is_development(&self) -> bool {
        false
    }
    fn is_main_window(&self) -> bool {
        false
    }
}
struct Capture(Arc<Mutex<Vec<RestRequest>>>);
impl RestAdapter for Capture {
    fn execute(&self, request: RestRequest) -> RestFuture<'_, RestResponse> {
        self.0.lock().unwrap().push(request);
        Box::pin(async {
            Ok(RestResponse {
                status: 200,
                reason: None,
                headers: Default::default(),
                body: RestResponseBody::Encoded(b"null".to_vec()),
            })
        })
    }
}
fn host(
    options: ExchangeOptions,
    capture: Arc<Mutex<Vec<RestRequest>>>,
    runtime: Rc<Runtime>,
) -> HttpClientOptions {
    HttpClientOptions {
        response_transform: None,
        params_serializer: None,
        exchange: ExchangeName::Txflow,
        options,
        request: RestRequest::default(),
        provider: None,
        signer: None,
        account_name: None,
        adapter: Arc::new(Capture(capture)),
        host: Rc::new(Host),
        runtime,
        store: None,
        stats: None,
        logs: None,
        every: vec![],
        periodic: vec![],
        next_id: Arc::new(|| "id".into()),
        signing_now: Arc::new(|| 1700000000123),
    }
}
fn oracle() -> Value {
    let output = std::process::Command::new("node")
        .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/oracle.cjs"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}
fn options(testnet: &Value) -> ExchangeOptions {
    let mut options = json!({"key":"k","secret":"s"});
    if !testnet.is_null() {
        options["testnet"] = testnet.clone();
    }
    serde_json::from_value(options).unwrap()
}

#[test]
fn actual_typescript_constants_factory_and_start() {
    let source = oracle();
    assert_eq!(source["constants"]["mainnet"], TXFLOW_MAINNET_REST_URL);
    assert_eq!(source["constants"]["testnet"], TXFLOW_TESTNET_REST_URL);
    assert_eq!(
        source["constants"]["unimplemented"],
        TXFLOW_PHASE_0_UNIMPLEMENTED_MESSAGE
    );

    let cases = source["factory"].as_array().unwrap();
    assert_eq!(cases.len(), 3, "TXFLOW factory case count");
    for case in cases {
        let captures = Arc::new(Mutex::new(vec![]));
        let client = create_api(
            host(
                options(&case["testnet"]),
                captures.clone(),
                Rc::new(Runtime::default()),
            ),
            &TransportRegistry::default(),
        )
        .unwrap();
        run(client.execute(RestRequest {
            url: "/info".into(),
            method: "post".into(),
            ..Default::default()
        }))
        .unwrap();
        let got = captures.lock().unwrap();
        assert_eq!(got.len(), 1);
        let config = &case["config"];
        assert_eq!(
            got[0].base_url.as_deref(),
            config["baseURL"].as_str(),
            "TXFLOW baseURL testnet={}",
            case["testnet"]
        );
        let headers = config["headers"].as_object().unwrap();
        assert_eq!(headers.len(), 1, "TXFLOW source header count");
        for (name, value) in headers {
            assert_eq!(
                got[0].headers.get(name).map(String::as_str),
                value.as_str(),
                "TXFLOW header {name}"
            );
        }
    }

    let start = &source["start"];
    assert_eq!(
        start["synchronous"], true,
        "source start throws synchronously"
    );
    assert_eq!(
        start["events"],
        json!([
            ["super", "txflow"],
            ["rejectPreflight", TXFLOW_PHASE_0_UNIMPLEMENTED_MESSAGE]
        ])
    );
    let runtime = Rc::new(Runtime::default());
    let opts = options(&json!(true));
    let exchange = TxFlowExchange::new(
        ExchangeName::Txflow,
        opts.clone(),
        Rc::new(DefaultStore::new()),
        runtime.clone(),
        host(opts, Arc::new(Mutex::new(vec![])), runtime.clone()),
        &TransportRegistry::default(),
    )
    .unwrap();
    assert_eq!(Exchange::name(&exchange), &ExchangeName::Txflow);
    let pending = Exchange::start(&exchange);
    let error_logs = |runtime: &Runtime| {
        runtime
            .console
            .borrow()
            .iter()
            .filter(|(severity, _)| severity == "error")
            .cloned()
            .collect::<Vec<_>>()
    };
    assert_eq!(
        error_logs(&runtime),
        vec![(
            "error".to_owned(),
            format!("exchange error {TXFLOW_PHASE_0_UNIMPLEMENTED_MESSAGE}")
        )],
        "TXFLOW preflight rejection is emitted at invocation, before polling"
    );
    let error = run(pending).unwrap_err();
    assert_eq!(error.to_string(), start["message"].as_str().unwrap());
    assert_eq!(error_logs(&runtime).len(), 1, "TXFLOW emits exactly once");
}
