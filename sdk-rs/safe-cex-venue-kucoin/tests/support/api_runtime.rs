use safe_cex_exchange::{
    http::{HttpClientOptions, HttpFuture, HttpHost},
    runtime::{ExchangeError, ExchangeFuture, ExchangeRuntime, ExchangeTask},
};
use safe_cex_transport::{
    NetworkFailure, RestAdapter, RestBody, RestFailure, RestFuture, RestRequest, RestResponse,
    TransportError, TransportFuture, rate_limit::RateLimitClock,
};
use safe_cex_types::ExchangeName;
use safe_cex_venue_kucoin::api::{create_api, create_kucoin_spot_api};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    future::Future,
    pin::Pin,
    process::Command,
    rc::Rc,
    sync::{Arc, Mutex},
    task::{Context, Poll, Waker},
};
struct Clock(Cell<u64>, RefCell<Vec<u64>>);
impl RateLimitClock for Clock {
    fn now_ms(&self) -> u64 {
        self.0.get()
    }
    fn sleep_ms(&self, delay: u64) -> TransportFuture<'_, ()> {
        assert_eq!(delay, 0, "KuCoin uses axios-retry default zero delay");
        self.1.borrow_mut().push(delay);
        Box::pin(async { Ok(()) })
    }
}
struct Runtime(Rc<Clock>);
impl ExchangeRuntime for Runtime {
    fn monotonic_now_ms(&self) -> f64 {
        self.0.0.get() as f64
    }
    fn now_ms(&self) -> f64 {
        self.0.0.get() as f64
    }
    fn sleep(&self, _: u64) -> ExchangeFuture<'static, ()> {
        panic!("unexpected sleep")
    }
    fn spawn_local(&self, _: Pin<Box<dyn Future<Output = ()>>>) -> Box<dyn ExchangeTask> {
        panic!("unexpected task")
    }
    fn console_values(&self, _: &str, _: &[safe_cex_exchange::js_runtime::value::Js]) {}
    fn unique_id(&self) -> String {
        "fixture-id".into()
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
    fn report_exception(&self, _: &ExchangeError, _: &Value) {}
    fn execute_worker_request(&self, _: Value) -> ExchangeFuture<'static, Value> {
        panic!("unexpected worker")
    }
}
struct Host;
impl HttpHost for Host {
    fn execute(&self, adapter: Arc<dyn RestAdapter>, request: RestRequest) -> HttpFuture<'_> {
        Box::pin(async move { adapter.execute_with_receipt(request).await })
    }
    fn enhance_error(&self, error: RestFailure, _: &ExchangeName) -> RestFailure {
        error
    }
    fn emit(&self, _: &str, _: &str) -> Result<(), String> {
        Ok(())
    }
    fn note_success(&self, _: &ExchangeName, _: &RestResponse) {}
    fn note_failure(&self, _: &ExchangeName, _: &RestFailure) {}
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
struct Wire {
    case: Value,
    clock: Rc<Clock>,
    calls: RefCell<Vec<Value>>,
}
impl RestAdapter for Wire {
    fn execute(&self, request: RestRequest) -> RestFuture<'_, RestResponse> {
        Box::pin(async move {
            let body = request.body.as_ref().map(|body| match body {
                // Observe the inner adapter's data BEFORE network byte projection.
                RestBody::Transformed(data) => data.value().clone(),
                RestBody::Encoded(bytes) => json!(String::from_utf8(bytes.clone()).unwrap()),
                RestBody::Untransformed(_) => panic!("inner adapter received untransformed data"),
            });
            let mut row = json!({"url":request.url,"method":request.method,"data":body,"headers":request.headers});
            let elapsed = self.case["elapsed"].as_u64().unwrap_or(0);
            if elapsed > 0 {
                row["timeout"] = json!(request.timeout_ms);
            }
            self.calls.borrow_mut().push(row);
            self.clock.0.set(self.clock.0.get() + elapsed);
            if self.calls.borrow().len()
                <= self.case["extra"]["failures"].as_u64().unwrap_or(3) as usize
            {
                let mut error = RestFailure::from_transport(
                    TransportError::Network(NetworkFailure::from_code(
                        "fixture failure".into(),
                        Some("ECONNRESET".into()),
                    )),
                    request,
                );
                if self.case["failure"] == "response" {
                    error.response = Some(Box::new(RestResponse {
                        status: 503,
                        reason: Some("Unavailable".into()),
                        headers: Default::default(),
                        body: safe_cex_transport::RestResponseBody::Encoded(
                            b"{\"code\":\"fixture-error\"}".to_vec(),
                        ),
                    }));
                }
                Err(error)
            } else {
                Ok(RestResponse {
                    status: 200,
                    reason: Some("OK".into()),
                    headers: Default::default(),
                    body: safe_cex_transport::RestResponseBody::Encoded(
                        b"{\"code\":\"200000\"}".to_vec(),
                    ),
                })
            }
        })
    }
}
fn ready<T>(future: impl Future<Output = T>) -> T {
    match std::pin::pin!(future)
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("unexpected pending"),
    }
}
#[test]
fn real_factories_match_live_axios_retry_signatures_wire_and_budget() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/api-retry-capture.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(cases.len(), 14);
    for (index, case) in cases.iter().enumerate() {
        let actual = run_with_expected_schedules(case, Some([3, 3, 0, 3, 0, 2, 0][index % 7]));
        assert_eq!(
            actual["wire"], case["wire"],
            "KuCoin live factory wire/signature/deadline {index}"
        );
        assert_eq!(
            actual["outcome"], case["outcome"],
            "KuCoin live factory outcome {index}"
        );
    }
}
fn run(case: &Value) -> Value {
    run_with_expected_schedules(case, None)
}
#[expect(clippy::arc_with_non_send_sync)]
fn run_with_expected_schedules(case: &Value, expected: Option<usize>) -> Value {
    let clock = Rc::new(Clock(Cell::new(100000), RefCell::new(Vec::new())));
    let wire = Arc::new(Wire {
        case: case.clone(),
        clock: clock.clone(),
        calls: RefCell::new(vec![]),
    });
    let runtime = Rc::new(Runtime(clock.clone()));
    let mut options =
        json!({"key":"fixture-key","secret":"fixture-secret","passphrase":"fixture-pass"});
    if let Some(extra) = case["extra"]["options"].as_object() {
        options.as_object_mut().unwrap().extend(extra.clone());
    }
    let options = serde_json::from_value(options).unwrap();
    let signer = Arc::new(Signer(RefCell::new(vec![])));
    let http = HttpClientOptions {
        response_transform: None,
        params_serializer: None,
        exchange: ExchangeName::Kucoin,
        options,
        request: RestRequest::default(),
        provider: None,
        signer: (case["extra"]["remote"] == true)
            .then(|| signer.clone() as Arc<dyn safe_cex_transport::signing::RemoteSigner>),
        account_name: None,
        adapter: wire.clone(),
        host: Rc::new(Host),
        runtime,
        store: None,
        stats: Some(Arc::new(Mutex::new(Default::default()))),
        logs: Some(Arc::new(Mutex::new(Default::default()))),
        every: vec![],
        periodic: vec![],
        next_id: Arc::new(|| "fixture".into()),
        signing_now: Arc::new(|| 1700000000000),
    };
    let mut virtual_clock = safe_cex_store::virtual_clock::VirtualClock::default();
    let time = std::sync::atomic::AtomicU64::new(1700000000000);
    virtual_clock.now =
        Arc::new(move || time.fetch_add(1, std::sync::atomic::Ordering::SeqCst) as f64);
    let registry = safe_cex_transport::TransportRegistry::default();
    let request = RestRequest {
        method: case["method"].as_str().unwrap().into(),
        url: case["url"].as_str().unwrap().into(),
        params: Some(json!({"q":"a b"})),
        body: case["dataPresent"]
            .as_bool()
            .unwrap_or(!case["data"].is_null())
            .then(|| RestBody::Untransformed(case["data"].clone())),
        headers: case["extra"]["request"]["headers"]
            .as_object()
            .map(|m| {
                m.iter()
                    .map(|(k, v)| (k.clone(), v.as_str().unwrap().into()))
                    .collect()
            })
            .unwrap_or_default(),
        ..Default::default()
    };
    let result = if case["venue"] == "kucoin" {
        let api = ready(create_api(http, &registry, virtual_clock, clock.clone())).unwrap();
        ready(api.execute(request))
    } else {
        let api = ready(create_kucoin_spot_api(
            http,
            &registry,
            virtual_clock,
            clock.clone(),
        ))
        .unwrap();
        ready(api.execute(request))
    };
    if let Some(expected) = expected {
        assert_eq!(
            *clock.1.borrow(),
            vec![0; expected],
            "KuCoin native independent zero-delay scheduling floor"
        );
    }
    let outcome = match result {
        Ok(response) => json!({"ok":response.body.json_value().unwrap()}),
        Err(error) => json!({"error":error.message,"status":error.response.map(|r|r.status)}),
    };
    json!({"wire": *wire.calls.borrow(), "outcome":outcome,"signRequests": *signer.0.borrow()})
}
fn assert_matches(case: &Value) {
    let actual = run(case);
    assert_eq!(
        actual["wire"], case["wire"],
        "live KuCoin factory wire/signature/deadline: {case}"
    );
    assert_eq!(
        actual["outcome"], case["outcome"],
        "live KuCoin factory outcome: {case}"
    );
}
struct Signer(RefCell<Vec<Value>>);
impl safe_cex_transport::signing::RemoteSigner for Signer {
    fn sign(
        &self,
        request: safe_cex_transport::signing::RemoteSignerRequest,
    ) -> TransportFuture<'_, safe_cex_transport::signing::RemoteSignerResponse> {
        use safe_cex_transport::signing::*;
        Box::pin(async move {
            let RemoteSignerRequest::Http(request) = request else {
                panic!("unexpected WS auth");
            };
            self.0
                .borrow_mut()
                .push(serde_json::to_value(&request.config).unwrap());
            Ok(RemoteSignerResponse::Http(HttpSigningResponse {
                id: request.id,
                error: None,
                config: Some(SignedConfig {
                    url: Some(request.config.url),
                    params: request.config.params,
                    data: request.config.data,
                    headers: request.config.headers,
                }),
            }))
        })
    }
}
fn runtime_cases() -> Vec<Value> {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/api-runtime.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(cases.len(), 48);
    cases
}
#[test]
fn local_factories_match_axios_headers_public_paths_and_body_shapes() {
    let cases = runtime_cases();
    for case in cases
        .iter()
        .filter(|c| c["extra"]["remote"] != true && c["extra"]["finding"].is_null())
    {
        assert_matches(case);
    }
}
#[test]
fn remote_factories_match_live_signer_input_and_echo_adapter_output() {
    let cases = runtime_cases();
    let mut count = 0;
    let mut objects = 0;
    for case in cases.iter().filter(|c| c["extra"]["remote"] == true) {
        count += 1;
        let actual = run(case);
        assert!(actual["signRequests"][0].get("baseURL").is_some());
        assert!(actual["signRequests"][0].get("baseUrl").is_none());
        if case["wire"][0]["data"].is_object() {
            objects += 1;
        }
        assert_eq!(
            actual["wire"], case["wire"],
            "remote echoed-body adapter type and value"
        );
        assert_eq!(
            actual["signRequests"], case["signRequests"],
            "remote transformed signer input"
        );
        assert_eq!(actual["outcome"], case["outcome"]);
    }
    assert_eq!(count, 18);
    assert_eq!(objects, 4, "four echo objects survive to the inner adapter");
}

