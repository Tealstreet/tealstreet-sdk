#[path = "support/cardinality.rs"]
mod cardinality;
use safe_cex_exchange::{
    client_order_id::ClientOrderLookup,
    http::{
        HttpClientOptions, HttpFailure, HttpFuture, HttpHost, HttpLogState,
        HttpRequestStatsTracker, safe_cex_axios_create_no_proxy,
    },
    runtime::{ExchangeError, ExchangeFuture, ExchangeRuntime, ExchangeTask},
};
use safe_cex_num::JsJsonValue;
use safe_cex_transport::{RestAdapter, RestFailure, RestFuture, RestRequest, RestResponse};
use safe_cex_types::{ExchangeName, Market, Order};
use safe_cex_venue_okx::rest_operations::{
    find_order_by_client_order_id, place_algo_order_batch, place_order_batch,
};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    future::Future,
    pin::Pin,
    rc::Rc,
    sync::{Arc, Mutex},
    task::{Context, Poll, Waker},
};
struct Runtime;
impl ExchangeRuntime for Runtime {
    fn monotonic_now_ms(&self) -> f64 {
        123.
    }
    fn now_ms(&self) -> f64 {
        123.
    }
    fn sleep(&self, _: u64) -> ExchangeFuture<'static, ()> {
        Box::pin(async { Err(ExchangeError::new("unexpected sleep")) })
    }
    fn spawn_local(&self, _: Pin<Box<dyn Future<Output = ()>>>) -> Box<dyn ExchangeTask> {
        panic!("REST operation must not detach work")
    }
    fn console_values(&self, _: &str, _: &[safe_cex_exchange::js_runtime::value::Js]) {}
    fn unique_id(&self) -> String {
        "fixture".into()
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
        _: &safe_cex_types::retained_json::RetainedJsonInput,
    ) {
        panic!("unexpected raw exception report in this test runtime");
    }
    fn report_exception(
        &self,
        _: &ExchangeError,
        _: &safe_cex_types::retained_json::RetainedJsonInput,
    ) {
    }
    fn execute_worker_request(&self, _: Value) -> ExchangeFuture<'static, Value> {
        Box::pin(async { Err(ExchangeError::new("unexpected worker request")) })
    }
}
struct Host;
impl HttpHost for Host {
    fn execute(&self, adapter: Arc<dyn RestAdapter>, request: RestRequest) -> HttpFuture<'_> {
        Box::pin(async move { adapter.execute_with_receipt(request).await })
    }
    fn enhance_error(&self, error: HttpFailure, _: &ExchangeName) -> HttpFailure {
        if std::env::var_os("OKX_MUTATE_LOSSY_FAILURE").is_some() {
            HttpFailure::pre_transmit(error.message, *error.request)
        } else {
            error
        }
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
struct Wire {
    responses: Vec<Value>,
    next: RefCell<usize>,
    calls: RefCell<Vec<Value>>,
}
impl RestAdapter for Wire {
    fn execute(&self, request: RestRequest) -> RestFuture<'_, RestResponse> {
        Box::pin(async move {
            self.calls.borrow_mut().push(if request.method=="post"{json!({"method":"post","url":request.url,"body":String::from_utf8(request.body.as_ref().map(|body| body.wire_bytes().unwrap()).unwrap_or_default()).unwrap()})}else{json!({"method":"get","url":request.url,"params":request.params})});
            let i = *self.next.borrow();
            *self.next.borrow_mut() += 1;
            let item = self.responses.get(i).expect("unexpected request");
            let response = RestResponse {
                status: item["status"].as_u64().unwrap_or(200) as u16,
                reason: Some("venue reason".into()),
                headers: [("x-venue".into(), "real-response".into())].into(),
                body: safe_cex_transport::RestResponseBody::Encoded(
                    item["bodyRaw"]
                        .as_str()
                        .map(|raw| raw.as_bytes().to_vec())
                        .unwrap_or_else(|| serde_json::to_vec(&item["body"]).unwrap()),
                ),
            };
            if item["reject"] == true {
                Err(RestFailure {
                    code: None,
                    message: item["message"]
                        .as_str()
                        .unwrap_or("DISPLAY TEXT IS NOT THE RESPONSE")
                        .into(),
                    response: Some(Box::new(response)),
                    request: Box::new(request),
                    transport_error: None,
                    context: Some(Box::new(json!({"provenance":"wire"}))),
                })
            } else {
                Ok(response)
            }
        })
    }
}
fn ready<T>(future: impl Future<Output = T>) -> T {
    let mut future = std::pin::pin!(future);
    match future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(v) => v,
        Poll::Pending => panic!("fixture must finish inline"),
    }
}
#[test]
#[ignore = "requires isolated TypeScript compiler; mandatory venue CI gate"]
#[expect(clippy::arc_with_non_send_sync)]
fn maintained_typescript_response_body_lookup_and_sequential_placement() {
    let oracle = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/rest-operations-oracle.mjs"
        ))
        .output()
        .expect("Node required");
    assert!(
        oracle.status.success(),
        "{}",
        String::from_utf8_lossy(&oracle.stderr)
    );
    for (mutation, expected) in [
        ("--mutate-lookup-response", "AssertionError"),
        ("--mutate-placement-response", "PER ORDER"),
        ("--mutate-missing-package", "MODULE_NOT_FOUND"),
        (
            "--mutate-eager-id-refusal",
            "deferred refusal source later error",
        ),
    ] {
        let control = std::process::Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/rest-operations-oracle.mjs"
            ))
            .arg(mutation)
            .output()
            .unwrap();
        assert_eq!(
            control.status.code(),
            Some(1),
            "source control must assert-fail: {mutation}"
        );
        assert!(
            String::from_utf8_lossy(&control.stderr).contains(expected),
            "source control failed for wrong reason: {}",
            String::from_utf8_lossy(&control.stderr)
        );
    }
    let cases: Vec<Value> = serde_json::from_slice(&oracle.stdout).unwrap();
    assert_eq!(cases.len(), 118);
    cardinality::assert_groups(
        &cases,
        "rest_operations profiles",
        |case| case["input"]["op"].as_str().unwrap().to_owned(),
        &[("lookup", 32), ("batch", 43), ("algo", 43)],
    );
    for (index, case) in cases.iter().enumerate() {
        let input = &case["input"];
        let wire = Arc::new(Wire {
            responses: input["responses"].as_array().unwrap().clone(),
            next: RefCell::new(0),
            calls: RefCell::new(vec![]),
        });
        let client = safe_cex_axios_create_no_proxy(HttpClientOptions {
            response_transform: None,
            params_serializer: None,
            exchange: ExchangeName::Okx,
            options: serde_json::from_value(json!({"key":"", "secret":"", "publicDataOnly":true}))
                .unwrap(),
            request: RestRequest::default(),
            provider: None,
            signer: None,
            account_name: None,
            adapter: wire.clone(),
            host: Rc::new(Host),
            runtime: Rc::new(Runtime),
            store: None,
            stats: Some(Arc::new(Mutex::new(HttpRequestStatsTracker::default()))),
            logs: Some(Arc::new(Mutex::new(HttpLogState::default()))),
            every: vec![],
            periodic: vec![],
            next_id: Arc::new(|| "fixture".into()),
            signing_now: Arc::new(|| 123),
        });
        let emissions = RefCell::new(Vec::<String>::new());
        let mappings = RefCell::new(Vec::<Value>::new());
        if input["op"] == "lookup" {
            let orders: Vec<Order> =
                serde_json::from_value(input.get("orders").cloned().unwrap_or(json!([]))).unwrap();
            let market: Option<Market> =
                serde_json::from_value(input.get("market").cloned().unwrap_or(Value::Null))
                    .unwrap();
            let mapper = |row: &Value, state: safe_cex_num::TableHit<&'static str>| {
                let mut wire_row = row.clone();
                match state {
                    safe_cex_num::TableHit::Inherited(member) => {
                        assert_eq!(
                            case["sourcePrototype"]["key"],
                            input["responses"][0]["body"]["data"][0]["state"],
                            "native source member known before wire projection"
                        );
                        if member == safe_cex_num::ProtoMember::Prototype {
                            wire_row["state"] = json!({});
                        } else {
                            wire_row.as_object_mut().unwrap().remove("state");
                        }
                    }
                    safe_cex_num::TableHit::Mapped(value) => assert_eq!(row["state"], value),
                    safe_cex_num::TableHit::Missing => {
                        panic!("nullish state must already take source fallback")
                    }
                }
                mappings.borrow_mut().push(json!([wire_row]));
                if input["mapEmpty"] == true {
                    return Ok(vec![]);
                };
                let mut order:Order=serde_json::from_value(json!({"id":"mapped-order","status":"open","symbol":"BTCUSDT","type":"limit","side":"buy","price":1,"amount":1,"filled":0,"remaining":1,"reduceOnly":false,"lastUpdate":123})).unwrap();
                order.info = (Some(row.clone().into())).into();
                Ok(vec![order])
            };
            match ready(find_order_by_client_order_id(
                &client,
                &orders,
                market.as_ref(),
                "BTCUSDT",
                "client",
                &mapper,
            )) {
                Ok(result) => {
                    let actual = match result {
                        ClientOrderLookup::Found(order) => json!({"found":true,"order":order}),
                        ClientOrderLookup::NotFound { definitive } => {
                            json!({"found":false,"definitive":definitive})
                        }
                    };
                    assert_json(&actual, &case["result"], index);
                }
                Err(error) => {
                    if case.get("sourcePrototype").is_some() {
                        assert_eq!(error.code, None);
                        assert_eq!(
                            error.context.as_deref().unwrap()["name"],
                            "RepresentationError"
                        );
                        assert_eq!(
                            error.message,
                            "OKX inherited lookup status cannot cross the closed typed output boundary"
                        );
                        assert!(error.response.is_none());
                    } else {
                        assert_eq!(
                            error.message, case["error"]["message"],
                            "lookup error {index}"
                        );
                        if let Some(expected) = case["error"].get("response") {
                            let response = error
                                .response
                                .as_ref()
                                .expect("response must survive HttpClient RestAdapter");
                            assert_eq!(response.reason.as_deref(), Some("venue reason"));
                            assert_eq!(response.headers["x-venue"], "real-response");
                            assert_json(&response.body.json_value().unwrap(), expected, index);
                        } else {
                            assert!(
                                error.response.is_none(),
                                "coercion throws before source attaches response: {index}"
                            );
                        }
                    }
                }
            }
        } else {
            let payloads: Vec<JsJsonValue> = input["payloads"]
                .as_array()
                .unwrap()
                .iter()
                .cloned()
                .map(JsJsonValue::from)
                .collect();
            let emit = |message: &str| {
                emissions.borrow_mut().push(message.to_owned());
                Ok(())
            };
            let result = if input["op"] == "batch" {
                ready(place_order_batch(&client, &payloads, &emit))
            } else {
                ready(place_algo_order_batch(&client, &payloads, &emit))
            };
            if input["refusalTiming"] == true {
                assert_eq!(
                    json!(*wire.calls.borrow()),
                    case["calls"],
                    "deferred refusal request count/order {index}"
                );
            }
            match result {
                Ok(ids) => assert_eq!(json!(ids), case["result"], "placement {index}"),
                Err(error) => {
                    assert_eq!(error.name, case["error"]["name"], "placement name {index}");
                    assert_eq!(
                        error.message, case["error"]["message"],
                        "placement response body {index}"
                    );
                }
            }
        }
        assert_eq!(
            json!(*wire.calls.borrow()),
            case["calls"],
            "request bytes/order/count {index}"
        );
        let source_emissions = case["emissions"].as_array().unwrap();
        if source_emissions.iter().any(|emitted| !emitted.is_string()) {
            // The source emits a non-string msg before new Error(msg) throws; the
            // string-valued emitter cannot carry it, so the native path only throws.
            assert!(
                emissions.borrow().is_empty(),
                "string emitter refuses a non-string diagnostic {index}"
            );
            assert_eq!(
                case["error"]["name"], "TypeError",
                "source then throws {index}"
            );
        } else {
            assert_eq!(
                json!(emissions.into_inner()),
                case["emissions"],
                "diagnostic {index}"
            );
        }
        assert_eq!(
            json!(mappings.into_inner()),
            case["mappings"],
            "mapper arguments {index}"
        );
    }
}
fn assert_json(actual: &Value, expected: &Value, index: usize) {
    fn canonical(value: &Value) -> Value {
        match value {
            Value::Number(n) => json!(
                n.as_f64()
                    .unwrap_or_else(|| safe_cex_num::number_from_string(&n.to_string()))
            ),
            Value::Array(a) => Value::Array(a.iter().map(canonical).collect()),
            Value::Object(o) => {
                Value::Object(o.iter().map(|(k, v)| (k.clone(), canonical(v))).collect())
            }
            v => v.clone(),
        }
    }
    assert_eq!(
        canonical(actual),
        canonical(expected),
        "response value {index}"
    );
}

#[test]
#[ignore = "requires isolated TypeScript compiler; mandatory venue CI gate"]
fn response_identity_control_rejects_lossy_host_boundary() {
    let child = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "maintained_typescript_response_body_lookup_and_sequential_placement",
            "--include-ignored",
            "--nocapture",
        ])
        .env("OKX_MUTATE_LOSSY_FAILURE", "1")
        .output()
        .unwrap();
    assert_eq!(child.status.code(), Some(101));
    let output = String::from_utf8_lossy(&child.stdout).to_string()
        + &String::from_utf8_lossy(&child.stderr);
    assert!(
        output.contains("lookup error 2"),
        "must fail on definitive response lookup, not unrelated error: {output}"
    );
}
