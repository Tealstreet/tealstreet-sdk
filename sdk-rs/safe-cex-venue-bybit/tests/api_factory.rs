#![allow(clippy::arc_with_non_send_sync)]
#[path = "support/api_factory_clock_budget.rs"]
pub mod api_factory_clock_budget;
#[path = "support/api_factory_params.rs"]
pub mod api_factory_params;
#[path = "support/api_factory_params_astral.rs"]
pub mod api_factory_params_astral;
#[path = "support/api_factory_probe.rs"]
pub mod api_factory_probe;
#[path = "support/api_factory_proxy_region.rs"]
pub mod api_factory_proxy_region;
#[path = "support/api_factory_runtime.rs"]
pub mod api_factory_runtime;
#[path = "support/api_factory_timeout.rs"]
pub mod api_factory_timeout;
use safe_cex_exchange::{
    capabilities::ExchangeCapabilities,
    exchange::{BaseExchange, Exchange},
    http::{HttpClientOptions, HttpFailure, HttpFuture, HttpHost},
    runtime::{ExchangeError, ExchangeFuture, ExchangeRuntime, ExchangeTask},
};
use safe_cex_store::{store::DefaultStore, virtual_clock::VirtualClock};
use safe_cex_transport::{
    NetworkFailure, RestAdapter, RestBody, RestFailure, RestFuture, RestRequest, RestResponse,
    RestResponseBody, TransportError, TransportFuture, TransportRegistry,
    rate_limit::RateLimitClock,
};
use safe_cex_types::{ExchangeName, ExchangeOptions};
use safe_cex_venue_bybit::api_factory::{BybitApi, BybitApiOptions, create_api};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    future::Future,
    pin::Pin,
    process::Command,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    task::{Context, Poll, Waker},
};