#[test]
fn factory_source_public_branch_mutation_fails_by_name() {
    let output = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "api_runtime::local_factories_match_axios_headers_public_paths_and_body_shapes",
            "--nocapture",
        ])
        .env(
            "NODE_OPTIONS",
            format!(
                "--require {}/scripts/oracle-controls.cjs",
                env!("CARGO_MANIFEST_DIR")
            ),
        )
        .env("KUCOIN_ORACLE_MUTATION", "factory-public")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(101));
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("live KuCoin factory wire/signature/deadline")
    );
}
#[test]
fn missing_remote_adapter_dependency_fails_loudly() {
    let output = Command::new("node")
        .arg("--require")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/oracle-controls.cjs"
        ))
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/api-runtime.cjs"
        ))
        .env("KUCOIN_ORACLE_MUTATION", "missing-nanoid")
        .output()
        .unwrap();
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success());
    assert!(
        error.contains("MODULE_NOT_FOUND") && error.contains("kucoin-intentionally-missing-nanoid"),
        "{error}"
    );
}

#[test]
fn form_body_matches_shared_form_transform() {
    let mut count = 0;
    for case in runtime_cases()
        .iter()
        .filter(|c| c["extra"]["finding"] == "form-body")
    {
        assert_eq!(case["wire"][0]["data"], "x=a+b&y=2");
        assert_matches(case);
        count += 1;
    }
    assert_eq!(count, 2);
}

