//! Real MEXC readers -> native emitter -> installed worker bridge, all120 source cases.
#![cfg(not(target_arch = "wasm32"))]
use safe_cex_exchange::native_store::DefaultStore;
use safe_cex_exchange::{
    error_reporting::ExchangeErrorInput,
    exchange::{BaseExchange, Exchange, ExchangeSettings},
    runtime::{ExchangeError, ExchangeFuture, ExchangeRuntime, ExchangeTask},
    websocket::{WebSocketConnectionOptions, WsRuntime, WsSocket},
};
use safe_cex_store::{Store, StoreUpdate};
use safe_cex_transport::{
    RestAdapter, RestFailure, RestFuture, RestRequest, RestResponse, RestResponseBody,
    TransportProvider, socket::TunnelHost,
};
use safe_cex_types::ExchangeName;
use safe_cex_venue_mexc::{
    exchange::MexcExchange,
    sockets::MexcSocketDependencies,
    utils::{fake_positions_for_market, normalize_market},
};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    future::Future,
    pin::Pin,
    rc::Rc,
    sync::Arc,
};
#[path = "../../safe-cex-venue-mexc/tests/support/history_store.rs"]
#[allow(
    dead_code,
    reason = "shared observer fixture delegates the complete Store contract"
)]
mod history_store;
struct AccountFailure {
    message: String,
    response_data: Option<Value>,
}
impl RestAdapter for AccountFailure {
    fn execute(&self, request: RestRequest) -> RestFuture<'_, RestResponse> {
        let mut failure = RestFailure::pre_transmit(self.message.clone(), request);
        failure.response = self.response_data.as_ref().map(|data| {
            Box::new(RestResponse {
                status: 401,
                reason: Some("Unauthorized".into()),
                headers: Default::default(),
                body: RestResponseBody::Untransformed(
                    safe_cex_types::retained_json::RetainedJsonInput::from_value(data.clone())
                        .unwrap(),
                ),
            })
        });
        Box::pin(async move { Err(failure) })
    }
}
struct ReportingRuntime {
    worker: bool,
    consoles: RefCell<Vec<String>>,
    reports: RefCell<Vec<Value>>,
    raw_reports: RefCell<Vec<safe_cex_exchange::js_runtime::value::Js>>,
    trace: Rc<RefCell<Vec<String>>>,
}
impl ExchangeRuntime for ReportingRuntime {
    fn monotonic_now_ms(&self) -> f64 {
        100.
    }
    fn now_ms(&self) -> f64 {
        100.
    }
    fn sleep(&self, _: u64) -> ExchangeFuture<'static, ()> {
        panic!("unexpected sleep")
    }
    fn spawn_local(&self, _: Pin<Box<dyn Future<Output = ()>>>) -> Box<dyn ExchangeTask> {
        panic!("unexpected spawn")
    }
    fn console_values(&self, severity: &str, _: &[safe_cex_exchange::js_runtime::value::Js]) {
        self.trace.borrow_mut().push(severity.into());
        self.consoles.borrow_mut().push(severity.into());
    }
    fn unique_id(&self) -> String {
        "reporting-id".into()
    }
    fn is_worker(&self) -> bool {
        self.worker
    }
    fn random(&self) -> f64 {
        0.5
    }
    fn report_raw_exception(
        &self,
        error: &safe_cex_exchange::js_runtime::value::Js,
        data: &safe_cex_types::retained_json::RetainedJsonInput,
    ) {
        // Retain the host's original value before any finite fixture observation.
        self.raw_reports.borrow_mut().push(error.clone());
        let error = native_report_shape(error);
        self.trace.borrow_mut().push("report".into());
        self.reports
            .borrow_mut()
            .push(json!({"error":error,"data":data}));
    }
    fn report_exception(
        &self,
        error: &ExchangeError,
        data: &safe_cex_types::retained_json::RetainedJsonInput,
    ) {
        self.trace.borrow_mut().push("typed-report".into());
        self.reports
            .borrow_mut()
            .push(json!({"error":{"name":error.name,"message":error.message},"data":data}));
    }
    fn execute_worker_request(&self, _: Value) -> ExchangeFuture<'static, Value> {
        panic!("unexpected worker request")
    }
}
struct ReportingFailure {
    failure: AccountFailure,
    gets: Cell<usize>,
    trace: Option<Rc<RefCell<Vec<String>>>>,
}

