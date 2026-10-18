use super::*;

pub(super) fn raw_reporting_base() -> BaseExchange {
    let runtime = Rc::new(ReportingRuntime {
        worker: false,
        consoles: RefCell::default(),
        reports: RefCell::default(),
        trace: Rc::default(),
    });
    BaseExchange::new(
        ExchangeName::Mexc,
        serde_json::from_value(json!({"key":"key","secret":"secret","publicDataOnly":false}))
            .unwrap(),
        Rc::new(DefaultStore::new()),
        runtime,
        ExchangeSettings::default(),
    )
}

struct ReportingRuntime {
    worker: bool,
    consoles: RefCell<Vec<String>>,
    reports: RefCell<Vec<Value>>,
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
    fn report_raw_exception(&self, error: &safe_cex_exchange::js_runtime::value::Js, data: &Value) {
        let error = safe_cex_exchange::js_runtime::value::to_json(error).expect("explicit JSON fixture observation");
        self.trace.borrow_mut().push("report".into());
        self.reports
            .borrow_mut()
            .push(json!({"error":error,"data":data}));
    }
    fn report_exception(&self, error: &ExchangeError, data: &Value) {
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

#[test]
fn mexc_raw_reporting_preserves_values_and_unwinds_before_fallback() {
    let mut dropped = 0;
    for method in ["positions", "validate", "account"] {
        let mut exchange = venue(Rc::new(DefaultStore::new()), 100., false);
        let failure = Rc::new(ReportingFailure {
            failure: AccountFailure {
                message: "unpolled".into(),
                response_data: None,
            },
            gets: Cell::new(0),
            trace: None,
        });
        exchange.xhr = failure.clone();
        match method {
            "positions" => discard(exchange.fetch_positions(None), &failure),
            "validate" => discard(exchange.validate_account(), &failure),
            "account" => discard(exchange.get_account(), &failure),
            _ => unreachable!(),
        }
        dropped += 1;
    }
    assert_eq!(dropped, 3, "native independent unpolled-drop method floor");
    let fixtures: Vec<Value> =
        serde_json::from_str(include_str!("../tests/fixtures/reporting_values.json")).unwrap();
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
                let market = normalize_market(
                    json!({"symbol":"BTC_USDT","contractSize":0.1})
                        .as_object()
                        .unwrap(),
                )
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
                let mut exchange = venue(Rc::new(DefaultStore::new()), 100., false);
                exchange.base=Rc::new(BaseExchange::new(ExchangeName::Mexc,
                    serde_json::from_value(json!({"key":"key","secret":"secret","testnet":false,"publicDataOnly":false})).unwrap(),
                    store.clone(),runtime.clone(),ExchangeSettings::default()));
                exchange.xhr = failure.clone();
                let events = Rc::new(RefCell::new(Vec::<Value>::new()));
                let observed = events.clone();
                exchange.base.on_args(
                    "error",
                    Rc::new(move |event, value| {
                        let value: Vec<serde_json::Value> = value
                            .iter()
                            .map(|value| {
                                safe_cex_exchange::js_runtime::value::to_json(value).unwrap()
                            })
                            .collect();
                        assert_eq!(event, "error", "native actual reporting event");
                        observed.borrow_mut().push(serde_json::json!(value));
                        Ok(())
                    }),
                );
                exchange.base.note_http_response(
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
                        events.borrow()[0][1]["sentryData"]["error"].clone()
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
                let after = if coercion {
                    assert!(
                        runtime.reports.borrow().is_empty(),
                        "native coercion rejects before reporting"
                    );
                    exchange
                        .base
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
                    "event":events.borrow()[0][0],"reported":reported,"positionReads":store.position_reads.get(),"after":after,"report":report}));
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
            "/tests/reporting_values_oracle.mjs"
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

#[test]
fn mexc_raw_reporting_listener_failure_prevents_fallback() {
    let mut observations = vec![];
    for worker in [false, true] {
        for method in ["positions", "validate", "account"] {
            let store = Rc::new(history_store::ObservedStore::new());
            let runtime = Rc::new(ReportingRuntime {
                worker,
                consoles: RefCell::default(),
                reports: RefCell::default(),
                trace: Rc::default(),
            });
            let failure = Rc::new(ReportingFailure {
                gets: Cell::new(0),
                trace: Some(runtime.trace.clone()),
                failure: AccountFailure {
                    message: "transport".into(),
                    response_data: Some(json!({"message":7})),
                },
            });
            let mut exchange = venue(Rc::new(DefaultStore::new()), 100., false);
            exchange.base = Rc::new(BaseExchange::new(
                ExchangeName::Mexc,
                serde_json::from_value(json!({"key":"key","secret":"secret"})).unwrap(),
                store.clone(),
                runtime.clone(),
                ExchangeSettings::default(),
            ));
            exchange.xhr = failure.clone();
            let mut original = ExchangeError::new("listener failed");
            original.code = Some(safe_cex_exchange::js_runtime::value::from_json(&json!(
                "LISTENER"
            )));
            original.origin = Some(safe_cex_exchange::runtime::ExchangeErrorOrigin::new(
                std::io::Error::other("listener origin"),
            ));
            let first = Rc::new(Cell::new(0));
            let second = Rc::new(Cell::new(0));
            let first_calls = first.clone();
            let trace = runtime.trace.clone();
            let thrown = original.clone();
            exchange.base.on_args(
                "error",
                Rc::new(move |_, args| {
                    let args: Vec<serde_json::Value> = args
                        .iter()
                        .map(|value| safe_cex_exchange::js_runtime::value::to_json(value).unwrap())
                        .collect();
                    first_calls.set(first_calls.get() + 1);
                    trace.borrow_mut().push("first".into());
                    assert_eq!(args[0], "7", "native decoded listener raw formatted value");
                    if worker {
                        assert_source_tree(
                            &json!(7),
                            &args[1]["sentryData"]["error"],
                            "native worker raw report before listener",
                        );
                    }
                    Err(thrown.clone())
                }),
            );
            let second_calls = second.clone();
            exchange.base.on(
                "error",
                Rc::new(move |_, _| {
                    second_calls.set(second_calls.get() + 1);
                    Ok(())
                }),
            );
            let result = match method {
                "positions" => {
                    futures::executor::block_on(exchange.fetch_positions(None)).map(|_| ())
                }
                "validate" => futures::executor::block_on(exchange.validate_account()).map(|_| ()),
                "account" => futures::executor::block_on(exchange.get_account()).map(|_| ()),
                _ => unreachable!(),
            };
            assert!(
                result.is_err(),
                "native listener rejection propagates before fallback {method}"
            );
            let error = result.unwrap_err();
            assert_eq!(
                error, original,
                "native same structured listener error and origin through {method}"
            );
            assert_eq!(
                (
                    first.get(),
                    second.get(),
                    failure.gets.get(),
                    store.position_reads.get()
                ),
                (1, 0, 1, 0),
                "native independent throwing-listener child floors"
            );
            assert_eq!(
                runtime.reports.borrow().len(),
                usize::from(!worker),
                "native main raw reporting before listener"
            );
            if !worker {
                assert_eq!(
                    runtime.reports.borrow()[0]["error"],
                    7,
                    "native raw main report type before listener"
                );
            }
            let expected = if worker {
                vec!["get", "error", "trace", "first"]
            } else {
                vec!["get", "error", "trace", "report", "first"]
            };
            assert_eq!(
                *runtime.trace.borrow(),
                expected,
                "native exact throwing-listener call order"
            );
            observations.push(json!({"worker":worker,"method":method,"error":{"name":error.name,"message":error.message,"code":observed_error_code(&error.code)},
                "trace":*runtime.trace.borrow(),"gets":failure.gets.get(),"positionReads":store.position_reads.get()}));
        }
    }
    assert_eq!(
        observations.len(),
        6,
        "native independent throwing-listener root floor"
    );
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/reporting_values_oracle.mjs"
        ))
        .arg("--listeners")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "SOURCE actual throwing-listener path: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_source_tree(&source, &Value::Array(observations), "listener");
}

fn observed_error_code(
    code: &Option<safe_cex_exchange::js_runtime::value::Js>,
) -> Option<serde_json::Value> {
    code.as_ref().map(|value| {
        safe_cex_exchange::js_runtime::value::to_json(value).expect("finite JSON code fixture")
    })
}