#[test]
fn factory_string_header_and_option_alias_boundaries() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/api-boundaries.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rows: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(rows.len(), 20);
    for row in rows {
        let mut case = row["case"].clone();
        match row["kind"].as_str().unwrap() {
            "alias" => {
                assert_eq!(
                    row["before"]["options"]["extra"]["kucoinBroker"]["partner"],
                    "before"
                );
                assert_eq!(
                    row["after"]["options"]["extra"]["kucoinBroker"]["partner"],
                    "after"
                );
                assert_eq!(
                    case["wire"][0]["headers"]["KC-API-PARTNER"], "after",
                    "source observes caller mutation"
                );
                case["extra"] = row["before"].clone();
                let rust = run(&case);
                assert_eq!(
                    rust["wire"][0]["headers"]["KC-API-PARTNER"], "before",
                    "owned factory options retain snapshot"
                );
            }
            "option-capture" => {
                let rust = run(&case);
                let source = &case["wire"][0]["headers"];
                let spot = case["venue"] == "kucoinspot";
                if let Some(key) = case["extra"]["afterCreate"].get("key") {
                    assert_eq!(case["optionViews"]["before"]["key"], "fixture-key");
                    assert_eq!(&case["optionViews"]["after"]["key"], key);
                    assert_eq!(
                        source["KC-API-KEY"],
                        if spot {
                            json!("changed-key")
                        } else {
                            json!("fixture-key")
                        }
                    );
                    assert_eq!(rust["wire"][0]["headers"]["KC-API-KEY"], "fixture-key");
                } else {
                    assert!(
                        case["optionViews"]["before"]
                            .get("publicDataOnly")
                            .is_none()
                    );
                    assert_eq!(case["optionViews"]["after"]["publicDataOnly"], true);
                    assert_eq!(
                        source.get("KC-API-SIGN").is_none(),
                        spot,
                        "Spot reads live privacy; futures captures authentication at creation"
                    );
                    assert!(
                        rust["wire"][0]["headers"].get("KC-API-SIGN").is_some(),
                        "Rust keeps the original owned privacy value"
                    );
                }
            }
            "header" => {
                if row["value"] == false {
                    assert!(
                        case["wire"][0]["headers"].get("KC-API-TIMESTAMP").is_none(),
                        "false disables an Axios header"
                    );
                    assert!(
                        serde_json::from_value::<indexmap::IndexMap<String, String>>(
                            case["extra"]["request"]["headers"].clone()
                        )
                        .is_err(),
                        "string header boundary refuses false sentinel"
                    );
                } else {
                    assert_matches(&case);
                }
            }
            "string" => {
                let value = serde_json::from_str::<Value>(row["rawData"].as_str().unwrap());
                let hashes = case["hashes"].as_array().unwrap();
                assert_eq!(hashes.len(), 2, "passphrase and request HMACs are captured");
                for hash in hashes {
                    let units: Vec<u16> =
                        serde_json::from_value(hash["preimageUtf16"].clone()).unwrap();
                    let hashed = String::from_utf16_lossy(&units);
                    assert_eq!(hash["preimage"], hashed, "Node hashes well-formed UTF-8");
                    assert_eq!(
                        hash["digest"],
                        safe_cex_crypto::hmac_sha256(
                            b"fixture-secret",
                            hashed.as_bytes(),
                            safe_cex_crypto::Encoding::Base64,
                        ),
                        "retained preimage produces the actual source digest"
                    );
                }
                if row["refused"] == true {
                    let surrogate = match row["rawData"].as_str().unwrap() {
                        "\"\\ud800\"" => 0xd800_u16,
                        "\"\\udfff\"" => 0xdfff_u16,
                        other => panic!("unexpected refused source string {other}"),
                    };
                    let query = if case["venue"] == "kucoinspot" {
                        "a%20b"
                    } else {
                        "a b"
                    };
                    let prefix = format!("1700000000000POST/api/v1/orders?q={query}");
                    let mut expected_units: Vec<u16> = prefix.encode_utf16().collect();
                    expected_units.push(surrogate);
                    let request_hash = &hashes[1];
                    assert_eq!(
                        request_hash["preimageUtf16"],
                        json!(expected_units),
                        "original HMAC input retains the exact lone UTF-16 unit"
                    );
                    assert_eq!(
                        request_hash["rawPreimage"],
                        format!("\"{prefix}\\u{surrogate:04x}\""),
                        "double-escaped witness retains the original preimage"
                    );
                    assert!(
                        value.is_err(),
                        "UTF-8 representation refusal for accepted source string"
                    );
                    assert!(case["outcome"].get("ok").is_some());
                    assert_eq!(
                        case["wire"][0]["data"], row["rawData"],
                        "source transmits escaped lone surrogate"
                    );
                } else {
                    case["data"] = value.unwrap();
                    assert_matches(&case);
                }
            }
            kind => panic!("unexpected boundary {kind}"),
        }
    }
}