fn assert_source_tree(source: &Value, native: &Value, path: &str) {
    match (source, native) {
        (Value::Number(a), Value::Number(b)) => assert_eq!(
            a.as_f64().unwrap().to_bits(),
            b.as_f64().unwrap().to_bits(),
            "Source/native decoded Number bits {path}"
        ),
        (Value::Array(a), Value::Array(b)) => {
            assert_eq!(a.len(), b.len(), "Source/native array fields {path}");
            for (i, (a, b)) in a.iter().zip(b).enumerate() {
                assert_source_tree(a, b, &format!("{path}/{i}"));
            }
        }
        (Value::Object(a), Value::Object(b)) => {
            let keys = |o: &serde_json::Map<String, Value>| {
                o.keys().cloned().collect::<std::collections::BTreeSet<_>>()
            };
            assert_eq!(keys(a), keys(b), "Source/native object fields {path}");
            for (key, a) in a {
                assert_source_tree(a, &b[key], &format!("{path}/{key}"));
            }
        }
        _ => assert_eq!(source, native, "Source/native decoded value {path}"),
    }
}
impl RestAdapter for ReportingFailure {
    fn execute(&self, request: RestRequest) -> RestFuture<'_, RestResponse> {
        if let Some(trace) = &self.trace {
            trace.borrow_mut().push("get".into());
        }
        assert_eq!(request.method, "get", "native reporting GET verb");
        assert!(
            matches!(
                request.url.as_str(),
                "/api/v1/private/position/open_positions" | "/api/v1/private/account/assets"
            ),
            "native reporting request route"
        );
        self.gets.set(self.gets.get() + 1);
        self.failure.execute(request)
    }
}

fn drive<T>(future: ExchangeFuture<'_, T>, failure: &ReportingFailure) -> Result<T, ExchangeError> {
    assert_eq!(
        failure.gets.get(),
        0,
        "native reporting GET remains lazy before poll"
    );
    futures::executor::block_on(future)
}
fn discard<T>(future: ExchangeFuture<'_, T>, failure: &ReportingFailure) {
    assert_eq!(
        failure.gets.get(),
        0,
        "native reporting dropped future prefix GET floor"
    );
    drop(future);
    assert_eq!(
        failure.gets.get(),
        0,
        "native reporting drop loses unpolled GET"
    );
}

impl WsRuntime for ReportingRuntime {
    fn transport(&self) -> Arc<dyn TransportProvider> {
        Arc::new(safe_cex_transport::DirectTransport)
    }
    fn tunnel_host(&self) -> Rc<dyn TunnelHost> {
        panic!("unexpected tunnel")
    }
    fn dial(
        &self,
        _: String,
        _: Vec<String>,
        _: WebSocketConnectionOptions,
        _: bool,
    ) -> ExchangeFuture<'static, Rc<dyn WsSocket>> {
        panic!("unexpected socket")
    }
    fn headers_capable(&self) -> bool {
        true
    }
    fn enable_reconnecting(&self) -> bool {
        false
    }
    fn min_reconnection_delay_ms(&self) -> f64 {
        1000.
    }
    fn library_min_reconnection_delay_ms(&self) -> f64 {
        3000.
    }
    fn diagnostics_enabled(&self) -> bool {
        false
    }
    fn lifecycle_logs_enabled(&self) -> bool {
        false
    }
}