const GET_ONE: &str = "d7679158b705d7843f97a42f20daf61e69e0a0508c27764103c814be66e10d24";
const GET_TWO: &str = "ba3a0f7ba95d8b58f6fded292ee626c757f5827ff24f3335c0a3a96c5a35b7a6";
const POST_ONE: &str = "6018b8615688d269d61a0f28357023f50edb484ce66230a5e0cef9e5b87027c8";
const CLOCK_TWO: &str = "7cf184fece248efb635e17e5814ad883608fcc8c998bad31cc6a082b449e8656";
fn ready<F: Future>(future: F) -> F::Output {
    let mut future = std::pin::pin!(future);
    match future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("factory fixture unexpectedly suspended"),
    }
}
#[derive(Default)]
struct Runtime {
    console: RefCell<Vec<(String, String)>>,
    expected_report: RefCell<Option<String>>,
    reports: RefCell<Vec<(ExchangeError, Value)>>,
}
impl ExchangeRuntime for Runtime {
    fn monotonic_now_ms(&self) -> f64 {
        1000.0
    }
    fn now_ms(&self) -> f64 {
        1000.0
    }
    fn sleep(&self, _: u64) -> ExchangeFuture<'static, ()> {
        Box::pin(async { Ok(()) })
    }
    fn spawn_local(&self, _: Pin<Box<dyn Future<Output = ()>>>) -> Box<dyn ExchangeTask> {
        panic!("no environment probe in this target")
    }
    fn console_values(&self, level: &str, message: &[safe_cex_exchange::js_runtime::value::Js]) {
        let rendered = safe_cex_exchange::runtime::console_text(message);
        let message = rendered.as_str();
        self.console
            .borrow_mut()
            .push((level.into(), message.into()));
    }
    fn unique_id(&self) -> String {
        "factory-listener".into()
    }
    fn is_worker(&self) -> bool {
        false
    }
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
    fn report_exception(&self, error: &ExchangeError, context: &Value) {
        assert_eq!(
            self.expected_report.borrow().as_deref(),
            Some(error.message.as_str()),
            "factory_native_unexpected_exception {error}"
        );
        self.reports
            .borrow_mut()
            .push((error.clone(), context.clone()));
    }
    fn execute_worker_request(&self, _: Value) -> ExchangeFuture<'static, Value> {
        panic!("no worker request")
    }
}
struct Clock;
impl RateLimitClock for Clock {
    fn now_ms(&self) -> u64 {
        1000
    }
    fn sleep_ms(&self, _: u64) -> TransportFuture<'_, ()> {
        Box::pin(async { Ok(()) })
    }
}
struct Owner {
    base: BaseExchange,
    capabilities: ExchangeCapabilities,
    accesses: RefCell<Vec<u8>>,
    trace: Rc<RefCell<Vec<String>>>,
}
impl Exchange for Owner {
    type PlaceOpts = safe_cex_types::PlaceOrderOpts;
    fn base(&self) -> &BaseExchange {
        self.accesses
            .borrow_mut()
            .push(self.base.auth_failure_streak());
        self.trace.borrow_mut().push("owner-base".into());
        &self.base
    }
    fn capabilities(&self) -> &ExchangeCapabilities {
        &self.capabilities
    }
}
struct Host {
    owner: std::rc::Weak<Owner>,
    trace: Rc<RefCell<Vec<String>>>,
    responses: RefCell<Vec<Value>>,
    failures: Cell<usize>,
}
impl HttpHost for Host {
    fn execute(&self, adapter: Arc<dyn RestAdapter>, request: RestRequest) -> HttpFuture<'_> {
        Box::pin(async move { adapter.execute_with_receipt(request).await })
    }
    fn enhance_error(&self, error: HttpFailure, _: &ExchangeName) -> HttpFailure {
        error
    }
    fn emit(&self, _: &str, _: &str) -> Result<(), String> {
        Ok(())
    }
    fn note_response(&self, response: &RestResponse, request: &RestRequest) {
        self.trace.borrow_mut().push("common-response".into());
        let value = response.body.clone().into_data();
        self.responses.borrow_mut().push(value.clone());
        if let Some(owner) = self.owner.upgrade() {
            owner.base.note_http_response(
                &json!({"status":response.status,"data":value}),
                Some(&request.url),
            );
        }
    }
    fn note_success(&self, _: &ExchangeName, _: &RestResponse) {}
    fn note_failure(&self, _: &ExchangeName, _: &HttpFailure) {
        self.trace.borrow_mut().push("common-failure".into());
        self.failures.set(self.failures.get() + 1);
    }
    fn mark_start(&self, _: &mut RestRequest) {
        self.trace.borrow_mut().push("start".into());
    }
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
enum Reply {
    Rejected(RestResponseBody),
    RejectedMethod {
        body: RestResponseBody,
        method: &'static str,
    },
    Response(RestResponseBody),
    Network {
        url: Option<&'static str>,
        method: Option<&'static str>,
    },
}
struct Wire {
    replies: RefCell<VecDeque<Reply>>,
    requests: RefCell<Vec<RestRequest>>,
    trace: Rc<RefCell<Vec<String>>>,
    query_in_url: bool,
}
impl RestAdapter for Wire {
    fn query_in_url(&self) -> bool {
        self.query_in_url
    }
    fn execute(&self, mut request: RestRequest) -> RestFuture<'_, RestResponse> {
        self.trace.borrow_mut().push("wire".into());
        self.requests.borrow_mut().push(request.clone());
        let reply = self
            .replies
            .borrow_mut()
            .pop_front()
            .expect("unexpected physical attempt");
        Box::pin(async move {
            match reply {
                Reply::Rejected(body) => Err(RestFailure {
                    code: Some("ERR_BAD_RESPONSE".into()),
                    transport_error: None,
                    message: "original venue rejection".into(),
                    response: Some(Box::new(RestResponse {
                        status: 400,
                        reason: Some("Bad Request".into()),
                        headers: Default::default(),
                        body,
                    })),
                    request: Box::new(request),
                    context: Some(Box::new(json!({"retainedCause": 71}))),
                }),
                Reply::RejectedMethod { body, method } => {
                    request.method = method.into();
                    Err(RestFailure {
                        code: Some("ERR_BAD_RESPONSE".into()),
                        transport_error: None,
                        message: "original venue rejection".into(),
                        response: Some(Box::new(RestResponse {
                            status: 400,
                            reason: Some("Bad Request".into()),
                            headers: Default::default(),
                            body,
                        })),
                        request: Box::new(request),
                        context: Some(Box::new(json!({"retainedCause": 71}))),
                    })
                }
                Reply::Response(body) => Ok(RestResponse {
                    status: 200,
                    reason: Some("OK".into()),
                    headers: Default::default(),
                    body,
                }),
                Reply::Network { url, method } => {
                    if let Some(url) = url {
                        request.url = url.into();
                    }
                    if let Some(method) = method {
                        request.method = method.into();
                    }
                    Err(RestFailure::from_transport(
                        TransportError::Network(NetworkFailure::from_code(
                            "scripted network".into(),
                            Some("ERR_NETWORK".into()),
                        )),
                        request,
                    ))
                }
            }
        })
    }
}
struct Fixture {
    api: BybitApi,
    owner: Rc<Owner>,
    host: Rc<Host>,
    wire: Arc<Wire>,
    runtime: Rc<Runtime>,
    trace: Rc<RefCell<Vec<String>>>,
    clock: VirtualClock,
    clock_fetches: Arc<AtomicUsize>,
    emissions: Rc<Cell<usize>>,
    caller_options: ExchangeOptions,
}
fn fixture(replies: Vec<Reply>) -> Fixture {
    fixture_with_query_owner(replies, true)
}
fn fixture_with_query_owner(replies: Vec<Reply>, query_in_url: bool) -> Fixture {
    let options: ExchangeOptions =
        serde_json::from_value(json!({"key":"key","secret":"secret"})).unwrap();
    fixture_with_options(replies, query_in_url, options)
}
fn fixture_with_options(
    replies: Vec<Reply>,
    query_in_url: bool,
    options: ExchangeOptions,
) -> Fixture {
    let runtime = Rc::new(Runtime::default());
    let trace = Rc::new(RefCell::new(Vec::new()));
    let caller_options = options.clone();
    let owner = Rc::new(Owner {
        base: BaseExchange::new(
            ExchangeName::Bybitinverse,
            options.clone(),
            Rc::new(DefaultStore::new()),
            runtime.clone(),
            Default::default(),
        ),
        capabilities: Default::default(),
        accesses: RefCell::new(Vec::new()),
        trace: trace.clone(),
    });
    let emissions = Rc::new(Cell::new(0));
    let out = emissions.clone();
    owner.base.on(
        "error",
        Rc::new(move |_, _| {
            out.set(out.get() + 1);
            Ok(())
        }),
    );
    let host = Rc::new(Host {
        owner: Rc::downgrade(&owner),
        trace: trace.clone(),
        responses: RefCell::new(Vec::new()),
        failures: Cell::new(0),
    });
    let wire = Arc::new(Wire {
        replies: RefCell::new(replies.into()),
        requests: RefCell::new(Vec::new()),
        trace: trace.clone(),
        query_in_url,
    });
    let clock_fetches = Arc::new(AtomicUsize::new(0));
    let fetches = clock_fetches.clone();
    let mut clock = VirtualClock::default();
    clock.now = Arc::new(|| 1000.0);
    clock.runtime = "browser".into();
    clock.venue_time_source = Some(Arc::new(move || {
        fetches.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { Ok(Some(2000.0)) })
    }));
    let ids = Arc::new(AtomicUsize::new(0));
    let source_owner: Rc<dyn Exchange> = owner.clone();
    let api = create_api(
        BybitApiOptions {
            http: HttpClientOptions {
                params_serializer: None,
                response_transform: None,
                exchange: ExchangeName::Bybitinverse,
                options,
                request: RestRequest::default(),
                provider: None,
                signer: None,
                account_name: None,
                adapter: wire.clone(),
                host: host.clone(),
                runtime: runtime.clone(),
                store: None,
                stats: Some(Default::default()),
                logs: Some(Default::default()),
                every: Vec::new(),
                periodic: Vec::new(),
                next_id: Arc::new(move || format!("id-{}", ids.fetch_add(1, Ordering::SeqCst) + 1)),
                signing_now: Arc::new(|| 1000),
            },
            owner: Rc::downgrade(&source_owner),
            is_demo: false,
            rest_base_url: None,
            clock: clock.clone(),
            retry_clock: Rc::new(Clock),
            payload_signer: None,
        },
        &TransportRegistry::default(),
    )
    .unwrap();
    Fixture {
        api,
        owner,
        host,
        wire,
        runtime,
        trace,
        clock,
        clock_fetches,
        emissions,
        caller_options,
    }
}
fn response(raw: &str) -> Reply {
    Reply::Response(RestResponseBody::Encoded(raw.as_bytes().to_vec()))
}
fn get(url: &str) -> RestRequest {
    RestRequest {
        method: "get".into(),
        url: url.into(),
        ..Default::default()
    }
}
fn signature(request: &RestRequest) -> &str {
    request.headers.get("X-BAPI-SIGN").unwrap()
}
fn body_bytes(request: &RestRequest) -> Option<Vec<u8>> {
    request.body.as_ref().map(|body| body.wire_bytes().unwrap())
}
fn response_cases() -> Vec<Value> {
    let first = fixture(vec![
        Reply::Network {
            url: None,
            method: None,
        },
        response(r#"{"retCode":10003,"retMsg":"invalid key"}"#),
    ]);
    let execution = ready(first.api.execute_with_receipt(get("/private"))).unwrap();
    assert_eq!(
        first.wire.requests.borrow().len(),
        2,
        "factory_response_native_attempts network"
    );
    assert_eq!(
        first.owner.base.auth_failure_streak(),
        2,
        "factory_response_native_auth_failure_unwind"
    );
    assert_eq!(
        &*first.owner.accesses.borrow(),
        &[0, 1],
        "factory_response_native_auth_calls"
    );
    assert_eq!(
        signature(&first.wire.requests.borrow()[0]),
        GET_ONE,
        "factory_response_native_first_signature"
    );
    assert_eq!(
        signature(&first.wire.requests.borrow()[1]),
        GET_TWO,
        "factory_response_native_retry_signature"
    );
    assert_eq!(
        first.host.failures.get(),
        1,
        "factory_response_native_common_failure"
    );
    assert_eq!(
        &*first.trace.borrow(),
        &[
            "start",
            "wire",
            "common-failure",
            "start",
            "wire",
            "common-response",
            "owner-base",
            "owner-base"
        ],
        "factory_response_native_hook_order"
    );
    assert_eq!(
        execution.response.body.json_value().unwrap()["retCode"],
        10003
    );
    let second = fixture(vec![
        response(r#"{"retCode":10002,"retMsg":"recv window rejected"}"#),
        response(r#"{"retCode":0,"retMsg":"OK"}"#),
    ]);
    second.owner.base.record_auth_failure();
    let execution = ready(second.api.execute_with_receipt(RestRequest {
        method: "post".into(),
        url: "/private".into(),
        body: Some(RestBody::Untransformed(json!({"x":1}))),
        ..Default::default()
    }))
    .unwrap();
    assert_eq!(
        second.wire.requests.borrow().len(),
        2,
        "factory_clock_native_attempts"
    );
    assert_eq!(
        second.owner.base.auth_failure_streak(),
        0,
        "factory_clock_native_auth_success"
    );
    assert_eq!(
        &*second.owner.accesses.borrow(),
        &[1],
        "factory_clock_native_auth_calls"
    );
    assert_eq!(
        second.clock_fetches.load(Ordering::SeqCst),
        1,
        "factory_clock_native_real_sync"
    );
    assert_eq!(
        second.clock.get_current_time_ms(),
        2000.0,
        "factory_clock_native_shared_offset"
    );
    let requests = second.wire.requests.borrow();
    assert_eq!(
        signature(&requests[0]),
        POST_ONE,
        "factory_clock_native_original_hmac"
    );
    assert_eq!(
        signature(&requests[1]),
        CLOCK_TWO,
        "factory_clock_native_reentry_hmac"
    );
    assert_eq!(
        body_bytes(&requests[0]),
        Some(br#"{"x":1}"#.to_vec()),
        "factory_clock_native_first_wire"
    );
    assert_eq!(
        body_bytes(&requests[1]),
        body_bytes(&requests[0]),
        "factory_clock_native_reentry_wire"
    );
    assert!(
        matches!(execution.config.body, Some(RestBody::Transformed(_))),
        "factory_clock_native_final_phase"
    );
    assert_eq!(
        &*second.trace.borrow(),
        &[
            "start",
            "wire",
            "common-response",
            "start",
            "wire",
            "common-response",
            "owner-base"
        ],
        "factory_clock_native_hook_order"
    );
    vec![
        json!({"id":"network-auth-unwind","attempts":first.wire.requests.borrow().len(),"authFailures":first.owner.base.auth_failure_streak(),"authSuccesses":0}),
        json!({"id":"clock-post-body-phase","attempts":second.wire.requests.borrow().len(),"authFailures":0,"authSuccesses":second.owner.accesses.borrow().len()}),
    ]
}
fn retry_config_cases() -> Vec<Value> {
    let inputs = [
        (
            "private-get-error-url-markets",
            "get",
            "/private",
            Some("/v5/market/instruments-info"),
            None,
            1,
        ),
        (
            "markets-get-error-url-private",
            "get",
            "/v5/market/instruments-info",
            Some("/private"),
            None,
            2,
        ),
        (
            "private-post-error-method-get",
            "post",
            "/private",
            None,
            Some("get"),
            1,
        ),
        (
            "private-get-error-method-post",
            "get",
            "/private",
            None,
            Some("post"),
            1,
        ),
        (
            "private-get-error-method-uppercase-get",
            "get",
            "/private",
            None,
            Some("GET"),
            2,
        ),
        (
            "signature-error-empty-method",
            "get",
            "/private",
            None,
            Some(""),
            2,
        ),
    ];
    let mut results = Vec::new();
    for (id, method, url, failure_url, failure_method, expected) in inputs {
        let signature_error = id == "signature-error-empty-method";
        let first_reply = if signature_error {
            Reply::RejectedMethod {
                body: RestResponseBody::Transformed(json!({"ret_msg":"error sign!"})),
                method: "",
            }
        } else {
            Reply::Network {
                url: failure_url,
                method: failure_method,
            }
        };
        let f = fixture(vec![
            first_reply,
            response(r#"{"retCode":0,"retMsg":"OK"}"#),
        ]);
        let mut request = get(url);
        request.method = method.into();
        if method == "post" {
            request.body = Some(RestBody::Untransformed(json!({"x":1})));
        }
        let result = ready(f.api.execute_with_receipt(request));
        assert_eq!(
            f.wire.requests.borrow().len(),
            expected,
            "factory_retry_config_native_attempts {id}"
        );
        assert_eq!(
            result.is_ok(),
            expected == 2,
            "factory_retry_config_native_outcome {id}"
        );
        assert_eq!(
            f.owner.accesses.borrow().len(),
            if expected == 2 { 2 } else { 0 },
            "factory_retry_config_native_auth {id}"
        );
        if id == "private-get-error-method-uppercase-get" {
            let requests = f.wire.requests.borrow();
            assert_eq!(
                requests
                    .iter()
                    .map(|request| request.method.as_str())
                    .collect::<Vec<_>>(),
                ["get", "get"],
                "factory_retry_config_native_uppercase_reentry_methods"
            );
            assert_eq!(
                signature(&requests[0]),
                GET_ONE,
                "factory_retry_config_native_uppercase_first_hmac"
            );
            assert_eq!(
                signature(&requests[1]),
                GET_TWO,
                "factory_retry_config_native_uppercase_retry_hmac"
            );
        }
        if signature_error {
            let requests = f.wire.requests.borrow();
            assert_eq!(
                requests
                    .iter()
                    .map(|request| request.method.as_str())
                    .collect::<Vec<_>>(),
                ["get", "get"],
                "factory_retry_config_native_empty_method_reentry_methods"
            );
            assert_eq!(
                signature(&requests[0]),
                GET_ONE,
                "factory_retry_config_native_empty_method_first_hmac"
            );
            assert_eq!(
                signature(&requests[1]),
                GET_TWO,
                "factory_retry_config_native_empty_method_retry_hmac"
            );
        }
        if let Ok(execution) = result {
            assert_eq!(
                execution.config.url, "/private",
                "factory_retry_config_native_effective_url"
            );
            assert_eq!(
                signature(&f.wire.requests.borrow()[1]),
                GET_TWO,
                "factory_retry_config_native_signature"
            );
        } else {
            assert_eq!(f.owner.base.auth_failure_streak(), 0);
        }
        results.push(json!({"id":id,"attempts":f.wire.requests.borrow().len(),"resolved":f.wire.requests.borrow().len()==2}));
    }
    assert_eq!(results.len(), 6, "factory_retry_config_native_inventory");
    results
}
fn observe(value: &Value) -> Value {
    match value {
        Value::String(text) => {
            json!({"kind":"string","units":text.encode_utf16().collect::<Vec<_>>()})
        }
        Value::Number(number) => {
            let n = number
                .as_f64()
                .unwrap_or_else(|| safe_cex_num::number_from_string(&number.to_string()));
            json!({"kind":"number","value":if n.is_finite() { json!(n) } else { json!(if n.is_sign_negative() {"-Infinity"} else {"Infinity"}) },"negativeZero":n==0.0 && n.is_sign_negative()})
        }
        Value::Null => json!({"kind":"null","value":null}),
        Value::Bool(value) => json!({"kind":"boolean","value":value}),
        _ => json!({"kind":"object","value":value}),
    }
}
fn transform_cases() -> Vec<Value> {
    let inputs = [
        ("native-json-object", "native", r#"{"retCode":0}"#),
        ("native-quoted-scalar", "native", r#""ready""#),
        (
            "native-quoted-json-object",
            "native",
            r#""{\"retCode\":0}""#,
        ),
        (
            "native-prior-transformed-string",
            "prior-transformed",
            r#"{"retCode":0}"#,
        ),
        ("physical-json-object", "physical", r#"{"retCode":0}"#),
        (
            "physical-bom-json-object",
            "physical",
            "\u{feff}{\"retCode\":0}",
        ),
        (
            "native-bom-remains-string",
            "native",
            "\u{feff}{\"retCode\":0}",
        ),
        ("native-invalid-json-remains-string", "native", "{broken"),
        ("native-empty-string", "native", ""),
        ("native-quoted-nul", "native", r#""\u0000""#),
        ("native-overflow-number", "native", "1e400"),
        ("native-lone-surrogate", "native", r#""\ud800""#),
        ("raw-null-skips-call", "raw", "null"),
        ("raw-empty-object-skips-missing-call", "raw", "{}"),
        (
            "raw-own-null-includes-skips-call",
            "raw",
            r#"{"includes":null}"#,
        ),
        ("raw-own-number-includes-throws", "raw", r#"{"includes":4}"#),
        ("raw-number-skips-missing-call", "raw", "7"),
        (
            "raw-array-exact-includes",
            "raw",
            r#"["There might be too much traffic or a configuration error"]"#,
        ),
        (
            "raw-string-substring-includes",
            "raw",
            r#""prefix There might be too much traffic or a configuration error suffix""#,
        ),
        ("raw-boolean-skips-missing-call", "raw", "false"),
    ];
    let mut results = Vec::new();
    for (id, kind, raw) in inputs {
        let raw = if kind == "raw" {
            format!("{{\"retCode\":10001,\"retMsg\":\"request rejected\",\"raw_response\":{raw}}}")
        } else {
            raw.into()
        };
        let body = match kind {
            "physical" => RestResponseBody::Encoded(raw.as_bytes().to_vec()),
            "prior-transformed" => RestResponseBody::Transformed(Value::String(raw)),
            _ => RestResponseBody::Untransformed(Value::String(raw)),
        };
        let f = fixture(vec![Reply::Response(body)]);
        let traffic = matches!(
            id,
            "raw-array-exact-includes" | "raw-string-substring-includes"
        );
        if traffic {
            *f.runtime.expected_report.borrow_mut() =
                Some("Exchange Error: Server was unavailable for a request, retrying...".into());
        }
        let result = ready(f.api.execute_with_receipt(get("/private")));
        assert_eq!(
            f.wire.requests.borrow().len(),
            1,
            "factory_transform_native_attempts {id}"
        );
        assert_eq!(
            signature(&f.wire.requests.borrow()[0]),
            GET_ONE,
            "factory_transform_native_hmac {id}"
        );
        let observation = if id == "native-lone-surrogate" {
            let error = result.unwrap_err();
            assert!(
                error
                    .message
                    .contains("Axios response representation refused"),
                "factory_transform_native_representation_refusal"
            );
            assert!(
                error.response.is_some(),
                "factory_transform_native_representation_response"
            );
            assert_eq!(
                f.host.responses.borrow().len(),
                0,
                "factory_transform_native_refusal_before_success_hook"
            );
            assert_eq!(
                f.host.failures.get(),
                1,
                "factory_transform_native_representation_failure_hook"
            );
            json!({"kind":"representation-refusal"})
        } else if id == "raw-own-number-includes-throws" {
            let error = result.unwrap_err();
            assert!(
                matches!(error.transport_error.as_deref(), Some(TransportError::JavaScript(cause)) if cause.name=="TypeError" && cause.message=="response.data?.raw_response?.includes is not a function"),
                "factory_transform_native_raw_includes_type_error"
            );
            assert_eq!(
                f.host.responses.borrow().len(),
                1,
                "factory_transform_native_raw_error_after_common_hook"
            );
            assert_eq!(
                f.host.failures.get(),
                0,
                "factory_transform_native_no_duplicate_common_failure"
            );
            json!({"kind":"TypeError","message":"response.data?.raw_response?.includes is not a function"})
        } else {
            let execution = result.unwrap();
            assert!(
                matches!(execution.response.body, RestResponseBody::Transformed(_)),
                "factory_transform_native_phase {id}"
            );
            let value = execution.response.body.clone().into_data();
            assert_eq!(
                f.host.responses.borrow().len(),
                1,
                "factory_transform_native_hook_count {id}"
            );
            assert_eq!(
                f.host.responses.borrow()[0],
                value,
                "factory_transform_native_before_common_hook {id}"
            );
            let expected = match id {
                "native-json-object"
                | "native-prior-transformed-string"
                | "physical-json-object"
                | "physical-bom-json-object" => json!({"retCode":0}),
                "native-quoted-scalar" => json!("ready"),
                "native-quoted-json-object" => json!(r#"{"retCode":0}"#),
                "native-bom-remains-string" => json!("\u{feff}{\"retCode\":0}"),
                "native-invalid-json-remains-string" => json!("{broken"),
                "native-empty-string" => json!(""),
                "native-quoted-nul" => json!("\0"),
                "native-overflow-number" => {
                    assert_eq!(
                        observe(&value),
                        json!({"kind":"number","value":"Infinity","negativeZero":false}),
                        "factory_transform_native_overflow_semantics"
                    );
                    value.clone()
                }
                _ => {
                    json!({"retCode":10001,"retMsg":"request rejected","raw_response":serde_json::from_str::<Value>(raw_for_id(id)).unwrap()})
                }
            };
            assert_eq!(value, expected, "factory_transform_native_value {id}");
            observe(&value)
        };
        let successful_auth = matches!(
            id,
            "native-json-object"
                | "native-prior-transformed-string"
                | "physical-json-object"
                | "physical-bom-json-object"
        );
        let reports = f.runtime.reports.borrow();
        assert_eq!(
            reports.len(),
            usize::from(traffic),
            "factory_transform_native_report_count {id}"
        );
        if traffic {
            let (error, context) = &reports[0];
            assert_eq!(
                error.name, "ExchangeError",
                "factory_transform_native_report_name {id}"
            );
            assert_eq!(
                error.message, "Exchange Error: Server was unavailable for a request, retrying...",
                "factory_transform_native_report_message {id}"
            );
            assert_eq!(
                context["tags"]["exchange"], "bybitinverse",
                "factory_transform_native_report_exchange {id}"
            );
            assert_eq!(
                context["tags"]["module"], "Exchange",
                "factory_transform_native_report_module {id}"
            );
            assert_eq!(
                context["tags"]["severity"], "error",
                "factory_transform_native_report_severity {id}"
            );
            assert_eq!(
                context["extra"]["originalMessage"], error.message,
                "factory_transform_native_report_original {id}"
            );
        }
        assert_eq!(
            f.emissions.get(),
            usize::from(traffic),
            "factory_transform_native_traffic_emission {id}"
        );
        assert_eq!(
            f.owner.accesses.borrow().len(),
            usize::from(successful_auth) + usize::from(traffic),
            "factory_transform_native_auth_and_emit_calls {id}"
        );
        assert_eq!(
            f.owner.base.auth_failure_streak(),
            0,
            "factory_transform_native_no_auth_failure {id}"
        );
        let console_calls = f
            .runtime
            .console
            .borrow()
            .iter()
            .filter(|(_, message)| message.starts_with("bybitinverse error "))
            .count();
        assert_eq!(
            console_calls,
            usize::from(kind == "raw" && id != "raw-own-number-includes-throws"),
            "factory_transform_native_console_branch {id}"
        );
        results.push(json!({"id":id,"result":observation,"authSuccesses":usize::from(successful_auth),"authFailures":0,"consoleCalls":console_calls,"emittedCalls":f.emissions.get()}));
    }
    assert_eq!(results.len(), 20, "factory_transform_native_inventory");
    results
}
fn raw_for_id(id: &str) -> &'static str {
    match id {
        "raw-null-skips-call" => "null",
        "raw-empty-object-skips-missing-call" => "{}",
        "raw-own-null-includes-skips-call" => r#"{"includes":null}"#,
        "raw-number-skips-missing-call" => "7",
        "raw-array-exact-includes" => {
            r#"["There might be too much traffic or a configuration error"]"#
        }
        "raw-string-substring-includes" => {
            r#""prefix There might be too much traffic or a configuration error suffix""#
        }
        "raw-boolean-skips-missing-call" => "false",
        _ => panic!("not a successful raw-response case"),
    }
}
fn source(script: &str, floor: usize) -> Value {
    let output = Command::new("node")
        .arg(format!("{}/scripts/{script}", env!("CARGO_MANIFEST_DIR")))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "source factory oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        value["declaredCases"], floor,
        "factory_source_declared_inventory"
    );
    assert_eq!(
        value["cases"].as_array().unwrap().len(),
        floor,
        "factory_source_executed_inventory"
    );
    value
}
#[test]
fn native_response_and_clock_reentry() {
    response_cases();
}
#[test]
fn native_retry_failure_config_and_original_read_guard() {
    retry_config_cases();
}
#[test]
fn native_astral_params_refusal_preserves_source_input_context() {
    api_factory_params_astral::collect_native();
}
#[test]
fn native_proxy_region_construction_matches_source_contract() {
    api_factory_proxy_region::collect_native();
}
#[test]
fn native_default_response_transform_and_raw_optional_call() {
    transform_cases();
}
fn compare_response_and_clock_reentry(native: &[Value]) {
    let source = source("create-api-response-boundaries.cjs", 2);
    for i in 0..2 {
        assert_eq!(native[i]["id"], source["cases"][i]["id"]);
        for field in ["attempts", "authFailures", "authSuccesses"] {
            assert_eq!(
                native[i][field], source["cases"][i]["source"][field],
                "factory_response_source_comparison {i}/{field}"
            );
        }
    }
}
fn compare_retry_failure_config_boundaries(native: &[Value]) {
    let source = source("create-api-retry-config-boundaries.cjs", 6);
    for i in 0..6 {
        assert_eq!(native[i]["id"], source["cases"][i]["id"]);
        if i == 2 {
            assert_eq!(native[i]["attempts"], 1, "CHOSEN original-read-token guard");
            assert_eq!(
                source["cases"][i]["source"]["attempts"], 2,
                "source trusts rewritten POST error.config method"
            );
        } else {
            for field in ["attempts", "resolved"] {
                assert_eq!(
                    native[i][field], source["cases"][i]["source"][field],
                    "factory_retry_source_comparison {i}/{field}"
                );
            }
        }
    }
}
fn compare_response_transform_boundaries(native: &[Value]) {
    let source = source("create-api-response-transform-boundaries.cjs", 20);
    for i in 0..20 {
        assert_eq!(native[i]["id"], source["cases"][i]["id"]);
        if i == 11 {
            assert_eq!(
                native[i]["result"]["kind"], "representation-refusal",
                "CHOSEN shared UTF8 string boundary"
            );
            assert_eq!(
                source["cases"][i]["source"]["result"]["units"],
                json!([0xd800])
            );
        } else {
            assert_eq!(
                native[i]["result"], source["cases"][i]["source"]["result"],
                "factory_transform_source_comparison {i}"
            );
        }
        for field in [
            "authFailures",
            "authSuccesses",
            "consoleCalls",
            "emittedCalls",
        ] {
            assert_eq!(
                native[i][field], source["cases"][i]["source"][field],
                "factory_transform_source_effects {i}/{field}"
            );
        }
    }
}
#[test]
fn native_how_query_ownership_survives_all_forwarders() {
    let f = fixture_with_query_owner(vec![response(r#"{"retCode":0}"#)], false);
    assert!(!f.api.query_in_url(), "factory_how_native_query_ownership");
    let mut request = get("/private");
    request.params = Some(json!({"symbol":"BTC USDT"}));
    let execution = ready(f.api.execute_with_receipt(request)).unwrap();
    let requests = f.wire.requests.borrow();
    assert_eq!(requests.len(), 1, "factory_how_native_attempts");
    assert_eq!(
        requests[0].url, "/private",
        "factory_how_native_query_not_consumed"
    );
    assert_eq!(
        requests[0].params,
        Some(json!({"symbol":"BTC USDT","requestId":"id-1"})),
        "factory_how_native_params_retained"
    );
    assert_eq!(
        signature(&requests[0]),
        "2801ec89d9812d5b1a4c3f27007d8dd6992219fbfcece7ae5a6942e28947883a",
        "factory_how_native_signature"
    );
    assert_eq!(
        execution.config.params, requests[0].params,
        "factory_how_native_effective_params"
    );
}
#[test]
fn native_owned_options_snapshot_boundary() {
    let mut f = fixture(vec![
        response(r#"{"retCode":0}"#),
        response(r#"{"retCode":0}"#),
    ]);
    ready(f.api.execute_with_receipt(get("/private"))).unwrap();
    f.caller_options.key = "changed-key".into();
    f.caller_options.secret = "changed-secret".into();
    ready(f.api.execute_with_receipt(get("/private"))).unwrap();
    let requests = f.wire.requests.borrow();
    assert_eq!(requests.len(), 2, "factory_snapshot_native_attempts");
    assert_eq!(
        signature(&requests[0]),
        GET_ONE,
        "factory_snapshot_native_first_signature"
    );
    assert_eq!(
        signature(&requests[1]),
        GET_TWO,
        "factory_snapshot_native_owned_credentials"
    );
    assert_eq!(
        requests[1]
            .headers
            .get("X-BAPI-API-KEY")
            .map(String::as_str),
        Some("key"),
        "factory_snapshot_native_key"
    );
}
#[test]
fn native_weak_owner_boundary_is_visible() {
    let f = fixture(vec![response(r#"{"retCode":0}"#)]);
    let weak = Rc::downgrade(&f.owner);
    drop(f.owner);
    assert!(
        weak.upgrade().is_none(),
        "factory_owner_native_not_retained"
    );
    let error = ready(f.api.execute_with_receipt(get("/private"))).unwrap_err();
    assert_eq!(
        error.message, "Bybit exchange owner is unavailable",
        "factory_owner_native_visible_refusal"
    );
    assert_eq!(
        f.wire.requests.borrow().len(),
        1,
        "factory_owner_native_completed_attempt"
    );
    assert_eq!(
        f.host.responses.borrow().len(),
        1,
        "factory_owner_native_common_hook_precedes_refusal"
    );
    assert!(
        !error.is_retryable_network(),
        "factory_owner_native_no_invented_network"
    );
}

#[test]
#[ignore = "requires frozen workspace Node dependencies; source/native factory comparison"]
fn source_and_native_factory_surface() {
    let responses = response_cases();
    let retry_configs = retry_config_cases();
    let transforms = transform_cases();
    let conditions = retry_condition_cases();
    let probes = api_factory_probe::collect_native();
    let timeouts = api_factory_timeout::collect_native();
    let clock_budgets = api_factory_clock_budget::collect_native();
    let params = api_factory_params::collect_native();
    let proxy_regions = api_factory_proxy_region::collect_native();
    let astral_params = api_factory_params_astral::collect_native();
    native_how_query_ownership_survives_all_forwarders();
    native_owned_options_snapshot_boundary();
    native_weak_owner_boundary_is_visible();
    assert_eq!(
        responses.len()
            + retry_configs.len()
            + transforms.len()
            + conditions.len()
            + probes.len()
            + timeouts.len()
            + clock_budgets.len()
            + params.len(),
        87,
        "factory_native_complete_corpus_before_source"
    );
    compare_response_and_clock_reentry(&responses);
    compare_retry_failure_config_boundaries(&retry_configs);
    compare_response_transform_boundaries(&transforms);
    compare_retry_condition_boundaries(&conditions);
    api_factory_probe::compare_source(&probes);
    api_factory_timeout::compare_source(&timeouts);
    api_factory_clock_budget::compare_source(&clock_budgets);
    api_factory_params::compare_source(&params);
    api_factory_proxy_region::compare_source(&proxy_regions);
    api_factory_params_astral::compare_source(&astral_params);
}

fn retry_condition_cases() -> Vec<Value> {
    let inputs = [
        (
            "lower-string-substring",
            r#"{"ret_msg":"prefix error sign! suffix"}"#,
            "retry",
        ),
        (
            "camel-string-substring",
            r#"{"retMsg":"prefix error sign! suffix"}"#,
            "retry",
        ),
        (
            "lower-array-exact",
            r#"{"ret_msg":["other","error sign!"]}"#,
            "original",
        ),
        (
            "camel-array-exact",
            r#"{"retMsg":["error sign!"]}"#,
            "original",
        ),
        (
            "lower-array-substring-does-not-match",
            r#"{"ret_msg":["prefix error sign! suffix"]}"#,
            "original",
        ),
        (
            "lower-empty-array-falls-through",
            r#"{"ret_msg":[],"retMsg":["error sign!"]}"#,
            "original",
        ),
        (
            "lower-null-falls-through",
            r#"{"ret_msg":null,"retMsg":["error sign!"]}"#,
            "original",
        ),
        ("lower-empty-object", r#"{"ret_msg":{}}"#, "original"),
        ("lower-number", r#"{"ret_msg":17}"#, "original"),
        ("lower-boolean", r#"{"ret_msg":false}"#, "original"),
        (
            "lower-own-includes-null",
            r#"{"ret_msg":{"includes":null}}"#,
            "original",
        ),
        (
            "lower-own-includes-string",
            r#"{"ret_msg":{"includes":"error sign!"}}"#,
            "original",
        ),
        (
            "lower-array-is-not-a-message",
            r#"{"ret_msg":["error sign!"],"retMsg":{}}"#,
            "original",
        ),
        (
            "lower-string-true-skips-camel-boolean",
            r#"{"ret_msg":"error sign!","retMsg":false}"#,
            "retry",
        ),
        (
            "lower-array-false-reaches-camel-object",
            r#"{"ret_msg":[],"retMsg":{}}"#,
            "original",
        ),
        (
            "lower-object-yields-to-positive-camel",
            r#"{"ret_msg":{},"retMsg":"error sign!"}"#,
            "retry",
        ),
        (
            "lower-empty-string-reaches-camel-number",
            r#"{"ret_msg":"","retMsg":0}"#,
            "original",
        ),
        (
            "camel-array-substring-does-not-match",
            r#"{"retMsg":["prefix error sign! suffix"]}"#,
            "original",
        ),
        (
            "both-null-no-signature-retry",
            r#"{"ret_msg":null,"retMsg":null}"#,
            "original",
        ),
        (
            "json-proto-key-does-not-install-includes",
            r#"{"ret_msg":{"__proto__":{"includes":"error sign!"}}}"#,
            "original",
        ),
        (
            "code-10004-without-sign-text",
            r#"{"retCode":10004,"retMsg":"signature mismatch"}"#,
            "retry",
        ),
        (
            "capitalised-error-sign-wording",
            r#"{"retCode":10001,"retMsg":"Error sign, please check your signature generation algorithm"}"#,
            "retry",
        ),
    ];
    assert_eq!(inputs.len(), 22, "factory_condition_native_declared_cases");
    let mut results = Vec::new();
    let mut totals = [0_usize; 3];
    for (id, raw, expected) in inputs {
        let body: Value = serde_json::from_str(raw).unwrap();
        let mut request = get("/private");
        request.method = "post".into();
        request.body = Some(RestBody::Untransformed(json!({"request":true})));
        let f = fixture(vec![
            Reply::Rejected(RestResponseBody::Encoded(raw.as_bytes().to_vec())),
            response(r#"{"retCode":0,"retMsg":"OK"}"#),
        ]);
        let result = ready(f.api.execute_with_receipt(request));
        let attempts = f.wire.requests.borrow().len();
        assert_eq!(
            attempts,
            if expected == "retry" { 2 } else { 1 },
            "factory_condition_native_attempts {id}"
        );
        assert_eq!(
            f.host.failures.get(),
            1,
            "factory_condition_native_common_failure {id}"
        );
        assert_eq!(
            f.owner.base.auth_failure_streak(),
            0,
            "factory_condition_native_no_auth_failure {id}"
        );
        assert!(
            f.runtime.console.borrow().is_empty(),
            "factory_condition_native_no_console {id}"
        );
        let outcome = match expected {
            "retry" => {
                totals[0] += 1;
                let result = result.unwrap();
                assert_eq!(
                    result.response.body.clone().into_data()["retCode"],
                    0,
                    "factory_condition_native_result {id}"
                );
                assert_eq!(
                    f.owner.accesses.borrow().len(),
                    2,
                    "factory_condition_native_unwind_auth {id}"
                );
                "resolved-after-retry"
            }
            "original" => {
                totals[1] += 1;
                let error = result.unwrap_err();
                assert_eq!(
                    error.code.as_deref(),
                    Some("ERR_BAD_RESPONSE"),
                    "factory_condition_native_original_code {id}"
                );
                assert_eq!(
                    error.message, "original venue rejection",
                    "factory_condition_native_original_message {id}"
                );
                assert_eq!(
                    error.context.as_deref(),
                    Some(&json!({"retainedCause":71})),
                    "factory_condition_native_original_cause {id}"
                );
                assert_eq!(
                    error.response.unwrap().body,
                    RestResponseBody::Transformed(body),
                    "factory_condition_native_original_payload {id}"
                );
                assert!(
                    f.owner.accesses.borrow().is_empty(),
                    "factory_condition_native_original_no_auth {id}"
                );
                "original-rejection"
            }
            _ => unreachable!(),
        };
        results.push(json!({"id":id,"attempts":attempts,"outcome":outcome,"authFailures":f.owner.base.auth_failure_streak(),"authSuccesses":f.owner.accesses.borrow().len()}));
    }
    assert_eq!(totals, [6, 16, 0], "factory_condition_native_branch_floors");
    assert_eq!(results.len(), 22, "factory_condition_native_executed_cases");
    results
}
#[test]
fn native_fallible_signature_retry_conditions() {
    retry_condition_cases();
}
fn compare_retry_condition_boundaries(native: &[Value]) {
    let source = source("create-api-retry-condition-boundaries.cjs", 22);
    for i in 0..22 {
        assert_eq!(
            native[i]["id"], source["cases"][i]["id"],
            "factory_condition_source_id {i}"
        );
        for field in ["attempts", "outcome", "authFailures", "authSuccesses"] {
            assert_eq!(
                native[i][field], source["cases"][i]["source"][field],
                "factory_condition_source_value {i}/{field}"
            );
        }
    }
}

#[test]
fn native_environment_probe_lifecycle() {
    api_factory_probe::collect_native();
}
#[test]
fn native_whole_attempt_timeout_boundary() {
    api_factory_timeout::collect_native();
}

#[test]
fn native_clock_reentry_retains_consumed_retry_budget() {
    api_factory_clock_budget::collect_native();
}

#[test]
fn native_params_spread_matches_both_carriers() {
    api_factory_params::collect_native();
}

// Resolved sign rejections re-sign once through the whole pipeline; the stamp is shared
// with the rejected-response condition, and each retry unwind reprocesses the response.
#[test]
fn native_resolved_signature_rejection_resends_once() {
    const SIGN: &str = r#"{"retCode":10004,"retMsg":"error sign!"}"#;
    let cases: [(&str, Vec<Reply>, (usize, f64, u8, usize)); 4] = [
        (
            "resolved-sign-then-ok",
            vec![response(SIGN), response(r#"{"retCode":0,"retMsg":"OK"}"#)],
            (2, 0., 0, 1),
        ),
        (
            "resolved-sign-twice",
            vec![response(SIGN), response(SIGN)],
            (2, 10004., 1, 0),
        ),
        (
            "rejected-then-resolved-sign",
            vec![
                Reply::Rejected(RestResponseBody::Encoded(SIGN.as_bytes().to_vec())),
                response(SIGN),
            ],
            (2, 10004., 2, 0),
        ),
        (
            "resolved-other-error",
            vec![response(r#"{"retCode":10001,"retMsg":"params error"}"#)],
            (1, 10001., 0, 0),
        ),
    ];
    let mut native = Vec::new();
    for (id, replies, (attempts, ret_code, failures, successes)) in cases {
        let f = fixture(replies);
        let mut request = get("/private");
        request.method = "post".into();
        request.body = Some(RestBody::Untransformed(json!({"request":true})));
        let result = ready(f.api.execute_with_receipt(request)).unwrap();
        let requests = f.wire.requests.borrow();
        assert_eq!(
            requests.len(),
            attempts,
            "factory_resend_native_attempts {id}"
        );
        let code = result.response.body.clone().into_data()["retCode"].as_f64();
        assert_eq!(
            code,
            Some(ret_code),
            "factory_resend_native_final_code {id}"
        );
        let streak = f.owner.base.auth_failure_streak();
        assert_eq!(streak, failures, "factory_resend_native_auth_failures {id}");
        let recorded = f.owner.accesses.borrow().len() - usize::from(streak);
        assert_eq!(
            recorded, successes,
            "factory_resend_native_auth_successes {id}"
        );
        native.push(json!({"id":id,"attempts":attempts,"retCode":ret_code,"authFailures":streak,"authSuccesses":recorded}));
    }
    let source = source("create-api-signature-resend-boundaries.cjs", 4);
    for (index, case) in native.iter().enumerate() {
        let source = &source["cases"][index];
        assert_eq!(case["id"], source["id"], "factory_resend_source_id {index}");
        for field in ["attempts", "retCode", "authFailures", "authSuccesses"] {
            assert_eq!(
                case[field].as_f64(),
                source["source"][field].as_f64(),
                "factory_resend_source_value {index}/{field}"
            );
        }
    }
}

// The per-request namespace overrides axios-retry's default, while resolved
// signature envelopes use the independent response interceptor.
#[test]
fn native_request_retry_override_retains_source_budget() {
    const SIGN: &str = r#"{"retCode":10004,"retMsg":"error sign!"}"#;
    let cases = [
        ("rejected-sign-zero", json!(0), true, false, 1, true),
        ("resolved-sign-zero", json!(0), false, false, 2, false),
        ("network-zero", json!(0), false, true, 1, true),
        ("network-one", json!(1), false, true, 2, true),
        ("network-fraction", json!(1.5), false, true, 3, true),
        ("network-five", json!(5), false, true, 6, true),
    ];
    let mut native = Vec::new();
    for (id, budget, rejected_sign, network, attempts, rejects) in cases {
        let replies = if network {
            (0..7)
                .map(|_| Reply::Network {
                    url: None,
                    method: None,
                })
                .collect()
        } else if rejected_sign {
            vec![
                Reply::Rejected(RestResponseBody::Encoded(SIGN.as_bytes().to_vec())),
                response(r#"{"retCode":0,"retMsg":"OK"}"#),
            ]
        } else {
            vec![response(SIGN), response(r#"{"retCode":0,"retMsg":"OK"}"#)]
        };
        let f = fixture(replies);
        let mut request = get("/private");
        if !network {
            request.method = "post".into();
            request.body = Some(RestBody::Untransformed(json!({"request":true})));
        }
        request.config_extensions.insert(
            "axios-retry".into(),
            safe_cex_types::retained_json::RetainedJsonInput::from_value(
                json!({"retries": budget}),
            )
            .unwrap(),
        );
        let result = ready(f.api.execute_with_receipt(request));
        assert_eq!(
            f.wire.requests.borrow().len(),
            attempts,
            "request_retry_budget_attempts {id}"
        );
        assert_eq!(
            result.is_err(),
            rejects,
            "request_retry_budget_outcome {id}"
        );
        native.push(json!({"id":id, "attempts":attempts, "rejected":rejects}));
    }
    let source = source("create-api-request-retry-boundaries.cjs", 6);
    for (index, case) in native.iter().enumerate() {
        assert_eq!(
            source["cases"][index]["id"], case["id"],
            "request_retry_budget_source_inventory"
        );
        for field in ["attempts", "rejected"] {
            assert_eq!(
                source["cases"][index]["source"][field], case[field],
                "request_retry_budget_source {index}/{field}"
            );
        }
    }
    for budget in [0, 1] {
        let network = || Reply::Network {
            url: None,
            method: None,
        };
        let mut replies: Vec<_> = (0..budget).map(|_| network()).collect();
        replies.push(response(
            r#"{"retCode":10002,"retMsg":"recv window rejected"}"#,
        ));
        replies.push(network());
        replies.push(response(r#"{"retCode":0,"retMsg":"OK"}"#));
        let f = fixture(replies);
        let mut request = get("/private");
        request.config_extensions.insert(
            "axios-retry".into(),
            safe_cex_types::retained_json::RetainedJsonInput::from_value(json!({"retries":budget}))
                .unwrap(),
        );
        assert!(
            ready(f.api.execute_with_receipt(request)).is_err(),
            "request_retry_clock_budget_exhausted {budget}"
        );
        assert_eq!(
            f.wire.requests.borrow().len(),
            budget + 2,
            "request_retry_clock_budget_retained {budget}"
        );
        assert_eq!(f.clock_fetches.load(Ordering::SeqCst), 1);
    }
}