struct ActiveGuard<'a>(&'a Cell<usize>);
impl Drop for ActiveGuard<'_> {
    fn drop(&mut self) {
        self.0.set(self.0.get() - 1);
    }
}
struct PendingWire {
    active: Cell<usize>,
    calls: Cell<usize>,
    fail_first: bool,
}
impl RestAdapter for PendingWire {
    fn execute(&self, request: RestRequest) -> RestFuture<'_, RestResponse> {
        Box::pin(async move {
            self.calls.set(self.calls.get() + 1);
            if self.fail_first && self.calls.get() == 1 {
                return Err(RestFailure::from_transport(
                    TransportError::Network(NetworkFailure::from_code(
                        "retry before pending".into(),
                        Some("ECONNRESET".into()),
                    )),
                    request,
                ));
            }
            self.active.set(self.active.get() + 1);
            let _guard = ActiveGuard(&self.active);
            std::future::pending().await
        })
    }
}
struct PendingSigner {
    active: Cell<usize>,
    calls: Cell<usize>,
}
impl safe_cex_transport::signing::RemoteSigner for PendingSigner {
    fn sign(
        &self,
        _: safe_cex_transport::signing::RemoteSignerRequest,
    ) -> TransportFuture<'_, safe_cex_transport::signing::RemoteSignerResponse> {
        Box::pin(async move {
            self.calls.set(self.calls.get() + 1);
            self.active.set(self.active.get() + 1);
            let _guard = ActiveGuard(&self.active);
            std::future::pending().await
        })
    }
}
#[test]
#[expect(clippy::arc_with_non_send_sync)]
fn factory_drop_releases_signer_transport_and_retried_attempt() {
    for spot in [false, true] {
        for phase in ["signer", "transport", "retry"] {
            let clock = Rc::new(Clock(Cell::new(100000), RefCell::new(Vec::new())));
            let wire = Arc::new(PendingWire {
                active: Cell::new(0),
                calls: Cell::new(0),
                fail_first: phase == "retry",
            });
            let signer = Arc::new(PendingSigner {
                active: Cell::new(0),
                calls: Cell::new(0),
            });
            let options = serde_json::from_value(
                json!({"key":"fixture-key","secret":"fixture-secret","passphrase":"fixture-pass"}),
            )
            .unwrap();
            let http = HttpClientOptions {
                response_transform: None,
                params_serializer: None,
                exchange: ExchangeName::Kucoin,
                options,
                request: RestRequest::default(),
                provider: None,
                signer: (phase == "signer")
                    .then(|| signer.clone() as Arc<dyn safe_cex_transport::signing::RemoteSigner>),
                account_name: None,
                adapter: wire.clone(),
                host: Rc::new(Host),
                runtime: Rc::new(Runtime(clock.clone())),
                store: None,
                stats: Some(Arc::new(Mutex::new(Default::default()))),
                logs: Some(Arc::new(Mutex::new(Default::default()))),
                every: vec![],
                periodic: vec![],
                next_id: Arc::new(|| "fixture".into()),
                signing_now: Arc::new(|| 1700000000000),
            };
            let registry = safe_cex_transport::TransportRegistry::default();
            let virtual_clock = safe_cex_store::virtual_clock::VirtualClock::default();
            let api: Box<dyn RestAdapter> = if spot {
                Box::new(
                    ready(create_kucoin_spot_api(
                        http,
                        &registry,
                        virtual_clock,
                        clock,
                    ))
                    .unwrap(),
                )
            } else {
                Box::new(ready(create_api(http, &registry, virtual_clock, clock)).unwrap())
            };
            for attempt in 0..2 {
                let request = RestRequest {
                    method: "get".into(),
                    url: "/api/v1/accounts".into(),
                    ..Default::default()
                };
                let mut future = api.execute(request);
                assert!(
                    future
                        .as_mut()
                        .poll(&mut Context::from_waker(Waker::noop()))
                        .is_pending(),
                    "{spot} {phase}"
                );
                if phase == "signer" {
                    assert_eq!(signer.active.get(), 1);
                    assert_eq!(wire.calls.get(), 0, "no transport before signer settles");
                    assert_eq!(signer.calls.get(), attempt + 1);
                } else {
                    assert_eq!(wire.active.get(), 1);
                    assert_eq!(
                        wire.calls.get(),
                        attempt + 1 + usize::from(phase == "retry")
                    );
                }
                drop(future);
                assert_eq!(
                    signer.active.get(),
                    0,
                    "cancelled signing resource released"
                );
                assert_eq!(
                    wire.active.get(),
                    0,
                    "cancelled transport resource released"
                );
            }
        }
    }
}