fn venue(
    store: Rc<dyn Store>,
    runtime: Rc<ReportingRuntime>,
    http: Rc<dyn RestAdapter>,
) -> Rc<MexcExchange> {
    let base = BaseExchange::new(
        ExchangeName::Mexc,
        serde_json::from_value(
            json!({"key":"key","secret":"secret","testnet":false,"publicDataOnly":false}),
        )
        .unwrap(),
        store,
        runtime.clone(),
        ExchangeSettings::default(),
    );
    MexcExchange::new(
        base,
        http,
        Rc::new(|v| v),
        MexcSocketDependencies {
            runtime,
            current_time_ms: Rc::new(|| 100.),
            remote_account_name: None,
            emit_fill: Rc::new(|_| Ok(())),
        },
        Rc::new(|_, _| panic!("unexpected external data")),
    )
}
#[test]
fn mexc_raw_reporting_preserves_values_and_unwinds_before_fallback() {
    let mut dropped = 0;
    for method in ["positions", "validate", "account"] {
        let runtime = Rc::new(ReportingRuntime {
            worker: false,
            consoles: Default::default(),
            reports: Default::default(),
            raw_reports: Default::default(),
            trace: Default::default(),
        });
        let failure = Rc::new(ReportingFailure {
            failure: AccountFailure {
                message: "unpolled".into(),
                response_data: None,
            },
            gets: Cell::new(0),
            trace: None,
        });
        let exchange = venue(Rc::new(DefaultStore::new()), runtime, failure.clone());
        match method {
            "positions" => discard(exchange.fetch_positions(None), &failure),
            "validate" => discard(exchange.validate_account(), &failure),
            "account" => discard(exchange.get_account(), &failure),
            _ => unreachable!(),
        }
        dropped += 1;
    }
    assert_eq!(dropped, 3, "native independent unpolled-drop method floor");
    let fixtures: Vec<Value> = serde_json::from_str(include_str!(
        "../../safe-cex-venue-mexc/tests/fixtures/reporting_values.json"
    ))
    .unwrap();
    assert_eq!(
        fixtures.len(),
        20,
        "native independent reporting fixture floor"
    );
    let mut observations = vec![];
    let (mut gets, mut consoles, mut event_calls, mut report_calls, mut fallback_reads) =
        (0, 0, 0, 0, 0);
    for worker in [false, true] {
        for fixture in &fixtures {
            let label = fixture["label"].as_str().unwrap();
            for method in ["positions", "validate", "account"] {
                let store = Rc::new(history_store::ObservedStore::new());
                let market =
                    normalize_market(&json!({"symbol":"BTC_USDT","contractSize":0.1}).into())
                        .unwrap();
                let mut position = fake_positions_for_market(&market, None, None, None).remove(0);
                position.id = "fallback".into();
                Store::update(
                    &store.inner,
                    StoreUpdate {
                        positions: Some(vec![position]),
                        ..Default::default()
                    },
                    false,
                )
                .unwrap();
                let runtime = Rc::new(ReportingRuntime {
                    worker,
                    consoles: RefCell::default(),
                    reports: RefCell::default(),
                    raw_reports: RefCell::default(),
                    trace: Rc::default(),
                });
                let failure = Rc::new(ReportingFailure {
                    gets: Cell::new(0),
                    trace: None,
                    failure: AccountFailure {
                        message: fixture["transport"].as_str().unwrap().into(),
                        response_data: (!fixture["data"].is_null())
                            .then(|| fixture["data"].clone()),
                    },
                });
                let exchange = venue(store.clone(), runtime.clone(), failure.clone());
                let events = Rc::new(RefCell::new(Vec::<Value>::new()));
                let observed = events.clone();
                // Keep the actual worker value: Error.message is a non-enumerable getter
                // observation, whereas JSON.stringify must only see enumerable fields.
                let worker_reports = Rc::new(RefCell::new(Vec::new()));
                let captured_reports = worker_reports.clone();
                safe_cex_webworker::exchange_worker_start::install_exchange_event_bridge(
                    exchange.base(),
                    safe_cex_webworker::data_post::WorkerDataPost::new(Rc::new(move |payload| {
                        assert_eq!(
                            payload.op,
                            safe_cex_webworker::payload::StoreWorkerOp::Error
                        );
                        let value = payload
                            .data
                            .array_values()
                            .expect("fixture array property reads")
                            .expect("worker posts emitted args once");
                        if worker {
                            captured_reports
                                .borrow_mut()
                                .push(value[1].get("sentryData")?.get("error")?);
                        }
                        let value: Vec<serde_json::Value> = value
                            .iter()
                            .map(|value| {
                                safe_cex_exchange::js_runtime::value::to_json(value).unwrap()
                            })
                            .collect();
                        observed.borrow_mut().push(serde_json::json!(value));
                        Ok(())
                    })),
                );
                exchange.base().note_http_response(
                    &json!({"status":503,"statusText":"Unavailable"}),
                    Some("/prior"),
                );
                let result: Result<Value, ExchangeError> = match method {
                    "positions" => {
                        drive(exchange.fetch_positions(None), &failure).map(|positions| {
                            assert_eq!(
                                positions.len(),
                                1,
                                "native reporting Store fallback row floor {label}"
                            );
                            assert_eq!(
                                positions[0].id, "fallback",
                                "native decoded fallback ID {label}"
                            );
                            json!({"ids":["fallback"]})
                        })
                    }
                    "validate" => drive(exchange.validate_account(), &failure).map(|validation| {
                        assert!(
                            validation.balance.is_none(),
                            "native rejected validation has no balance"
                        );
                        assert_eq!(
                            validation.error.as_value(),
                            &fixture["validation"],
                            "native raw validation value {label}"
                        );
                        validation.error.into_value()
                    }),
                    "account" => drive(exchange.get_account(), &failure)
                        .map(|_| panic!("failed account unexpectedly succeeds")),
                    _ => unreachable!(),
                };
                assert_eq!(
                    failure.gets.get(),
                    1,
                    "native independent reporting GET floor {label}/{method}"
                );
                assert_eq!(
                    *runtime.consoles.borrow(),
                    ["error", "trace"],
                    "native console-before-format order {label}/{method}"
                );
                let coercion = label == "coercion";
                assert_eq!(
                    events.borrow().len(),
                    if coercion { 0 } else { 1 },
                    "native raw emitter call floor {label}/{method}"
                );
                assert_eq!(
                    store.position_reads.get(),
                    usize::from(method == "positions" && !coercion),
                    "native report-before-fallback access {label}/{method}"
                );
                let (value, error) = match result {
                    Ok(value) => {
                        assert!(
                            !coercion && method != "account",
                            "native raw coercion must reject {label}/{method}"
                        );
                        (value, Value::Null)
                    }
                    Err(error) => {
                        assert!(
                            coercion || method == "account",
                            "native unexpected reporting rejection {label}/{method}: {error}"
                        );
                        assert_eq!(
                            error.name, fixture["getError"]["name"],
                            "native reporting rejection name {label}/{method}"
                        );
                        assert_eq!(
                            error.message, fixture["getError"]["message"],
                            "native reporting rejection message {label}/{method}"
                        );
                        (
                            Value::Null,
                            json!({"name":error.name,"message":error.message}),
                        )
                    }
                };
                let selected = &fixture[if method == "positions" {
                    "position"
                } else {
                    "validation"
                }];
                let format = &fixture[if method == "positions" {
                    "positionFormat"
                } else {
                    "validationFormat"
                }];
                let mut reported = Value::Null;
                let mut report = Value::Null;
                if !coercion {
                    assert_source_tree(
                        format,
                        &events.borrow()[0][0],
                        &format!("native raw formatted value {label}/{method}"),
                    );
                    reported = if worker {
                        let raw = &worker_reports.borrow()[0];
                        if selected.is_string() {
                            assert!(
                                matches!(raw, safe_cex_exchange::js_runtime::value::Js::Object(object) if object.class == "Error"),
                                "worker posts a real Error before projection"
                            );
                        }
                        native_report_shape(raw)
                    } else {
                        assert_eq!(
                            runtime.reports.borrow().len(),
                            1,
                            "native raw host report child floor {label}/{method}"
                        );
                        runtime.reports.borrow()[0]["error"].clone()
                    };
                    if selected.is_string() {
                        assert_eq!(
                            reported["name"], "ExchangeError",
                            "native constructed string error name"
                        );
                        assert_eq!(
                            &reported["message"], format,
                            "native constructed string error message"
                        );
                    } else {
                        assert_source_tree(
                            selected,
                            &reported,
                            &format!("native raw host/worker value and type {label}/{method}"),
                        );
                    }
                    report = if worker {
                        events.borrow()[0][1]["sentryData"].clone()
                    } else {
                        runtime.reports.borrow()[0]["data"].clone()
                    };
                    report.as_object_mut().unwrap().shift_remove("error");
                    assert_eq!(
                        report["tags"]["exchange"], "mexc",
                        "native raw report exchange tag"
                    );
                    assert_eq!(
                        report["extra"]["exchangeOptions"],
                        json!({"testnet":false,"publicDataOnly":false}),
                        "native raw report options"
                    );
                    if let Some(expected) = fixture.get("configExpected") {
                        if expected.is_null() {
                            assert!(
                                report["extra"].get("config").is_none(),
                                "native raw falsy config omission {label}"
                            );
                        } else {
                            assert_eq!(
                                &report["extra"]["config"], expected,
                                "native raw header projection {label}"
                            );
                        }
                    }
                }
                let reported_json = if worker && !coercion {
                    json!(native_stringify(&worker_reports.borrow()[0]))
                } else if !coercion && !selected.is_string() {
                    let raw = runtime.raw_reports.borrow();
                    assert_eq!(
                        raw.len(),
                        1,
                        "main raw reporter retains its native argument"
                    );
                    assert_source_tree(
                        selected,
                        &native_report_shape(&raw[0]),
                        "main raw report before JSON stringify",
                    );
                    json!(native_stringify(&raw[0]))
                } else {
                    Value::Null
                };
                let after = if coercion {
                    assert!(
                        runtime.reports.borrow().is_empty(),
                        "native coercion rejects before reporting"
                    );
                    exchange
                        .base()
                        .emit_and_log_error(ExchangeErrorInput::Message("after".into()), None, None)
                        .unwrap();
                    let tags = if worker {
                        events.borrow()[0][1]["sentryData"]["tags"].clone()
                    } else {
                        runtime.reports.borrow()[0]["data"]["tags"].clone()
                    };
                    assert_eq!(
                        tags["recentHttpStatus"], 503.,
                        "native coercion leaves correlation unconsumed"
                    );
                    json!(503)
                } else {
                    Value::Null
                };
                gets += failure.gets.get();
                consoles += runtime.consoles.borrow().len();
                event_calls += events.borrow().len();
                report_calls += runtime.reports.borrow().len();
                fallback_reads += store.position_reads.get();
                observations.push(json!({"worker":worker,"label":label,"method":method,"result":value,"error":error,
                    "event":events.borrow()[0][0],"reported":reported,"reportedJson":reported_json,"positionReads":store.position_reads.get(),"after":after,"report":report}));
            }
        }
    }
    assert_eq!(
        observations.len(),
        120,
        "native independent raw reporting method root floor"
    );
    assert_eq!(
        (gets, consoles, event_calls, report_calls, fallback_reads),
        (120, 252, 120, 60, 38),
        "native independent direct child-call floors"
    );
    // All native types, values and child-call assertions precede Source/wire output.
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../safe-cex-venue-mexc/tests/reporting_values_oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "SOURCE actual catches/reporter: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_source_tree(&source, &Value::Array(observations), "reporting");
}

// Exercise the same operation installed as JSON.stringify, never serde's formatter.
fn native_stringify(value: &safe_cex_exchange::js_runtime::value::Js) -> String {
    let units = safe_cex_exchange::js_runtime::globals::stringify_json(value)
        .expect("native reporting JSON.stringify")
        .expect("report value is JSON serializable");
    String::from_utf16(&units).expect("report JSON contains valid UTF-16")
}

// Match the source observer's Error property reads without redefining their
// enumerability. JSON.stringify is checked separately on the same native value.
fn native_report_shape(raw: &safe_cex_exchange::js_runtime::value::Js) -> Value {
    use safe_cex_exchange::js_runtime::value::{Js, to_json};
    if matches!(raw, Js::Object(object) if object.class == "Error") {
        json!({"name": to_json(&raw.get("name").unwrap()).unwrap(), "message": to_json(&raw.get("message").unwrap()).unwrap()})
    } else {
        to_json(raw).expect("finite raw reporting fixture")
    }
}