#[test]
#[expect(clippy::arc_with_non_send_sync)]
fn native_query_profile_is_inherited_from_production_factory() {
    use safe_cex_transport::query::{QueryArrayFormat, QueryProfile};
    use safe_cex_types::QueryValue as Q;
    struct BoundaryHost(RefCell<Vec<RestRequest>>);
    impl HttpHost for BoundaryHost {
        fn execute(&self, adapter: Arc<dyn RestAdapter>, request: RestRequest) -> HttpFuture<'_> {
            self.0.borrow_mut().push(request.clone());
            Box::pin(async move { adapter.execute_with_receipt(request).await })
        }
        fn enhance_error(&self, error: RestFailure, _: &ExchangeName) -> RestFailure {
            error
        }
        fn emit(&self, _: &str, _: &str) -> Result<(), String> {
            Ok(())
        }
        fn note_success(&self, _: &ExchangeName, _: &RestResponse) {}
        fn note_failure(&self, _: &ExchangeName, _: &RestFailure) {}
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
    struct NativeWire(RefCell<Vec<RestRequest>>);
    impl RestAdapter for NativeWire {
        fn execute(&self, request: RestRequest) -> RestFuture<'_, RestResponse> {
            self.0.borrow_mut().push(request);
            Box::pin(async {
                Ok(RestResponse {
                    status: 200,
                    reason: None,
                    headers: Default::default(),
                    body: safe_cex_transport::RestResponseBody::Untransformed(
                        json!({"code":"200000"}),
                    ),
                })
            })
        }
    }
    let clock = Rc::new(Clock(Cell::new(100000), RefCell::new(Vec::new())));
    let wire = Arc::new(NativeWire(RefCell::new(vec![])));
    let host = Rc::new(BoundaryHost(RefCell::new(vec![])));
    let http=HttpClientOptions {
        response_transform:None,params_serializer:None,exchange:ExchangeName::Kucoinspot,
        options:serde_json::from_value(json!({"key":"fixture-key","secret":"fixture-secret","passphrase":"fixture-pass","publicDataOnly":true})).unwrap(),
        request:RestRequest::default(),provider:None,signer:None,account_name:None,adapter:wire.clone(),host:host.clone(),runtime:Rc::new(Runtime(clock.clone())),store:None,
        stats:Some(Arc::new(Mutex::new(Default::default()))),logs:Some(Arc::new(Mutex::new(Default::default()))),every:vec![],periodic:vec![],next_id:Arc::new(||"native-profile".into()),signing_now:Arc::new(||0),
    };
    let registry = safe_cex_transport::TransportRegistry::default();
    let api = ready(create_kucoin_spot_api(
        http,
        &registry,
        safe_cex_store::virtual_clock::VirtualClock::default(),
        clock,
    ))
    .unwrap();
    let native = Q::Object(
        [
            ("number".into(), Q::Number(f64::NAN)),
            ("zero".into(), Q::Number(-0.0)),
            (
                "list".into(),
                Q::Array(vec![Q::Number(f64::NEG_INFINITY), Q::String("-0".into())]),
            ),
        ]
        .into_iter()
        .collect(),
    );
    ready(api.execute(RestRequest {
        method: "GET".into(),
        url: "/api/v2/symbols".into(),
        query_params: Some(native),
        ..Default::default()
    }))
    .unwrap();
    let input = host.0.borrow();
    assert_eq!(input.len(), 1, "QUERY production factory native call count");
    assert_eq!(
        input[0].query_profile,
        Some(QueryProfile {
            key_order: safe_cex_transport::query::QueryKeyOrder::Preserve,
            arrays: QueryArrayFormat::Indexed,
            encode: true
        }),
        "QUERY production factory profile inherited"
    );
    let Some(Q::Object(params)) = &input[0].query_params else {
        panic!("QUERY production native object")
    };
    assert!(
        matches!(params.get("number"),Some(Q::Number(n)) if n.is_nan()),
        "QUERY production native NaN before rendering"
    );
    assert!(
        matches!(params.get("zero"),Some(Q::Number(n)) if n.to_bits()==(-0.0_f64).to_bits()),
        "QUERY production native signed zero before rendering"
    );
    let actual = wire.0.borrow();
    assert_eq!(actual.len(), 1, "QUERY production wire call count");
    assert!(
        actual[0].query_params.is_none(),
        "QUERY production params consumed once"
    );
    assert_eq!(
        actual[0].url, "/api/v2/symbols?number=NaN&zero=0&list%5B0%5D=-Infinity&list%5B1%5D=-0",
        "QUERY production exact indexed URL"
    );
}
